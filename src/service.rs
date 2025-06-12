#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

use std::{
    collections::HashSet,
    io::Read,
    path::PathBuf,
    sync::{Arc, MutexGuard},
};

use android_auto::{
    AndroidAutoAudioInputTrait, AndroidAutoAudioOutputTrait, AndroidAutoInputChannelTrait,
    AndroidAutoWirelessTrait, HeadUnitInfo, NetworkInformation,
};
use bluetooth_rust::BluetoothAdapterTrait;
use tokio::io::AsyncReadExt;
use uobradio_comms::{aauto::AndroidAutoMessageFromPhone, NonvolatileSettings, WifiConfig};
use video_service::VideoSource;
use wifi_manage::WifiAdapterTrait;

mod video_service;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

/// The optional command line arguments for the service
#[derive(clap::Parser, Debug)]
struct Arguments {
    /// Specify the actual location for the non-volatile configuratio file
    #[arg(long)]
    nvconfig: Option<PathBuf>,
}

/// System specific settings (not set by the user)
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct SystemSettings {
    /// The gpio setup for all the lights in the system
    lights: Vec<(String, u32)>,
    /// The gpio setup for auxilliary outputs
    aux_outs: Vec<(String, u32)>,
    /// The gpio setup for auxilliary inputs
    aux_ins: Vec<(String, u32)>,
}

impl SystemSettings {
    /// Load the system settings from the current directory
    pub fn load() -> Self {
        let p = std::path::Path::new("./settings.toml");
        let f = std::fs::File::open(p);
        if let Ok(mut f) = f {
            let mut a = String::new();
            if f.read_to_string(&mut a).is_ok() {
                match toml::from_str(&a) {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Config file {:?} is invalid: {:?}", p.display(), e);
                        Default::default()
                    }
                }
            } else {
                Self::default()
            }
        } else {
            log::error!("Config file {:?} not found", p.display());
            Self::default()
        }
    }
}

/// The structure for starting and stopping the android auto service
#[cfg(feature = "androidauto")]
struct AndroidAutoService {
    /// Determines who deals with the android-auto stuff
    addr: std::net::SocketAddr,
    /// The task list of tasks running to make the android auto service work
    tasks: tokio::task::JoinSet<Result<(), String>>,
    /// Used to send messages to the android auto library
    sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
    /// Used to receive android auto messages from a users device
    recv: tokio::sync::mpsc::Receiver<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
}

impl Drop for AndroidAutoService {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

impl AndroidAutoService {
    /// Construct and start an android auto service
    pub async fn new(com: &AppUserCommon, addr: std::net::SocketAddr) -> Result<Self, String> {
        if com.aa_network.is_none() {
            return Err("No wireless network details defined".to_string());
        }

        let mut tasks = tokio::task::JoinSet::new();

        let aautochan = tokio::sync::mpsc::channel(5);

        let blue_addresses: Vec<[u8; 6]> = com.bluetooth.addresses().await;
        let bluetooth_address = blue_addresses.first().map(|b| {
            let a = format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                b[0], b[1], b[2], b[3], b[4], b[5]
            );
            android_auto::BluetoothInformation { address: a }
        });

        #[cfg(feature = "androidauto")]
        let android_auto_server = android_auto::AndroidAutoServer::new().await;

        let config = android_auto::AndroidAutoConfiguration {
            unit: HeadUnitInfo {
                name: "UobRadio".to_string(),
                car_model: "Cherokee".to_string(),
                car_year: "1995".to_string(),
                car_serial: "42".to_string(),
                left_hand: true,
                head_manufacturer: "Uob".to_string(),
                head_model: "XJ1".to_string(),
                sw_build: "0".to_string(),
                sw_version: "1".to_string(),
                native_media: true,
                hide_clock: Some(false),
            },
            custom_certificate: None,
        };

        let aa_chan = tokio::sync::mpsc::channel(10);
        let main = AndroidAutoStuff::new(
            aautochan.0,
            aa_chan.1,
            aa_chan.0.clone(),
            com.bluetooth.clone(),
            com.aa_network.clone().unwrap(),
            bluetooth_address,
        );
        android_auto_server
            .run(config, &mut tasks, main)
            .await
            .inspect_err(|_| {
                log::error!("Failure starting up android auto service");
                tasks.abort_all();
            })?;
        Ok(Self {
            addr,
            tasks,
            sender: aa_chan.0,
            recv: aautochan.1,
        })
    }
}

/// The common data for an app user
pub struct AppUserCommon {
    /// The command line arguments specify any additional options required
    args: Arguments,
    #[cfg(feature = "androidauto")]
    /// The android auto service
    aauto_service: Option<AndroidAutoService>,
    /// The system specific (not user set) settings.
    system: SystemSettings,
    /// The network details for android auto
    #[cfg(feature = "androidauto")]
    aa_network: Option<NetworkInformation>,
    #[cfg(feature = "wifi")]
    /// Used for wifi operations
    wifi: Option<std::sync::Arc<wifi_manage::WifiAdapter>>,
    #[cfg(feature = "wifi")]
    /// The wifi setup
    wifi_setup: Option<uobradio_comms::WifiMode>,
    #[cfg(feature = "bluetooth")]
    /// The main bluetooth struct
    bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
    #[cfg(feature = "bluetooth")]
    /// Used to receive messages to the bluetooth host
    blue_recv: tokio::sync::mpsc::Receiver<bluetooth_rust::MessageToBluetoothHost>,
    #[cfg(feature = "bluetooth")]
    /// Determines who deals with the bluetooth stuff
    blue_addr: Option<std::net::SocketAddr>,
    /// The video sources in the system
    video: Vec<VideoSource>,
    /// The old nonvolatile settings of the radio, used to see if settings should be saved
    old_settings: NonvolatileSettings,
    /// The nonvolatile settings of the radio
    settings: NonvolatileSettings,
}

/// Performs the creation of a managed wifi hotspot, and also starts it up.
#[cfg(feature = "wifi")]
fn create_hotspot(
    wifi: &wifi_manage::WifiAdapter,
    name: &String,
    password: &String,
) -> Option<wifi_manage::WifiHotspot> {
    log::info!("Attempting to create hotspot {:?} {:?}", name, password);
    let mut a = None;
    let mut times = 0;
    loop {
        use wifi_manage::WifiAdapterTrait;
        a = wifi.build_hotspot("Hotspot", name, password).ok();
        if a.is_some() {
            break;
        }
        times += 1;
        if times == 5 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    a
}

/// Iterate over all known wifi networks, trying to connect in order if they are detected
#[cfg(feature = "wifi")]
fn iterate_over_networks(wifi: &wifi_manage::WifiAdapter, networks: &Vec<(String, String)>) -> Option<wifi_manage::WifiConnection> {
    let wifis = wifi.scan_for_networks();
    for (ssid, password) in networks {
        for w in &wifis {
            if w.name == *ssid {
                return wifi.connect_to_network(ssid, ssid, password).ok();
            }
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
/// Processes a tcp connection from an app
pub async fn process_app(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
) -> Result<(), String> {
    use bluetooth_rust::MessageFromBluetoothHost;
    use std::collections::BTreeMap;
    use tokio::io::AsyncReadExt;
    use uobradio_comms::MessageToApp;

    log::info!("Processing an app at {:?}", addr);

    let (mut streamr, streamw) = stream.into_split();

    let streamw = std::sync::Arc::new(tokio::sync::Mutex::new(streamw));

    let mut send_passkey_response = None;

    loop {
        let length = streamr
            .read_u32()
            .await
            .map_err(|e| format!("Error reading packet length: {}", e))?;
        let mut packet = vec![0; length as usize];
        let mut index = 0;
        while index < length {
            let l = streamr
                .read(&mut packet[index as usize..])
                .await
                .map_err(|e| format!("Error reading packet data: {}", e))?;
            index += l as u32;
        }
        let packet: Result<(uobradio_comms::MessageFromApp, usize), bincode::error::DecodeError> =
            bincode::serde::decode_from_slice(&packet, bincode::config::standard());
        if let Ok((packet, _length)) = packet {
            {
                let mut common2 = common.lock().await;
                if common2.blue_addr.is_some() {
                    while let Ok(m) = common2.blue_recv.try_recv() {
                        match &m {
                            bluetooth_rust::MessageToBluetoothHost::DisplayPasskey(_, sender) => {
                                send_passkey_response = Some(sender.clone());
                            }
                            bluetooth_rust::MessageToBluetoothHost::ConfirmPasskey(_, sender) => {
                                send_passkey_response = Some(sender.clone());
                            }
                            bluetooth_rust::MessageToBluetoothHost::CancelDisplayPasskey => {
                                log::info!("Cancel display passkey");
                                send_passkey_response.take();
                            }
                        }
                        let packet = MessageToApp::BluetoothMessage(m.into());
                        packet.send_to_stream(&streamw).await?;
                        log::info!("Sent bluetooth message to bluetooth master");
                    }
                }
            }
            match packet {
                uobradio_comms::MessageFromApp::Ac(c) => {
                    match c {
                        uobradio_comms::AcControl::GetCurrentTemperature => {
                            log::info!("Stub for get current ac temperature, faking 72 degrees");
                            let packet = MessageToApp::Ac(uobradio_comms::AcResponse::CurrentTemperature(Some(72.0)));
                            packet.send_to_stream(&streamw).await?;
                        }
                        uobradio_comms::AcControl::SetAcTargetTemperature(t) => {
                            log::info!("Set target ac temperature to {}", t);
                        }
                        uobradio_comms::AcControl::SetHeatTargetTemperature(t) => {
                            log::info!("Set target heat temperature to {}", t);
                        }
                        uobradio_comms::AcControl::SetFanSpeed(f) => {
                            log::info!("Set ac fan speed to {}", f);
                        }
                    }
                }
                uobradio_comms::MessageFromApp::GetWifiDetails => {
                    let common2 = common.lock().await;
                    if let Some(wifi) = &common2.wifi_setup {
                        match wifi {
                            uobradio_comms::WifiMode::Hotspot(wifi_hotspot) => {
                                use wifi_manage::WifiHotspotTrait;
                                let ssid = wifi_hotspot.ssid();
                                let password = wifi_hotspot.password();
                                let packet = MessageToApp::WifiDetails { ssid, password };
                                packet.send_to_stream(&streamw).await?;
                            }
                            uobradio_comms::WifiMode::RegularNetwork(wifi_connection) => {
                                use wifi_manage::WifiConnectionTrait;
                                let ssid = wifi_connection.ssid();
                                let password = wifi_connection.password();
                                let packet = MessageToApp::WifiDetails { ssid, password };
                                packet.send_to_stream(&streamw).await?;
                            }
                        }
                    }
                }
                uobradio_comms::MessageFromApp::ConnectToNetwork(ssid, password) => {
                    let wifi = {
                        let mut common2 = common.lock().await;
                        common2.wifi_setup.take();
                        common2.wifi.as_ref().map(|wifi| wifi.clone())
                    };
                    let common2 = common.clone();
                    let stream2w = streamw.clone();
                    tokio::task::spawn(async move {
                        if let Some(wifi) = wifi {
                            if let Some(p) = password {
                                log::info!("Start connect to wifi {}", ssid);
                                let ssid2 = ssid.clone();
                                let p2 = p.clone();
                                let a = tokio::task::spawn_blocking(move || {
                                    wifi.connect_to_network(&ssid2, &ssid2, &p2)
                                }).await.expect("Failed to run task to connect to wifi");
                                match a {
                                    Ok(wifi) => {
                                        log::info!("Connected to wifi network {}", ssid);
                                        let mut common2 = common2.lock().await;
                                        common2.wifi_setup =
                                            Some(uobradio_comms::WifiMode::RegularNetwork(wifi));
                                        common2.settings.wifi_network.push((ssid.clone(), p.clone()));
                                        common2.settings.save(&common2.args.nvconfig);
                                        let packet = MessageToApp::ConnectedToWifiNetwork { ssid, password: p, };
                                        packet.send_to_stream(&stream2w).await?;
                                    }
                                    Err(e) => {
                                        log::error!("Error connecting to {}: {:?}", ssid, e);
                                        let packet = MessageToApp::FailedToConnectToWifiNetwork { ssid: ssid, };
                                        packet.send_to_stream(&stream2w).await?;
                                    }
                                }
                            }
                            else {
                                log::error!("No password for wifi defined");
                            }
                        }
                        else {
                            log::error!("No wifi adapter found?");
                        }
                        Ok::<(), String>(())
                    });
                }
                uobradio_comms::MessageFromApp::ScanForWifiNetworks => {
                    let wifi = {
                        let mut common2 = common.lock().await;
                        common2.wifi_setup.take();
                        common2.wifi.as_ref().map(|wifi| wifi.clone())
                    };
                    let stream2w = streamw.clone();
                    tokio::task::spawn(async move {
                        if let Some(wifi) = wifi {
                            log::info!("Scanning for wifi networks");
                            let wifis = tokio::task::spawn_blocking(move || {
                                wifi.scan_for_networks()
                            }).await.expect("Failed to run task to scan for wifi");
                            log::info!("Done scanning for wifi networks");
                            let packet = MessageToApp::WifiList(wifis);
                            packet.send_to_stream(&stream2w).await?;
                        }
                        Ok::<(), String>(())
                    });
                }
                uobradio_comms::MessageFromApp::ExternalRadio(rc) => match rc {
                    uobradio_comms::RadioCommand::StartTransmission => {
                        log::info!("Start external radio transmission");
                    }
                    uobradio_comms::RadioCommand::StopTransmission => {
                        log::info!("Stop external radio transmission");
                    }
                    uobradio_comms::RadioCommand::TransmissionDataPartial(d) => {
                        log::info!("Process {} bytes of radio transmission data", d.len());
                    }
                },
                uobradio_comms::MessageFromApp::AndroidAutoMessage(m) => match m {
                    uobradio_comms::aauto::AndroidAutoMessageToPhone::Test => todo!(),
                    uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(m) => {
                        let mut common2 = common.lock().await;
                        if let Some(aauto) = &common2.aauto_service {
                            if addr == aauto.addr {
                                if let Err(e) = aauto.sender.send(m).await {
                                    let m = uobradio_comms::aauto::AndroidAutoMessageFromPhone::Disconnect;
                                    let packet = MessageToApp::AndroidAutoMessage(m);
                                    packet.send_to_stream(&streamw).await?;
                                    common2.aauto_service.take();
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    common2.aauto_service =
                                        AndroidAutoService::new(&common2, addr).await.ok();
                                }
                            }
                        }
                    }
                },
                uobradio_comms::MessageFromApp::RequestAndroidAutoControl => {
                    let mut common = common.lock().await;
                    let r = if common.aauto_service.is_none() {
                        log::info!("Setting {:?} as android auto master", addr);
                        let r = AndroidAutoService::new(&common, addr).await;
                        if let Err(e) = &r {
                            log::error!("Error starting android auto service: {}", e);
                        }
                        common.aauto_service = r.ok();
                        common.aauto_service.is_some()
                    } else {
                        false
                    };
                    let packet = uobradio_comms::MessageToApp::AndroidAutoHandlerResult(r);
                    packet.send_to_stream(&streamw).await?;
                }
                uobradio_comms::MessageFromApp::BluetoothMessage(m) => {
                    let common2 = common.lock().await;
                    if Some(addr) == common2.blue_addr {
                        if let MessageFromBluetoothHost::PasskeyMessage(m) = m {
                            if let Some(sender) = &send_passkey_response {
                                if sender.send(m).await.is_err() {
                                    let m = uobradio_comms::ActualMessageToBluetoothHost::CancelDisplayPasskey;
                                    let packet = MessageToApp::BluetoothMessage(m);
                                    packet.send_to_stream(&streamw).await?;
                                }
                            }
                        }
                    }
                }
                uobradio_comms::MessageFromApp::SetBluetoothDiscovery(val) => {
                    let common2 = common.lock().await;
                    if Some(addr) == common2.blue_addr {
                        if common2.bluetooth.set_discoverable(val).await.is_ok() {
                            let a =
                                uobradio_comms::ActualMessageToBluetoothHost::BluetoothEnabled(val);
                            let packet = uobradio_comms::MessageToApp::BluetoothMessage(a);
                            packet.send_to_stream(&streamw).await?;
                        } else {
                            log::error!("Failed to change bluetooth discoverable to {}", val);
                        }
                    }
                }
                uobradio_comms::MessageFromApp::RequestBluetoothControl => {
                    let mut common = common.lock().await;
                    let r = if common.blue_addr.is_none() {
                        log::info!("Setting {:?} as bluetooth master", addr);
                        common.blue_addr = Some(addr);
                        true
                    } else {
                        false
                    };
                    let packet = uobradio_comms::MessageToApp::BluetoothHandlerResult(r);
                    log::info!("Sending bluetooth response {:?}", packet);
                    packet.send_to_stream(&streamw).await?;
                }
                uobradio_comms::MessageFromApp::RequestSettings => {
                    let common2 = common.lock().await;
                    let packet =
                        uobradio_comms::MessageToApp::NewSettings(common2.settings.clone());
                    packet.send_to_stream(&streamw).await?;
                }
                uobradio_comms::MessageFromApp::NewSettings{ settings, wifi_reconnect } => {
                    let mut common2 = common.lock().await;
                    common2.settings = settings;
                    common2.settings.save(&common2.args.nvconfig);
                    let mut wifi_changed = false;
                    if common2.old_settings.hotspot_enabled != common2.settings.hotspot_enabled {
                        common2.old_settings.hotspot_enabled =
                            common2.settings.hotspot_enabled.clone();
                        wifi_changed = true;
                    }
                    if common2.old_settings.wifi_config != common2.settings.wifi_config {
                        common2.old_settings.wifi_config = common2.settings.wifi_config.clone();
                        wifi_changed = true;
                    }
                    if common2.old_settings.wifi_network != common2.settings.wifi_network {
                        common2.old_settings.wifi_network = common2.settings.wifi_network.clone();
                        if wifi_reconnect {
                            wifi_changed = true;
                        }
                    }
                    #[cfg(feature = "wifi")]
                    if wifi_changed {
                        setup_wifi(common2);
                    }
                }
                uobradio_comms::MessageFromApp::CameraSettingControl(id, control, data) => {
                    let a: uobradio_comms::v4l::control::Value = data.into();
                    let mut common2 = common.lock().await;
                    if let Some(vid) = common2.video.get_mut(id as usize) {
                        let _ = vid.send_update(control as usize, &a);
                        vid.controls[control as usize].value = a;
                    }
                }
                uobradio_comms::MessageFromApp::RequestCameras => {
                    let common2 = common.lock().await;
                    let mut map = BTreeMap::new();
                    for (i, cam) in common2.video.iter().enumerate() {
                        if let Some(c) = cam.sendable() {
                            map.insert(i as u8, c);
                        }
                    }
                    let packet = uobradio_comms::MessageToApp::CamerasBtreeMap(map);
                    packet.send_to_stream(&streamw).await?;
                }
                uobradio_comms::MessageFromApp::Ping(id) => {
                    let packet = uobradio_comms::MessageToApp::PingReply(id);
                    packet.send_to_stream(&streamw).await?;
                }
                uobradio_comms::MessageFromApp::RequestCamera(index) => {
                    let common2 = common.lock().await;
                    if let Some(v) = common2.video.get(index as usize) {
                        let jpeg = {
                            let frame = v.image.lock().unwrap();
                            frame.get_jpeg()
                        };
                        let response = uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg);
                        response.send_to_stream(&streamw).await?;
                    }
                }
                uobradio_comms::MessageFromApp::GpioControl(gpio) => match gpio {
                    uobradio_comms::Gpio::AuxOutput(id, v) => {
                        log::info!("Set aux output {} to {}", id, v);
                    }
                    uobradio_comms::Gpio::GetAuxInput(id) => {
                        log::info!("Request for aux input {}", id);
                    }
                    uobradio_comms::Gpio::InverterPower(p) => {
                        log::info!("Set inverter power to {}", p);
                    }
                    uobradio_comms::Gpio::LightControl(id, v) => {
                        log::info!("Set light output {} to {}", id, v);
                    }
                    uobradio_comms::Gpio::WinchControl(f, r) => {
                        log::info!("Winch control {} {}", f, r)
                    }
                    uobradio_comms::Gpio::CameraLedControl(i, s) => {
                        log::info!("Camera led {} to {}", i, s)
                    }
                    uobradio_comms::Gpio::LockDoors => {
                        log::info!("Received request to lock all doors")
                    }
                    uobradio_comms::Gpio::UnlockDoors => {
                        log::info!("Recieved request to unlock all doors")
                    }
                    uobradio_comms::Gpio::WindowControl { id, up, down } => {
                        log::info!("Window {} {}/{}", id, up, down)
                    }
                },
            }
            {
                let mut common2 = common.lock().await;
                if let Some(aauto) = &mut common2.aauto_service {
                    while let Ok(m) = aauto.recv.try_recv() {
                        let packet = MessageToApp::AndroidAutoMessage(m);
                        packet.send_to_stream(&streamw).await?;
                    }
                }
            }
        } else {
            log::error!("Failed to process packet");
            return Err("Received bad packet".to_string());
        }
    }
}

/// Start the udp listener, responsible for making a radio discoverable on the network.
async fn udp_listener(_common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    log::info!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        log::info!("Waiting for a udp client");
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            log::info!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                if let uobradio_comms::MessageFromApp::Ping(val) = packet {
                    log::info!("got ping packet {}", val);
                    let response = bincode::serde::encode_to_vec(
                        uobradio_comms::MessageToApp::PingReply(13457),
                        bincode::config::standard(),
                    )
                    .unwrap();
                    let _ = socket.send_to(&response, addr).await;
                }
            } else {
                log::info!("invalid packet received {:x?}", response);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Run the tcp listener for a radio, reporting an error if anything went wront setting up the service
async fn tcp_listener(common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    let mut set = tokio::task::JoinSet::new();
    if let Ok(tcp) = tcp {
        loop {
            log::info!("Waiting for a tcp client");
            if let Ok((stream, addr)) = tcp.accept().await {
                let common2 = common.clone();
                set.spawn(async move {
                    log::info!("Got a tcp client {:?}", addr);
                    let r = process_app(stream, addr, common2.clone()).await;
                    let mut common3 = common2.lock().await;
                    if Some(addr) == common3.blue_addr {
                        log::info!("Setting {:?} as no longer the bluetooth master", addr);
                        common3.blue_addr.take();
                    }
                    if let Some(aauto) = &common3.aauto_service {
                        if addr == aauto.addr {
                            log::info!("Setting {:?} as no longer the android auto master", addr);
                            common3.aauto_service.take();
                        }
                    }
                    log::info!("Completed handling user {:?}", r);
                    r
                });
            }
        }
    } else {
        panic!("Unable to open tcp listener to listen for apps connecting");
    }
}

/// An internally used structure for sending messages between the android auto user and the frontend
struct InternalAndroidAutoStuff {
    /// Used internally to relay android auto messages from the users phone
    sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
    /// Temporary storage for the android auto crate to use to send us messages
    recvr: Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>>,
    /// Used for sending responses to the android auto crate
    frame_sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
}

/// Stores communication links for android auto
#[derive(Clone)]
struct AndroidAutoStuff {
    /// The protected internals
    inner: Arc<tokio::sync::Mutex<InternalAndroidAutoStuff>>,
    /// The bluetooth reference
    bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
    /// This is defined if there is actually a bluetooth adapter present
    bluetooth_config: Option<android_auto::BluetoothInformation>,
    /// The network information
    network: Arc<android_auto::NetworkInformation>,
    /// The input channel config
    input_config: android_auto::InputConfiguration,
    /// The video channel config
    video_config: android_auto::VideoConfiguration,
    /// The sensors config
    sensors: android_auto::SensorInformation,
}

impl AndroidAutoStuff {
    /// Construct a new self
    /// #Arguments
    /// sendr: The channel for sending responses from the user device to the android auto handler
    /// recvr: The channel that receives android auto messages to be sent back to the phone
    /// frame_sender: The channel that sends android auto messages to the user device
    /// bluetooth: The bluetooth adapter to use
    /// network: The network details to use for android auto
    /// bluetooth_config: Contains the bluetooth configuration
    pub fn new(
        sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
        recvr: tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>,
        frame_sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
        bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
        network: android_auto::NetworkInformation,
        bluetooth_config: Option<android_auto::BluetoothInformation>,
    ) -> Self {
        let inner = InternalAndroidAutoStuff {
            sendr,
            recvr: Some(recvr),
            frame_sender,
        };
        let mut s = HashSet::new();
        s.insert(android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS);
        s.insert(android_auto::Wifi::sensor_type::Enum::NIGHT_DATA);
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            bluetooth,
            network: Arc::new(network),
            input_config: android_auto::InputConfiguration {
                touchscreen: Some((800, 480)),
                keycodes: vec![1, 2, 3, 4, 5],
            },
            video_config: android_auto::VideoConfiguration {
                resolution: android_auto::Wifi::video_resolution::Enum::_480p,
                fps: android_auto::Wifi::video_fps::Enum::_30,
                dpi: 111,
            },
            sensors: android_auto::SensorInformation { sensors: s },
            bluetooth_config,
        }
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoAudioOutputTrait for AndroidAutoStuff {
    async fn open_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelOpen(t))
            .await;
        Ok(())
    }

    async fn close_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelClose(t))
            .await;
        Ok(())
    }

    async fn receive_audio(&self, t: android_auto::AudioChannelType, data: Vec<u8>) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioContent(t, data))
            .await;
    }

    async fn start_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelStart(t))
            .await;
    }

    async fn stop_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelStop(t))
            .await;
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoInputChannelTrait for AndroidAutoStuff {
    async fn binding_request(&self, _code: u32) -> Result<(), ()> {
        Ok(())
    }

    fn retrieve_input_configuration(&self) -> &android_auto::InputConfiguration {
        &self.input_config
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoAudioInputTrait for AndroidAutoStuff {
    async fn open_channel(&self) -> Result<(), ()> {
        Ok(())
    }
    async fn close_channel(&self) -> Result<(), ()> {
        Ok(())
    }
    async fn start_audio(&self) {
        log::error!("Start audio input channel");
    }
    async fn stop_audio(&self) {
        log::error!("Stop audio input channel");
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoWirelessTrait for AndroidAutoStuff {
    async fn setup_bluetooth_profile(
        &self,
        suggestions: &bluetooth_rust::BluetoothRfcommProfileSettings,
    ) -> Result<bluetooth_rust::BluetoothRfcommProfile, String> {
        self.bluetooth
            .register_rfcomm_profile(suggestions.clone())
            .await
    }

    fn get_wifi_details(&self) -> android_auto::NetworkInformation {
        self.network.as_ref().to_owned()
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoMainTrait for AndroidAutoStuff {
    fn supports_video(&self) -> Option<&dyn android_auto::AndroidAutoVideoChannelTrait> {
        Some(self)
    }

    fn supports_bluetooth(&self) -> Option<&dyn android_auto::AndroidAutoBluetoothTrait> {
        if self.bluetooth_config.is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn supports_wireless(&self) -> Option<Arc<dyn AndroidAutoWirelessTrait>> {
        Some(Arc::new(self.clone()))
    }

    fn supports_input(&self) -> Option<&dyn AndroidAutoInputChannelTrait> {
        Some(self)
    }

    fn supports_audio_output(&self) -> Option<&dyn AndroidAutoAudioOutputTrait> {
        Some(self)
    }

    fn supports_audio_input(&self) -> Option<&dyn AndroidAutoAudioInputTrait> {
        Some(self)
    }

    fn supports_sensors(&self) -> Option<&dyn android_auto::AndroidAutoSensorTrait> {
        Some(self)
    }

    async fn get_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>> {
        let mut s = self.inner.lock().await;
        s.recvr.take()
    }

    async fn connect(&self) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::Connect).await;
    }

    async fn disconnect(&self) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::Disconnect).await;
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoBluetoothTrait for AndroidAutoStuff {
    async fn do_stuff(&self) {}
    /// This is probably fine because the supports_bluetooth function already checked this
    /// Removing the bluetooth adapter while the code is running might be problematic here
    fn get_config(&self) -> &android_auto::BluetoothInformation {
        self.bluetooth_config.as_ref().unwrap()
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoSensorTrait for AndroidAutoStuff {
    fn get_supported_sensors(&self) -> &android_auto::SensorInformation {
        &self.sensors
    }

    async fn start_sensor(&self, stype: android_auto::Wifi::sensor_type::Enum) -> Result<(), ()> {
        if self.sensors.sensors.contains(&stype) {
            let mut m3 = android_auto::Wifi::SensorEventIndication::new();
            match stype {
                android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS => {
                    let mut ds = android_auto::Wifi::DrivingStatus::new();
                    ds.set_status(android_auto::Wifi::DrivingStatusEnum::UNRESTRICTED as i32);
                    m3.driving_status.push(ds);
                }
                android_auto::Wifi::sensor_type::Enum::NIGHT_DATA => {
                    let mut ds = android_auto::Wifi::NightMode::new();
                    ds.set_is_night(false);
                    m3.night_mode.push(ds);
                }
                _ => {
                    todo!();
                }
            }
            let s = self.inner.lock().await;
            let m = android_auto::AndroidAutoMessage::Sensor(m3);
            s.frame_sender.send(m.sendable()).await.map_err(|_| ())?;
            Ok(())
        } else {
            Err(())
        }
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoVideoChannelTrait for AndroidAutoStuff {
    async fn receive_video(&self, data: Vec<u8>, _timestamp: Option<u64>) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::VideoContent(data))
            .await;
    }

    async fn setup_video(&self) -> Result<(), ()> {
        Ok(())
    }

    async fn teardown_video(&self) {}

    async fn wait_for_focus(&self) {}

    async fn set_focus(&self, _focus: bool) {}

    fn retrieve_video_configuration(&self) -> &android_auto::VideoConfiguration {
        &self.video_config
    }
}

/// Sets up the wifi hardware according to settings
/// Call this when initially setting up wifi, or when changing wifi settings
/// Wifi connections will likey drop and reconnect
fn setup_wifi(mut common2: tokio::sync::MutexGuard<AppUserCommon>) {
    log::info!("Setup wifi with {:?}", common2.settings.wifi_config);
    match &common2.settings.wifi_config {
        WifiConfig::Ready => {
            common2.wifi_setup.take();
        }
        WifiConfig::Hotspot => {
            let hotspot = common2.settings.hotspot_enabled.clone();
            if let Some((n, p)) = hotspot {
                common2.wifi_setup.take();
                if let Some(wifi) = &common2.wifi {
                    common2.wifi_setup =
                        create_hotspot(wifi, &n, &p).map(uobradio_comms::WifiMode::Hotspot);
                } else {
                    log::error!("Wifi is not present?");
                }
            }
        }
        WifiConfig::RegularNetwork => {
            if let Some(wifi) = &common2.wifi {
                let w = iterate_over_networks(wifi, &common2.settings.wifi_network);
                common2.wifi_setup.take();
                common2.wifi_setup = w.map(uobradio_comms::WifiMode::RegularNetwork);
            } else {
                log::error!("Wifi is not present?");
            }
        }
        WifiConfig::Disabled => {
            common2.wifi_setup.take();
        }
    }
}

/// The main function for the service
async fn smain() {
    #[cfg(target_family = "windows")]
    {
        std::panic::set_hook(Box::new(|p| {
            service::log::debug!("Panic {:?}", p);
        }))
    }

    let args = <Arguments as clap::Parser>::parse();
    android_auto::setup();

    let mut times_wifi = 0;
    let wifis = loop {
        let wifis = wifi_manage::get_wifi_adapters().unwrap();
        log::info!("Wifi NAMES:");
        for w in &wifis {
            log::info!("NAME: {}", w);
        }
        if !wifis.is_empty() {
            break wifis;
        }
        times_wifi += 1;
        if times_wifi == 5 {
            break Vec::new();
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    };
    let main_wifi = wifis.first();

    let f = tokio::fs::File::open("./service.toml").await;
    let settings = if let Ok(mut f) = f {
        let mut config_raw = Vec::new();
        f.read_to_end(&mut config_raw).await.unwrap();
        let config_str = String::from_utf8(config_raw).unwrap();
        toml::from_str(&config_str).unwrap()
    } else {
        MainConfiguration::default()
    };

    service::log::set_max_level(
        settings
            .debug_level
            .as_ref()
            .unwrap_or(&service::LogLevel::Trace)
            .level_filter(),
    );

    let (_shutdown_send, mut shutdown_recv) = tokio::sync::mpsc::unbounded_channel::<()>();

    let mut vs = Vec::new();
    if let Ok(d) = uobradio_comms::v4l::Device::new(0) {
        vs.push(video_service::Video::video_start(d));
    }
    let s = NonvolatileSettings::load(&args.nvconfig);
    let sys = SystemSettings::load();
    #[cfg(feature = "bluetooth")]
    let bluechan = tokio::sync::mpsc::channel(5);
    let mut bluetooth = bluetooth_rust::BluetoothAdapterBuilder::new();
    bluetooth.with_sender(bluechan.0);
    let bluetooth = Arc::new(bluetooth.build().await.expect("Could not open bluetooth"));

    use network_interface::NetworkInterfaceConfig;
    let network_interfaces = network_interface::NetworkInterface::show().unwrap();
    let mut wifi_mac = String::new();
    if let Some(wn) = &main_wifi {
        for i in &network_interfaces {
            log::info!("Mac address of {} is {:?}", i.name, i.mac_addr);
        }
    }
    if let Some(wn) = &main_wifi {
        for i in network_interfaces {
            if i.name == **wn {
                wifi_mac = i.mac_addr.unwrap();
            }
        }
    }

    let network = {
        s.hotspot_enabled
            .as_ref()
            .map(|a| android_auto::NetworkInformation {
                ssid: a.0.clone(),
                psk: a.1.clone(),
                mac_addr: wifi_mac,
                ip: "10.42.0.1".to_string(),
                port: 5277,
                security_mode: android_auto::Bluetooth::SecurityMode::WPA2_PERSONAL,
                ap_type: android_auto::Bluetooth::AccessPointType::STATIC,
            })
    };

    let common = Arc::new(tokio::sync::Mutex::new(AppUserCommon {
        args,
        #[cfg(feature = "wifi")]
        wifi: main_wifi.map(|m| Arc::new(wifi_manage::get_network_adapter(m).expect("Failed to setup wifi"))),
        #[cfg(feature = "androidauto")]
        aauto_service: None,
        #[cfg(feature = "androidauto")]
        aa_network: network,
        system: sys,
        #[cfg(feature = "wifi")]
        wifi_setup: None,
        #[cfg(feature = "bluetooth")]
        bluetooth: bluetooth.clone(),
        #[cfg(feature = "bluetooth")]
        blue_recv: bluechan.1,
        #[cfg(feature = "bluetooth")]
        blue_addr: None,
        video: vs,
        old_settings: s.clone(),
        settings: s.clone(),
    }));

    {
        let mut common2 = common.lock().await;
        if let Some(a) = common2.wifi.as_ref() { a.set_stay(); }
        setup_wifi(common2);
    }

    let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
    let common2 = common.clone();
    tasks.spawn(async move {
        udp_listener(common2)
            .await
            .inspect_err(|a| log::error!("Radio tcp listener ended: {:?}", a))
    });
    let common2 = common.clone();
    tasks.spawn(async move {
        tcp_listener(common2)
            .await
            .inspect_err(|a| log::error!("Radio tcp listener ended: {:?}", a))
    });

    tokio::select! {
        r = tasks.join_next() => {
            service::log::error!("A task exited {:?}, closing server in 5 seconds", r);
            tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
        }
        _ = tokio::signal::ctrl_c() => {}
        _ = shutdown_recv.recv() => {}
    }
    service::log::error!("Closing server now");
}

service::ServiceAsyncMacro!(service_starter, smain, u64);

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), u32> {
    let service = service::Service::new("uobradio".to_string());
    service.new_log(service::LogLevel::Debug);
    if let Err(e) = service::DispatchAsync!(service, service_starter) {
        Err(e)
    } else {
        Ok(())
    }
}
