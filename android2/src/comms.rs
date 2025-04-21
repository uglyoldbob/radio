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
    WaitForLength([u8;4], u8),
    WaitForPacket(Vec<u8>, u32, u32),
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

impl MessageToApp {
    #[cfg(not(target_os = "android"))]
    pub async fn send_to_stream(&self, stream: &mut tokio::net::TcpStream) -> Result<(),()> {
        use tokio::io::AsyncWriteExt;
        let packet =
            bincode::serde::encode_to_vec(self, bincode::config::standard())
                .unwrap();
        let length = packet.len();
        stream
            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
            .await.map_err(|_|())?;
        stream.write_all(&packet).await.map_err(|_|())?;
        Ok(())
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
                self.status = RadioReceiveStatus::WaitForLength([0;4], 0);
            }
            let mut got_length = None;
            if let RadioReceiveStatus::WaitForLength(mut l, mut i) = &mut self.status {
                match stream.read(&mut l[i as usize..]) {
                    Ok(a) => {
                        if (a + i as usize) == 4 {
                            let length = u32::from_be_bytes(l);
                            log::error!("Got length of 0x{:04x}, waiting for packet", length);
                            got_length = Some(length);
                        }
                        i += a as u8;
                    }
                    Err(e) => {
                        if let std::io::ErrorKind::WouldBlock = e.kind() {
                        } else {
                            return Err(());
                        }
                    }
                }
            }
            if let Some(length) = got_length {
                self.status = RadioReceiveStatus::WaitForPacket(vec![0; length as usize], length, 0);
            }
            let mut go_idle = false;
            if let RadioReceiveStatus::WaitForPacket(packet, length, l) = &mut self.status {
                match stream.read(&mut packet[*l as usize..]) {
                    Ok(a) => {
                        if (a + *l as usize) == *length as usize {
                            log::error!("got packet length {}", length);
                            if *length > 16 {
                                log::error!("DATA {:x?}...", &packet[0..16]);
                            }
                            else {
                                log::error!("DATA {:x?}", &packet[0..*length as usize]);
                            }
                            let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                                bincode::serde::decode_from_slice(&packet, bincode::config::standard());
                            if let Ok((packet, length2)) = packet {
                                if length2 != *length as usize {
                                    log::error!("Wrong packet length received {}/{}", length, length2);
                                    return Err(());
                                }
                                match &packet {
                                    MessageToApp::PingReply(_) => {
                                        return Err(());
                                    }
                                    MessageToApp::CameraDataJpeg(_, _) => {
                                        log::error!("Finishing camera request");
                                        self.waiting_until = None;
                                    }
                                    _ => {}
                                }
                                send.send(packet).map_err(|_| ())?;
                            }
                            go_idle = true;
                        }
                        *l += a as u32;
                    }
                    Err(e) => {
                        if let std::io::ErrorKind::WouldBlock = e.kind() {
                        } else {
                            return Err(());
                        }
                    }
                }
            }
            if go_idle {
                self.status = RadioReceiveStatus::Idle;
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
                self.status = RadioReceiveStatus::Idle;
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
