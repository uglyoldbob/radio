//! Module for communicating with a uobradio

use std::{collections::BTreeMap, net::{SocketAddr, UdpSocket}};

pub type UobRadios = BTreeMap<SocketAddr, UobRadio>;

#[derive(Debug)]
pub struct UobRadio {
    address: SocketAddr,
}



impl UobRadio {
    pub fn listener() {
        let socket = UdpSocket::bind("0.0.0.0:13456").unwrap();
        println!("Starting radio listener");
        let mut response = Vec::new();
        loop {
            while let Ok((_n, addr)) = socket.recv_from(&mut response) {
                println!("Got request from {:?}", addr);
                let buf = &[0];
                let _ = socket.send_to(buf, addr);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    pub fn detect_radios() -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let data = &[0];
        let r = socket.send_to(data, "255.255.255.255:13456")?;
        if r == data.len() {
            let mut response = Vec::new();
            while let Ok((_n, addr)) = socket.recv_from(&mut response) {
                let r = UobRadio { address: addr, };
                radios.insert(addr, r);
            }
        }
        Ok(radios)
    }
}