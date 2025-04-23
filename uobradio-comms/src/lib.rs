//! Module for communicating with a uobradio

use std::{collections::BTreeMap, io::{Read, Write}, thread::JoinHandle};

pub mod video;

#[cfg(target_os = "linux")]
pub use v4l;
use video::SendableVideoSource;

pub type UobRadios = BTreeMap<std::net::SocketAddr, UobRadio>;

#[derive(Debug)]
pub enum RadioReceiveStatus {
    Disconnected,
    WaitForLength(std::time::Instant, [u8; 4], u8),
    WaitForPacket(std::time::Instant, Vec<u8>, u32, u32),
    GotPacket(Vec<u8>),
}

/// The port to listen to for udp communication
const UDP_PORT: u16 = 13456;

pub struct UobRadio {
    address: std::net::SocketAddr,
    comms: Option<std::net::TcpStream>,
    status: RadioReceiveStatus,
    waiting_until: Option<std::time::Instant>,
    timeout: std::time::Duration,
    ping_time: std::time::Instant,
    cameras: Option<BTreeMap<u8, video::SendableVideoSource>>,
    waiting_for_camera_options: bool,
}

impl UobRadio {
    fn new(address: std::net::SocketAddr, timeout_secs: u64) -> Self {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        Self {
            address,
            comms: None,
            status: RadioReceiveStatus::Disconnected,
            waiting_until: None,
            timeout: timeout,
            ping_time: std::time::Instant::now() + timeout / 3,
            cameras: None,
            waiting_for_camera_options: false,
        }
    }

    fn update_ping_time(&mut self) {
        self.ping_time = std::time::Instant::now() + self.timeout / 3;
    }

    fn check_ping_time(&self) -> bool {
        std::time::Instant::now() > self.ping_time
    }

    pub fn cameras(&self) -> Option<&BTreeMap<u8, video::SendableVideoSource>> {
        self.cameras.as_ref()
    }

    pub fn cameras_mut(&mut self) -> Option<&mut BTreeMap<u8, video::SendableVideoSource>> {
        self.cameras.as_mut()
    }
}

#[cfg(not(target_os = "android"))]
pub struct MessageAboutAppUser {
    pub addr: std::net::SocketAddr,
    pub send: std::sync::mpsc::Sender<MessageToApp>,
}

impl Drop for UobRadio {
    fn drop(&mut self) {}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Gpio {
    /// Control the winch output, IN, OUT. Both together is invalid.
    WinchControl(bool, bool),
    /// Enable or disable the leds for the given camera
    CameraLedControl(u8, bool),
    /// Lock all doors
    LockDoors,
    /// Unlock doors
    UnlockDoors,
    /// Control a door window up or down
    WindowControl { id: u8, up: bool, down: bool },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageFromApp {
    Ping(u16),
    RequestCamera(u8),
    /// Request the entire btreemap of all cameras
    RequestCameras,
    GpioControl(Gpio),
    /// The camera index with the bincode encoded data for the setting to change
    CameraSettingControl(u8, u8, video::ControlValue),
    NewSettings(NonvolatileSettings),
    RequestSettings,
}

pub struct MessageFromAppWithAddr {
    pub addr: std::net::SocketAddr,
    pub message: MessageFromApp,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageToApp {
    /// Contains the ping id number for tcp communications, or the port number for udp communications
    PingReply(u16),
    /// Contains a jpeg image from a single camera
    CameraDataJpeg(u8, Vec<u8>),
    /// Send the btreemap of all cameras
    CamerasBtreeMap(BTreeMap<u8, video::SendableVideoSource>),
    /// The new settings for the radio
    NewSettings(NonvolatileSettings),
}

impl MessageFromApp {
    pub fn send_to_stream(&self, stream: &mut std::net::TcpStream) -> Result<(), String> {
        let packet = bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap();
        stream
            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
            .map_err(|e| e.to_string())?;
        stream.write_all(&packet).map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl MessageToApp {
    #[cfg(not(target_os = "android"))]
    pub async fn send_to_stream(&self, stream: &mut tokio::net::TcpStream) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let packet = bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap();
        stream
            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
            .await
            .map_err(|e| e.to_string())?;
        stream.write_all(&packet).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl UobRadio {
    pub fn process_received<F: FnMut(&MessageToApp)>(
        &mut self,
        mut closure: F,
    ) -> Result<(), String> {
        self.connect();
        if let Some(stream) = &mut self.comms {
            loop {
                use std::io::Read;
                let mut got_length = None;
                if let RadioReceiveStatus::WaitForLength(time, l, i) = &mut self.status {
                    if std::time::Instant::now() > *time {
                        return Err("Timeout".to_string());
                    }
                    match stream.read(&mut l[*i as usize..]) {
                        Ok(a) => {
                            if (a + *i as usize) == 4 {
                                let length = u32::from_be_bytes(*l);
                                got_length = Some(length);
                            }
                            *i += a as u8;
                        }
                        Err(e) => {
                            if let std::io::ErrorKind::WouldBlock = e.kind() {
                                return Ok(());
                            } else {
                                return Err(e.to_string());
                            }
                        }
                    }
                }
                if let Some(length) = got_length {
                    self.status = RadioReceiveStatus::WaitForPacket(
                        std::time::Instant::now() + self.timeout,
                        vec![0; length as usize],
                        length,
                        0,
                    );
                }
                let mut go_idle = false;
                if let RadioReceiveStatus::WaitForPacket(time, packet, length, l) = &mut self.status {
                    if std::time::Instant::now() > *time {
                        return Err("Timeout".to_string());
                    }
                    match stream.read(&mut packet[*l as usize..]) {
                        Ok(a) => {
                            if (a + *l as usize) == *length as usize {
                                let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                                    bincode::serde::decode_from_slice(
                                        &packet,
                                        bincode::config::standard(),
                                    );
                                if let Ok((packet, length2)) = packet {
                                    if length2 != *length as usize {
                                        log::error!(
                                            "Wrong packet length received {}/{}",
                                            length,
                                            length2
                                        );
                                        return Err("Invalid packet received".to_string());
                                    }
                                    match &packet {
                                        MessageToApp::CameraDataJpeg(id, data) => {
                                            self.waiting_until = None;
                                            if let Some(cameras) = &mut self.cameras {
                                                let vsrc = cameras.get_mut(id);
                                                if let Some(camera) = vsrc {
                                                    if let Some(img) = crate::video::PixelImage::<crate::video::RgbPixel>::from_jpeg_image(&data) {
                                                        camera.image.replace(img.into());
                                                    }
                                                }
                                            }
                                        }
                                        MessageToApp::CamerasBtreeMap(map) => {
                                            self.cameras.replace(map.to_owned());
                                        }
                                        _ => {}
                                    }
                                    closure(&packet);
                                }
                                go_idle = true;
                            }
                            *l += a as u32;
                        }
                        Err(e) => {
                            if let std::io::ErrorKind::WouldBlock = e.kind() {
                                return Ok(());
                            } else {
                                return Err(e.to_string());
                            }
                        }
                    }
                }
                if go_idle {
                    self.status = RadioReceiveStatus::WaitForLength(
                        std::time::Instant::now() + self.timeout,
                        [0; 4],
                        0,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn get_cameras(&mut self) -> bool {
        if self.cameras.is_none() {
            if !self.waiting_for_camera_options {
                let packet = MessageFromApp::RequestCameras;
                if let Some(comms) = &mut self.comms {
                    let a = packet.send_to_stream(comms);
                    self.waiting_for_camera_options = a.is_ok();
                    self.update_ping_time();
                }
            }
        }
        !self.cameras.is_none()
    }

    pub fn ping(&mut self) -> Result<(), String> {
        self.connect();
        let time = self.check_ping_time();
        if let Some(comms) = &mut self.comms {
            if time {
                let packet = MessageFromApp::Ping(1);
                packet.send_to_stream(comms)?;
                self.update_ping_time();
            }
            Ok(())
        }
        else {
            Err("Not connected".to_string())
        }
    }

    pub fn connect(&mut self) {
        if self.comms.is_none() {
            let tcp = std::net::TcpStream::connect(self.address);
            if let Ok(tcp) = tcp {
                let _ = tcp.set_nonblocking(true);
                self.comms.replace(tcp);
                self.status = RadioReceiveStatus::WaitForLength(
                    std::time::Instant::now() + self.timeout,
                    [0; 4],
                    0,
                );
                self.waiting_until = None;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

    pub fn send_packet(&mut self, packet: MessageFromApp) {
        if let Some(stream) = &mut self.comms {
            if packet.send_to_stream(stream).is_err() {
                self.disconnect();
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.comms.take();
        self.status = RadioReceiveStatus::Disconnected;
        self.waiting_until = None;
    }

    pub fn send_gpio(&mut self, gpio: Gpio) -> Result<(), String> {
        self.connect();
        if let Some(comms) = &mut self.comms {
            log::error!("Sending gpio request {:?}", gpio);
            let packet = MessageFromApp::GpioControl(gpio);
            packet.send_to_stream(comms)?;
            self.update_ping_time();
            Ok(())
        }
        else {
            Err("Not connected".to_string())
        }
    }

    pub fn send_camera_request(&mut self, index: u8) -> Result<(),String> {
        self.connect();
        if let Some(inst) = &self.waiting_until {
            if *inst < std::time::Instant::now() {
                self.waiting_until = None;
            }
        } else {
            if let Some(comms) = &mut self.comms {
                let packet = MessageFromApp::RequestCamera(index);
                packet.send_to_stream(comms)?;
                self.waiting_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
                self.update_ping_time();
            }
        }
        Ok(())
    }

    pub fn finish_camera_request(&mut self) {
        log::error!("Finishing camera request");
        self.waiting_until = None;
    }

    pub fn localhost() -> Self {
        let ip: std::net::Ipv4Addr = std::net::Ipv4Addr::new(127, 0, 0, 1);
        let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ip), 13457);
        UobRadio::new(addr, 5)
    }

    pub fn detect_radios(
        times: u8,
    ) -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let packet =
            bincode::serde::encode_to_vec(MessageFromApp::Ping(0), bincode::config::standard())
                .unwrap();
        for _ in 0..times {
            socket.send_to(&packet, format!("255.255.255.255:{}", UDP_PORT))?;
        }
        let mut response = vec![0; 1500];
        while let Ok((n, addr)) = socket.recv_from(&mut response) {
            let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((MessageToApp::PingReply(port), _n)) = packet {
                let mut tcp_addr = addr.clone();
                tcp_addr.set_port(port);
                let r = UobRadio::new(tcp_addr, 5);
                radios.insert(addr, r);
            }
        }
        Ok(radios)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct NonvolatileSettings {
    #[cfg(feature = "wifi")]
    /// Optional wifi name and password for wifi hotspot
    pub hotspot_enabled: Option<(String, String)>,
}

impl NonvolatileSettings {
    pub fn save(&self) {
        let d = bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap();
        let f = std::fs::File::create("./settings.bin");
        if let Ok(mut f) = f {
            let _ = f.write_all(&d);
        }
    }

    pub fn load() -> Self {
        let f = std::fs::File::open("./settings.bin");
        if let Ok(mut f) = f {
            let mut contents = Vec::new();
            let _ = f.read_to_end(&mut contents);
            let s = bincode::serde::decode_from_slice(&contents, bincode::config::standard());
            if let Ok((s, _)) = s {
                s
            }
            else {
                Self::default()
            }
        }
        else {
            Self::default()
        }
    }
}