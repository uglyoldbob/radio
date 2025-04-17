//! Module for communicating with a uobradio

use std::{
    collections::BTreeMap,
    net::{SocketAddr, UdpSocket},
    thread::JoinHandle,
};

pub type UobRadios = BTreeMap<SocketAddr, UobRadio>;

#[derive(Debug)]
pub struct UobRadio {
    address: SocketAddr,
    /// Used to indicate that the app thread should stop
    s: std::sync::mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl UobRadio {
    fn new(address: SocketAddr, send: std::sync::mpsc::Sender<MessageToApp>) -> Self {
        let (s, r) = std::sync::mpsc::channel();
        Self {
            address: address,
            s,
            thread: Some(std::thread::spawn(move || {
                Self::app_listener(address, r, send);
            })),
        }
    }
}

impl Drop for UobRadio {
    fn drop(&mut self) {
        let _ = self.s.send(());
        let _ = self.thread.take().unwrap().join();
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum MessageFromApp {
    Ping,
    RequestCamera(bool, u8),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum MessageToApp {
    PingReply,
    CameraDataJpeg(u8, Vec<u8>),
}

impl UobRadio {
    pub fn listener() {
        let socket = UdpSocket::bind("0.0.0.0:13456").unwrap();
        println!("Starting radio listener");
        let mut response = vec![0; 1500];
        loop {
            while let Ok((n, addr)) = socket.recv_from(&mut response) {
                let mut addr = addr.clone();
                println!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
                let packet =
                    bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
                if let Ok((packet, _len)) = packet {
                    match packet {
                        MessageFromApp::Ping => {
                            println!("got ping packet");
                            let response = bincode::serde::encode_to_vec(
                                MessageToApp::PingReply,
                                bincode::config::standard(),
                            )
                            .unwrap();
                            let _ = socket.send_to(&response, addr);
                        }
                        MessageFromApp::RequestCamera(enabled, index) => {
                            println!("got camera request {} {}", enabled, index);
                            addr.set_port(13457);
                            let response = bincode::serde::encode_to_vec(
                                MessageToApp::CameraDataJpeg(0, Vec::new()),
                                bincode::config::standard(),
                            )
                            .unwrap();
                            let _ = socket.send_to(&response, addr);
                            let _ = socket.send_to(&response, addr);
                        }
                    }
                } else {
                    println!("invalid packet received {:x?}", response);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    pub fn app_listener(
        address: SocketAddr,
        mut r: std::sync::mpsc::Receiver<()>,
        send: std::sync::mpsc::Sender<MessageToApp>,
    ) {
        log::error!(
            "Starting thread for receiving data from radio at {:?}",
            address
        );
        let socket = UdpSocket::bind("0.0.0.0:13457");
        log::error!("socket is {:?}", socket);
        let socket = socket.unwrap();
        let _ = socket.set_read_timeout(Some(std::time::Duration::new(1, 0)));
        let mut response = vec![0; 1500];
        log::error!(
            "Starting loop for receiving data from radio at {:?}",
            address
        );
        loop {
            while let Ok((n, addr)) = socket.recv_from(&mut response) {
                let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                    bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
                if let Ok((message, _n)) = packet {
                    match &message {
                        MessageToApp::PingReply => {}
                        MessageToApp::CameraDataJpeg(index, _jpeg) => {
                            log::error!("Got jpeg for camera {} from {:?}", index, addr);
                        }
                    }
                    let _ = send.send(message);
                }
            }
            if r.try_recv().is_ok() {
                break;
            }
        }
    }

    pub fn send_camera_request(&mut self, enabled: bool, index: u8) {
        let socket = UdpSocket::bind("0.0.0.0:0");
        if let Ok(socket) = socket {
            let packet = bincode::serde::encode_to_vec(
                MessageFromApp::RequestCamera(enabled, index),
                bincode::config::standard(),
            )
            .unwrap();
            let _ = socket.send_to(&packet, self.address);
        } else {
            log::error!("Failed to bind socket: {:?} {:?}", socket, self.address);
        }
    }

    pub fn detect_radios(
        send: std::sync::mpsc::Sender<MessageToApp>,
    ) -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = UdpSocket::bind("0.0.0.0:0")?;
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
                if let Ok((MessageToApp::PingReply, _n)) = packet {
                    let r = UobRadio::new(addr, send.clone());
                    radios.insert(addr, r);
                }
            }
        } else {
            log::error!("Only sent {} bytes", r);
        }
        Ok(radios)
    }
}
