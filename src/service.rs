#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

use std::{
    io::Read,
    sync::{Arc, Mutex},
};

use android_auto::HeadUnitInfo;
use tokio::io::AsyncReadExt;
use uobradio_comms::{aauto::AndroidAutoMessageFromPhone, NonvolatileSettings};
use video_service::VideoSource;
use wifi_rs::prelude::ManagedWifiHotspotTrait;
use wifi_rs::prelude::WifiHotspot;

mod video_service;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct SystemSettings {
    #[cfg(feature = "wifi")]
    wifi_name: String,
    #[cfg(feature = "wifi")]
    wifi_mac: String,
}

impl SystemSettings {
    pub fn load() -> Self {
        let p = std::path::Path::new("./settings.toml");
        let f = std::fs::File::open(&p);
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

/// The common data for an app user
pub struct AppUserCommon {
    system: SystemSettings,
    #[cfg(feature = "wifi")]
    wifi: wifi_rs::WiFi,
    #[cfg(feature = "wifi")]
    hotspot: Option<wifi_rs::prelude::ManagedWifiHotspot>,
    #[cfg(feature = "bluetooth")]
    bluetooth: bluetooth_rust::BluetoothHandler,
    #[cfg(feature = "bluetooth")]
    blue_recv: tokio::sync::mpsc::Receiver<bluetooth_rust::MessageToBluetoothHost>,
    #[cfg(feature = "bluetooth")]
    /// Determines who deals with the bluetooth stuff
    blue_addr: Option<std::net::SocketAddr>,
    /// Determines who deals with the android-auto stuff
    aauto_addr: Option<std::net::SocketAddr>,
    aauto_recv: tokio::sync::mpsc::Receiver<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
    video: Vec<VideoSource>,
    old_settings: NonvolatileSettings,
    settings: NonvolatileSettings,
}

#[cfg(feature = "wifi")]
fn create_hotspot(
    wifi: &mut wifi_rs::WiFi,
    name: &String,
    password: &String,
) -> Option<wifi_rs::prelude::ManagedWifiHotspot> {
    use wifi_rs::prelude::ManagedWifiHotspotTrait;
    let configuration = wifi_rs::prelude::HotspotConfig::new(None, None);
    log::info!("Attempting to create hotspot {:?} {:?}", name, password);
    let a = wifi
        .create_managed_hotspot(name, password, Some(&configuration))
        .ok();
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
        let length = stream.read_u32().await.map_err(|e| e.to_string())?;
        let mut packet = vec![0; length as usize];
        stream
            .read_exact(&mut packet)
            .await
            .map_err(|e| e.to_string())?;
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
                },
                uobradio_comms::MessageFromApp::RequestAndroidAutoControl => {
                    let mut common = common.lock().await;
                    let r = if common.aauto_addr.is_none() {
                        println!("Setting {:?} as android auto master", addr);
                        common.aauto_addr = Some(addr);
                        true
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
                        common2.bluetooth.set_discoverable(val).await;
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
                        if let Some(hotspot) = &mut common2.hotspot {
                            let _ = hotspot.start_hotspot();
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
                if common2.aauto_addr.is_some() {
                    while let Ok(m) = common2.aauto_recv.try_recv() {
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

async fn udp_listener(_common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    println!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        log::info!("Waiting for a udp client");
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            let addr = addr.clone();
            println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                match packet {
                    uobradio_comms::MessageFromApp::Ping(val) => {
                        println!("got ping packet {}", val);
                        let response = bincode::serde::encode_to_vec(
                            uobradio_comms::MessageToApp::PingReply(13457),
                            bincode::config::standard(),
                        )
                        .unwrap();
                        let _ = socket.send_to(&response, addr).await;
                    }
                    _ => {}
                }
            } else {
                println!("invalid packet received {:x?}", response);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn tcp_listener(common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    if let Ok(tcp) = tcp {
        loop {
            log::info!("Waiting for a tcp client");
            if let Ok((stream, addr)) = tcp.accept().await {
                let common2 = common.clone();
                let _ = tokio::task::spawn(async move {
                    let r = process_app(stream, addr, common2.clone()).await;
                    let mut common3 = common2.lock().await;
                    if Some(addr) == common3.blue_addr {
                        println!("Setting {:?} as no longer the bluetooth master", addr);
                        common3.blue_addr.take();
                    }
                    if Some(addr) == common3.aauto_addr {
                        println!("Setting {:?} as no longer the android auto master", addr);
                        common3.aauto_addr.take();
                    }
                    println!("Completed handling user {:?}", r);
                    r
                });
            }
        }
    } else {
        panic!("Unable to open tcp listener to listen for apps connecting");
    }
}

struct AndroidAutoStuff {
    sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
}

impl android_auto::AndroidAutoMainTrait for AndroidAutoStuff {
    fn supports_video(&mut self) -> Option<&mut dyn android_auto::AndroidAutoVideoChannelTrait> {
        Some(self)
    }
}

impl android_auto::AndroidAutoVideoChannelTrait for AndroidAutoStuff {
    fn receive_video(&mut self, data: &[u8]) {
        log::error!("Received {} bytes of video data", data.len());
        let a = self.sendr.blocking_send(AndroidAutoMessageFromPhone::VideoContent(data.to_vec()));
        log::error!("Attempt to relay video data {:?}", a);
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
    let mut bluetooth = bluetooth_rust::BluetoothHandler::new(bluechan.0)
        .await
        .expect("Could not open bluetooth");

    let aautochan = tokio::sync::mpsc::channel(5);

    #[cfg(all(feature = "bluetooth", feature = "androidauto"))]
    let mut android_auto_bluetooth_server =
        android_auto::AndriodAutoBluettothServer::new(&mut bluetooth).await;

    let common = Arc::new(tokio::sync::Mutex::new(AppUserCommon {
        #[cfg(feature = "wifi")]
        wifi: wifi_rs::WiFi::new(Some(wifi_rs::prelude::Config {
            interface: Some(&sys.wifi_name),
        })),
        system: sys,
        #[cfg(feature = "wifi")]
        hotspot: None,
        #[cfg(feature = "bluetooth")]
        bluetooth,
        #[cfg(feature = "bluetooth")]
        blue_recv: bluechan.1,
        #[cfg(feature = "bluetooth")]
        blue_addr: None,
        aauto_addr: None,
        aauto_recv: aautochan.1,
        video: vs,
        old_settings: s.clone(),
        settings: s.clone(),
    }));

    {
        let mut common2 = common.lock().await;
        let hotspot = common2.settings.hotspot_enabled.clone();
        if let Some((n, p)) = hotspot {
            common2.hotspot = create_hotspot(&mut common2.wifi, &n, &p);
            if let Some(hotspot) = &mut common2.hotspot {
                let _ = hotspot.start_hotspot();
            }
        }
    }

    let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
    let common2 = common.clone();
    tasks.spawn(async move { udp_listener(common2).await });
    let common2 = common.clone();
    tasks.spawn(async move { tcp_listener(common2).await });

    let network = {
        let common2 = common.lock().await;
        if let Some(a) = &common2.settings.hotspot_enabled {
            Some(android_auto::NetworkInformation {
                ssid: a.0.clone(),
                psk: a.1.clone(),
                mac_addr: common2.system.wifi_mac.clone(),
                ip: "10.42.0.1".to_string(),
                port: 5277,
                security_mode: android_auto::Bluetooth::SecurityMode::WPA2_PERSONAL,
                ap_type: android_auto::Bluetooth::AccessPointType::STATIC,
            })
        } else {
            None
        }
    };

    {
        if let Some(network) = network {
            let config = android_auto::AndroidAutoConfiguration {
                network: network.clone(),
                bluetooth: android_auto::BluetoothInformation {
                    address: "00:93:37:EF:B7:57".to_string(),
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
            };
            let net2 = network.clone();
            tasks.spawn(async move { android_auto_bluetooth_server.bluetooth_listen(net2).await });
            let main = AndroidAutoStuff { sendr: aautochan.0, };
            std::thread::spawn(move || {
                android_auto::AndriodAutoBluettothServer::wifi_listen(config, main)
            });
        }
    }
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

#[tokio::main]
async fn main() -> Result<(), u32> {
    let service = service::Service::new("uobradio".to_string());
    service.new_log(service::LogLevel::Debug);
    if let Err(e) = service::DispatchAsync!(service, service_starter) {
        Err(e)
    } else {
        Ok(())
    }
}
