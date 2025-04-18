//! Module for communicating with a uobradio

use std::{collections::BTreeMap, io::Write, thread::JoinHandle};

pub type UobRadios = BTreeMap<std::net::SocketAddr, UobRadio>;

#[derive(Debug)]
pub enum RadioReceiveStatus {
    Disconnected,
    WaitForLength,
    WaitForPacket(u32),
    GotPacket(Vec<u8>),
}

#[derive(Debug)]
pub struct UobRadio {
    address: std::net::SocketAddr,
    comms: Option<std::net::TcpStream>,
    status: RadioReceiveStatus,
}

impl UobRadio {
    fn new(address: std::net::SocketAddr) -> Self {
        Self {
            address,
            comms: None,
            status: RadioReceiveStatus::Disconnected,
        }
    }
}

impl Drop for UobRadio {
    fn drop(&mut self) {}
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageFromApp {
    Ping,
    RequestCamera(bool, u8),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageToApp {
    PingReply(u16),
    CameraDataJpeg(u8, Vec<u8>),
}

impl UobRadio {
    #[cfg(not(target_os = "android"))]
    pub async fn tcp_listener() {
        let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
        if let Ok(tcp) = tcp {
            loop {
                if let Ok((stream, addr)) = tcp.accept().await {
                    let _ =
                        tokio::task::spawn(async move { Self::process_app(stream, addr).await })
                            .await
                            .unwrap();
                }
            }
        } else {
            panic!("Unable to open tcp listener to listen for apps connecting");
        }
    }

    #[cfg(not(target_os = "android"))]
    /// Processes a tcp connection from an app
    pub async fn process_app(
        mut stream: tokio::net::TcpStream,
        addr: std::net::SocketAddr,
    ) -> Result<(), ()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        println!("Processing an app at {:?}", addr);
        loop {
            let length = stream.read_u32().await.map_err(|_| ())?;
            println!("Got length of {}", length);
            let mut packet = vec![0; length as usize];
            stream.read_exact(&mut packet).await.map_err(|_| ())?;
            println!("got packet");
            let packet: Result<(MessageFromApp, usize), bincode::error::DecodeError> =
                bincode::serde::decode_from_slice(&packet, bincode::config::standard());
            println!("Packet is {:?}", packet);
            if let Ok((packet, _length)) = packet {
                match packet {
                    MessageFromApp::Ping => {
                        let reply = bincode::serde::encode_to_vec(
                            MessageToApp::PingReply(stream.local_addr().unwrap().port()),
                            bincode::config::standard(),
                        )
                        .unwrap();
                        stream.write_all(&reply).await.map_err(|_| ())?;
                    }
                    MessageFromApp::RequestCamera(enabled, index) => {
                        if enabled {
                            let reply = bincode::serde::encode_to_vec(
                                MessageToApp::CameraDataJpeg(index, Vec::new()),
                                bincode::config::standard(),
                            )
                            .unwrap();
                            stream
                                .write_all(&(reply.len() as u32).to_be_bytes()[0..4])
                                .await
                                .map_err(|_| ())?;
                            stream.write_all(&reply).await.map_err(|_| ())?;
                            stream
                                .write_all(&(reply.len() as u32).to_be_bytes()[0..4])
                                .await
                                .map_err(|_| ())?;
                            stream.write_all(&reply).await.map_err(|_| ())?;
                            println!("Done sending camera dummy data");
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    pub async fn udp_listener() {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
        println!("Starting radio listener");
        let mut response = vec![0; 1500];
        loop {
            while let Ok((n, addr)) = socket.recv_from(&mut response).await {
                let mut addr = addr.clone();
                println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
                let packet =
                    bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
                if let Ok((packet, _len)) = packet {
                    match packet {
                        MessageFromApp::Ping => {
                            println!("got ping packet");
                            let response = bincode::serde::encode_to_vec(
                                MessageToApp::PingReply(13457),
                                bincode::config::standard(),
                            )
                            .unwrap();
                            let _ = socket.send_to(&response, addr).await;
                        }
                        MessageFromApp::RequestCamera(enabled, index) => {
                            println!("got camera request {} {}", enabled, index);
                            addr.set_port(13457);
                            let response = bincode::serde::encode_to_vec(
                                MessageToApp::CameraDataJpeg(0, Vec::new()),
                                bincode::config::standard(),
                            )
                            .unwrap();
                            let _ = socket.send_to(&response, addr).await;
                            let _ = socket.send_to(&response, addr).await;
                        }
                    }
                } else {
                    println!("invalid packet received {:x?}", response);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    #[cfg(target_os = "android")]
    pub fn process_received(
        &mut self,
        send: &mut std::sync::mpsc::Sender<MessageToApp>,
    ) -> Result<(), ()> {
        self.connect();
        if let Some(stream) = &mut self.comms {
            use std::io::Read;
            if let RadioReceiveStatus::WaitForLength = self.status {
                let mut length: [u8; 4] = [0; 4];
                match stream.read_exact(&mut length) {
                    Ok(a) => {
                        let length = u32::from_be_bytes(length);
                        println!("Got length of {}", length);
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
                        println!("got packet");
                        let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                            bincode::serde::decode_from_slice(&packet, bincode::config::standard());
                        println!("Packet is {:?}", packet);
                        if let Ok((packet, _length)) = packet {
                            send.send(packet).map_err(|_| ())?;
                        }
                        self.status = RadioReceiveStatus::WaitForLength;
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
            }
        }
    }

    #[cfg(target_os = "android")]
    pub fn disconnect(&mut self) {
        self.comms.take();
        self.status = RadioReceiveStatus::Disconnected;
    }

    #[cfg(target_os = "android")]
    pub fn send_camera_request(&mut self, enabled: bool, index: u8) {
        self.connect();
        if let Some(comms) = &mut self.comms {
            log::error!("Sending camera request {} {}", enabled, index);
            let packet = bincode::serde::encode_to_vec(
                MessageFromApp::RequestCamera(enabled, index),
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
    pub fn detect_radios(
        send: std::sync::mpsc::Sender<MessageToApp>,
    ) -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let packet =
            bincode::serde::encode_to_vec(MessageFromApp::Ping, bincode::config::standard())
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
