//! Module for communicating with a uobradio

#[cfg(target_os = "android")]
use std::{collections::BTreeMap, io::Write, thread::JoinHandle};

#[cfg(target_os = "android")]
pub type UobRadios = BTreeMap<std::net::SocketAddr, UobRadio>;

#[cfg(target_os = "android")]
#[derive(Debug)]
pub enum RadioReceiveStatus {
    Disconnected,
    Idle,
    WaitForLength,
    WaitForPacket(u32),
    GotPacket(Vec<u8>),
}

#[cfg(target_os = "android")]
#[derive(Debug)]
pub struct UobRadio {
    address: std::net::SocketAddr,
    comms: Option<std::net::TcpStream>,
    status: RadioReceiveStatus,
    waiting_until: Option<std::time::Instant>,
}

#[cfg(target_os = "android")]
impl UobRadio {
    fn new(address: std::net::SocketAddr) -> Self {
        Self {
            address,
            comms: None,
            status: RadioReceiveStatus::Disconnected,
            waiting_until: None,
        }
    }
}

#[cfg(not(target_os = "android"))]
pub struct MessageAboutAppUser {
    pub addr: std::net::SocketAddr,
    pub send: std::sync::mpsc::Sender<MessageToApp>,
}

#[cfg(target_os = "android")]
impl Drop for UobRadio {
    fn drop(&mut self) {}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Gpio {
    /// Control the winch output, forwards, reverse. Both together is invalid.
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
    Ping(u32),
    RequestCamera(u8),
    GpioControl(Gpio),
}

pub struct MessageFromAppWithAddr {
    pub addr: std::net::SocketAddr,
    pub message: MessageFromApp,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageToApp {
    PingReply(u16),
    CameraDataJpeg(u8, Vec<u8>),
}

#[cfg(not(target_os = "android"))]
pub async fn udp_listener() -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    println!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            let addr = addr.clone();
            println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                match packet {
                    MessageFromApp::Ping(val) => {
                        println!("got ping packet {}", val);
                        let response = bincode::serde::encode_to_vec(
                            MessageToApp::PingReply(13457),
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

#[cfg(target_os = "android")]
impl UobRadio {
    #[cfg(target_os = "android")]
    pub fn process_received(
        &mut self,
        send: &mut std::sync::mpsc::Sender<MessageToApp>,
    ) -> Result<(), ()> {
        self.connect();
        if let Some(stream) = &mut self.comms {
            use std::io::Read;
            if let RadioReceiveStatus::Idle = self.status {
                log::error!("Waiting for length of packet");
                self.status = RadioReceiveStatus::WaitForLength;
            }
            if let RadioReceiveStatus::WaitForLength = self.status {
                let mut length: [u8; 4] = [0; 4];
                match stream.read_exact(&mut length) {
                    Ok(a) => {
                        let length = u32::from_be_bytes(length);
                        log::error!("Got length of {}, waiting for packet", length);
                        self.status = RadioReceiveStatus::WaitForPacket(length);
                    }
                    Err(e) => {
                        if let std::io::ErrorKind::WouldBlock = e.kind() {
                        } else {
                            return Err(());
                        }
                    }
                }
            }
            if let RadioReceiveStatus::WaitForPacket(length) = self.status {
                let mut packet = vec![0; length as usize];
                match stream.read_exact(&mut packet) {
                    Ok(a) => {
                        log::error!("got packet length {}", length);
                        let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                            bincode::serde::decode_from_slice(&packet, bincode::config::standard());
                        log::error!("Packet is {:?}", packet);
                        if let Ok((packet, _length)) = packet {
                            match &packet {
                                MessageToApp::CameraDataJpeg(_, _) => self.finish_camera_request(),
                                _ => {}
                            }
                            send.send(packet).map_err(|_| ())?;
                        }
                        self.status = RadioReceiveStatus::Idle;
                    }
                    Err(e) => {
                        if let std::io::ErrorKind::WouldBlock = e.kind() {
                        } else {
                            return Err(());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "android")]
    pub fn connect(&mut self) {
        if self.comms.is_none() {
            let tcp = std::net::TcpStream::connect(self.address);
            if let Ok(tcp) = tcp {
                tcp.set_nonblocking(true);
                self.comms.replace(tcp);
                self.status = RadioReceiveStatus::WaitForLength;
                self.waiting_until = None;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

    #[cfg(target_os = "android")]
    pub fn disconnect(&mut self) {
        self.comms.take();
        self.status = RadioReceiveStatus::Disconnected;
        self.waiting_until = None;
    }

    #[cfg(target_os = "android")]
    pub fn send_gpio(&mut self, gpio: Gpio) {
        self.connect();
        if let Some(comms) = &mut self.comms {
            log::error!("Sending gpio request {:?}", gpio);
            let packet = bincode::serde::encode_to_vec(
                MessageFromApp::GpioControl(gpio),
                bincode::config::standard(),
            )
            .unwrap();
            let len = packet.len() as u32;
            log::error!("Packet length is {}", len);
            let len = len.to_be_bytes();
            log::error!("Packet is {:x?}", len);
            let _ = comms.write_all(&(len[0..4]));
            let _ = comms.write_all(&packet);
        }
    }

    #[cfg(target_os = "android")]
    pub fn send_camera_request(&mut self, enabled: bool, index: u8) {
        self.connect();
        if let Some(inst) = &self.waiting_until {
            if *inst < std::time::Instant::now() {
                self.waiting_until = None;
            }
        } else {
            if let Some(comms) = &mut self.comms {
                let packet = bincode::serde::encode_to_vec(
                    MessageFromApp::RequestCamera(index),
                    bincode::config::standard(),
                )
                .unwrap();
                let len = packet.len() as u32;
                let len = len.to_be_bytes();
                let _ = comms.write_all(&(len[0..4]));
                let _ = comms.write_all(&packet);
                self.waiting_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
            }
        }
    }

    #[cfg(target_os = "android")]
    pub fn finish_camera_request(&mut self) {
        log::error!("Finishing camera request");
        self.waiting_until = None;
    }

    #[cfg(target_os = "android")]
    pub fn detect_radios(
        send: std::sync::mpsc::Sender<MessageToApp>,
    ) -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let packet =
            bincode::serde::encode_to_vec(MessageFromApp::Ping(0), bincode::config::standard())
                .unwrap();
        log::error!("Sending packet to discover radios: {:x?}", packet);
        let r = socket.send_to(&packet, "255.255.255.255:13456")?;
        if r == packet.len() {
            log::error!("Sent {} bytes", r);
            let mut response = vec![0; 1500];
            while let Ok((n, addr)) = socket.recv_from(&mut response) {
                let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                    bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
                if let Ok((MessageToApp::PingReply(port), _n)) = packet {
                    let mut tcp_addr = addr.clone();
                    tcp_addr.set_port(port);
                    let r = UobRadio::new(tcp_addr);
                    radios.insert(addr, r);
                }
            }
        } else {
            log::error!("Only sent {} bytes", r);
        }
        Ok(radios)
    }
}
