#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

use std::{collections::HashSet, io::Read, sync::Arc};

use android_auto::{AndroidAutoAudioOutputTrait, AndroidAutoInputChannelTrait, AndroidAutoWirelessTrait, HeadUnitInfo, NetworkInformation, SendableAndroidAutoMessage};
use bluetooth_rust::BluetoothAdapterTrait;
use tokio::io::AsyncReadExt;
use uobradio_comms::{aauto::AndroidAutoMessageFromPhone, NonvolatileSettings};
use video_service::VideoSource;

mod video_service;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

/// System specific settings (not set by the user)
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct SystemSettings {
    /// The name of the wifi adapter to use for wifi operations
    #[cfg(feature = "wifi")]
    wifi_name: String,
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
        let bluetooth_address = {
            let b = blue_addresses[0];
            format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                b[0], b[1], b[2], b[3], b[4], b[5]
            )
        };

        #[cfg(feature = "androidauto")]
        let android_auto_server = android_auto::AndroidAutoServer::new().await;

        let config = android_auto::AndroidAutoConfiguration {
            bluetooth: android_auto::BluetoothInformation {
                address: bluetooth_address,
            },
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
    wifi: wifi_rs::WiFi,
    #[cfg(feature = "wifi")]
    /// The optional wifi hotspot (if enabled by the user)
    hotspot: Option<wifi_rs::prelude::ManagedWifiHotspot>,
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
    wifi: &mut wifi_rs::WiFi,
    name: &String,
    password: &String,
) -> Option<wifi_rs::prelude::ManagedWifiHotspot> {
    use wifi_rs::prelude::ManagedWifiHotspotTrait;
    let configuration = wifi_rs::prelude::HotspotConfig::new(None, None);
    log::info!("Attempting to create hotspot {:?} {:?}", name, password);
    let mut a = wifi
        .create_managed_hotspot(name, password, Some(&configuration))
        .ok();
    if let Some(a) = &mut a {
        a.start_hotspot().ok()?;
    }
    a
}

#[cfg(not(target_os = "android"))]
/// Processes a tcp connection from an app
pub async fn process_app(
    mut stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
) -> Result<(), String> {
    use bluetooth_rust::MessageFromBluetoothHost;
    use std::collections::BTreeMap;
    use tokio::io::AsyncReadExt;
    use uobradio_comms::MessageToApp;

    println!("Processing an app at {:?}", addr);

    let mut send_passkey_response = None;

    loop {
        let length = stream
            .read_u32()
            .await
            .map_err(|e| format!("Error reading packet length: {}", e))?;
        let mut packet = vec![0; length as usize];
        stream
            .read_exact(&mut packet)
            .await
            .map_err(|e| format!("Error reading packet of length {}: {}", length, e))?;
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
                                println!("Cancel display passkey");
                                send_passkey_response.take();
                            }
                        }
                        let packet = MessageToApp::BluetoothMessage(m.into());
                        packet.send_to_stream(&mut stream).await?;
                        println!("Sent bluetooth message to bluetooth master");
                    }
                }
            }
            match packet {
                uobradio_comms::MessageFromApp::AndroidAutoMessage(m) => match m {
                    uobradio_comms::aauto::AndroidAutoMessageToPhone::Test => todo!(),
                    uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(m) => {
                        let mut common2 = common.lock().await;
                        if let Some(aauto) = &common2.aauto_service {
                            if addr == aauto.addr {
                                if let Err(e) = aauto.sender.send(m).await {
                                    log::error!("Closing android auto sender now: {:?}", e);
                                    let m = uobradio_comms::aauto::AndroidAutoMessageFromPhone::Disconnect;
                                    let packet = MessageToApp::AndroidAutoMessage(m);
                                    packet.send_to_stream(&mut stream).await?;
                                    common2.aauto_service.take();
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    common2.aauto_service = AndroidAutoService::new(&common2, addr).await.ok();
                                }
                            }
                        }
                    }
                },
                uobradio_comms::MessageFromApp::RequestAndroidAutoControl => {
                    let mut common = common.lock().await;
                    let r = if common.aauto_service.is_none() {
                        println!("Setting {:?} as android auto master", addr);
                        common.aauto_service = AndroidAutoService::new(&common, addr).await.ok();
                        common.aauto_service.is_some()
                    } else {
                        false
                    };
                    let packet = uobradio_comms::MessageToApp::AndroidAutoHandlerResult(r);
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::BluetoothMessage(m) => {
                    let common2 = common.lock().await;
                    if Some(addr) == common2.blue_addr {
                        if let MessageFromBluetoothHost::PasskeyMessage(m) = m {
                            if let Some(sender) = &send_passkey_response {
                                if sender.send(m).await.is_err() {
                                    let m = uobradio_comms::ActualMessageToBluetoothHost::CancelDisplayPasskey;
                                    let packet = MessageToApp::BluetoothMessage(m);
                                    packet.send_to_stream(&mut stream).await?;
                                }
                            }
                        }
                    }
                }
                uobradio_comms::MessageFromApp::SetBluetoothDiscovery(val) => {
                    let mut common2 = common.lock().await;
                    if Some(addr) == common2.blue_addr {
                        todo!();
                        //common2.bluetooth.set_discoverable(val).await;
                        let a = uobradio_comms::ActualMessageToBluetoothHost::BluetoothEnabled(val);
                        let packet = uobradio_comms::MessageToApp::BluetoothMessage(a);
                        packet.send_to_stream(&mut stream).await?;
                    }
                }
                uobradio_comms::MessageFromApp::RequestBluetoothControl => {
                    let mut common = common.lock().await;
                    let r = if common.blue_addr.is_none() {
                        println!("Setting {:?} as bluetooth master", addr);
                        common.blue_addr = Some(addr);
                        true
                    } else {
                        false
                    };
                    let packet = uobradio_comms::MessageToApp::BluetoothHandlerResult(r);
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::RequestSettings => {
                    let common2 = common.lock().await;
                    let packet =
                        uobradio_comms::MessageToApp::NewSettings(common2.settings.clone());
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::NewSettings(s) => {
                    let mut common2 = common.lock().await;
                    #[cfg(feature = "wifi")]
                    let mut change_hotspot = false;
                    common2.settings = s;
                    common2.settings.save();
                    #[cfg(feature = "wifi")]
                    if common2.old_settings.hotspot_enabled != common2.settings.hotspot_enabled {
                        common2.old_settings.hotspot_enabled =
                            common2.settings.hotspot_enabled.clone();
                        change_hotspot = true;
                    }
                    #[cfg(feature = "wifi")]
                    if change_hotspot {
                        let hotspot = common2.settings.hotspot_enabled.clone();
                        if let Some((n, p)) = hotspot {
                            common2.hotspot = create_hotspot(&mut common2.wifi, &n, &p);
                        } else {
                            common2.hotspot = None;
                        }
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
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::Ping(id) => {
                    let packet = uobradio_comms::MessageToApp::PingReply(id);
                    packet.send_to_stream(&mut stream).await?;
                }
                uobradio_comms::MessageFromApp::RequestCamera(index) => {
                    let common2 = common.lock().await;
                    if let Some(v) = common2.video.get(index as usize) {
                        let jpeg = {
                            let frame = v.image.lock().unwrap();
                            frame.get_jpeg()
                        };
                        let response = uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg);
                        response.send_to_stream(&mut stream).await?;
                    }
                }
                uobradio_comms::MessageFromApp::GpioControl(gpio) => match gpio {
                    uobradio_comms::Gpio::WinchControl(f, r) => {
                        println!("Winch control {} {}", f, r)
                    }
                    uobradio_comms::Gpio::CameraLedControl(i, s) => {
                        println!("Camera led {} to {}", i, s)
                    }
                    uobradio_comms::Gpio::LockDoors => {
                        println!("Received request to lock all doors")
                    }
                    uobradio_comms::Gpio::UnlockDoors => {
                        println!("Recieved request to unlock all doors")
                    }
                    uobradio_comms::Gpio::WindowControl { id, up, down } => {
                        println!("Window {} {}/{}", id, up, down)
                    }
                },
            }
            {
                let mut common2 = common.lock().await;
                if let Some(aauto) = &mut common2.aauto_service {
                    while let Ok(m) = aauto.recv.try_recv() {
                        let packet = MessageToApp::AndroidAutoMessage(m);
                        packet.send_to_stream(&mut stream).await?;
                    }
                }
            }
        } else {
            println!("Failed to process packet");
            return Err("Received bad packet".to_string());
        }
    }
}

/// Start the udp listener, responsible for making a radio discoverable on the network.
async fn udp_listener(_common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    println!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        log::info!("Waiting for a udp client");
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                if let uobradio_comms::MessageFromApp::Ping(val) = packet {
                    println!("got ping packet {}", val);
                    let response = bincode::serde::encode_to_vec(
                        uobradio_comms::MessageToApp::PingReply(13457),
                        bincode::config::standard(),
                    )
                    .unwrap();
                    let _ = socket.send_to(&response, addr).await;
                }
            } else {
                println!("invalid packet received {:x?}", response);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Run the tcp listener for a radio, reporting an error if anything went wront setting up the service
async fn tcp_listener(common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    if let Ok(tcp) = tcp {
        loop {
            log::info!("Waiting for a tcp client");
            if let Ok((stream, addr)) = tcp.accept().await {
                let common2 = common.clone();
                tokio::task::spawn(async move {
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
    pub fn new(
        sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
        recvr: tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>,
        frame_sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
        bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
        network: android_auto::NetworkInformation,
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
                keycodes: vec![1,2,3,4,5],
            },
            video_config: android_auto::VideoConfiguration { 
                resolution: android_auto::Wifi::video_resolution::Enum::_480p,
                fps: android_auto::Wifi::video_fps::Enum::_60, 
                dpi: 111,
            },
            sensors: android_auto::SensorInformation {
                sensors: s,
            }
        }
    }
}

#[async_trait::async_trait]
impl android_auto::AndroidAutoAudioOutputTrait for AndroidAutoStuff {
    async fn open_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::AudioChannelOpen(t)).await;
        Ok(())
    }

    async fn close_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::AudioChannelClose(t)).await;
        Ok(())
    }

    async fn receive_audio(&self, t: android_auto::AudioChannelType, data: Vec<u8>) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::AudioContent(t, data)).await;
    }

    async fn start_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::AudioChannelStart(t)).await;
    }

    async fn stop_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::AudioChannelStop(t)).await;
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

    fn supports_wireless(&self) -> Option<Arc<dyn AndroidAutoWirelessTrait>> {
        Some(Arc::new(self.clone()))
    }

    fn supports_input(&self) -> Option<&dyn AndroidAutoInputChannelTrait> {
        Some(self)
    }

    fn supports_audio_output(&self) -> Option<&dyn AndroidAutoAudioOutputTrait> {
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
impl android_auto::AndroidAutoSensorTrait for AndroidAutoStuff {
    fn get_supported_sensors(&self) ->  &android_auto::SensorInformation {
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
            s.frame_sender.send(m.sendable()).await.map_err(|_|())?;
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

/// The main function for the service
async fn smain() {
    #[cfg(target_family = "windows")]
    {
        std::panic::set_hook(Box::new(|p| {
            service::log::debug!("Panic {:?}", p);
        }))
    }

    android_auto::setup();

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
    let s = NonvolatileSettings::load();
    let sys = SystemSettings::load();
    #[cfg(feature = "bluetooth")]
    let bluechan = tokio::sync::mpsc::channel(5);
    let mut bluetooth = bluetooth_rust::BluetoothAdapterBuilder::new();
    bluetooth.with_sender(bluechan.0);
    let bluetooth = Arc::new(bluetooth.build().await.expect("Could not open bluetooth"));

    use network_interface::NetworkInterfaceConfig;
    let network_interfaces = network_interface::NetworkInterface::show().unwrap();
    let mut wifi_mac = String::new();
    for i in network_interfaces {
        if i.name == sys.wifi_name {
            wifi_mac = i.mac_addr.unwrap();
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
        #[cfg(feature = "wifi")]
        wifi: wifi_rs::WiFi::new(Some(wifi_rs::prelude::Config {
            interface: Some(&sys.wifi_name),
        })),
        #[cfg(feature = "androidauto")]
        aauto_service: None,
        #[cfg(feature = "androidauto")]
        aa_network: network,
        system: sys,
        #[cfg(feature = "wifi")]
        hotspot: None,
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
        let hotspot = common2.settings.hotspot_enabled.clone();
        if let Some((n, p)) = hotspot {
            common2.hotspot = create_hotspot(&mut common2.wifi, &n, &p);
        }
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
