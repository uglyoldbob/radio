//! Module for communicating with a uobradio

use std::{collections::BTreeMap, net::{SocketAddr, UdpSocket}};

pub type UobRadios = BTreeMap<SocketAddr, UobRadio>;

#[derive(Debug)]
pub struct UobRadio {
    address: SocketAddr,
}

impl UobRadio {
    pub fn detect_radios() -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket : UdpSocket = UdpSocket::bind("0.0.0.0:13456")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let data = &[0];
        let r = socket.send(data);
        if let Ok(n) = r {
            if n == data.len() {
                let mut response = Vec::new();
                while let Ok((_n, addr)) = socket.recv_from(&mut response) {
                    let r = UobRadio { address: addr, };
                    radios.insert(addr, r);
                }
            }
        }
        Ok(radios)
    }
}