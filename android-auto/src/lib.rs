pub struct AndriodAutoBluettothServer {
    #[cfg(feature = "wireless")]
    blue: bluetooth_rust::RfcommProfileHandle,
}

include!(concat!(env!("OUT_DIR"), "/protobuf/mod.rs"));

#[cfg(feature = "wireless")]
struct AndroidAutoMessage {
    t: u16,
    message: Vec<u8>,
}

enum AndroidAutoBluetoothMessage {
    SocketInfoRequest(SocketInfoRequest::SocketInfoRequest),
}

impl AndroidAutoBluetoothMessage {
    fn as_message(&self) -> AndroidAutoMessage {
        match self {
            AndroidAutoBluetoothMessage::SocketInfoRequest(m) => {
                use protobuf::Message;
                AndroidAutoMessage {
                    t: 1,
                    message: m.write_to_bytes().unwrap(),
                }
            }
        }
    }
}

impl Into<Vec<u8>> for AndroidAutoMessage {
    fn into(self) -> Vec<u8> {
        let mut r = Vec::new();
        let b = self.message.len() as u16;
        let a = b.to_be_bytes();
        r.push(a[0]);
        r.push(a[1]);
        let a = self.t.to_be_bytes();
        r.push(a[0]);
        r.push(a[1]);
        for b in &self.message {
            r.push(*b);
        }
        r
    }
}

impl AndriodAutoBluettothServer {
    #[cfg(feature = "wireless")]
    pub async fn new(bluetooth: &mut bluetooth_rust::BluetoothHandler) -> Self {
        let profile = bluetooth_rust::RfcommProfile {
            uuid: bluetooth_rust::Uuid::parse_str(bluetooth_rust::BluetoothUuid::AndroidAuto.as_str()).unwrap(),
            name: Some("Android Auto Bluetooth Service".to_string()),
            service: bluetooth_rust::Uuid::parse_str(bluetooth_rust::BluetoothUuid::AndroidAuto.as_str()).ok(),
            role: None,
            channel: Some(22),
            psm: None,
            require_authentication: Some(true),
            require_authorization: Some(true),
            auto_connect: Some(true),
            service_record: None,
            version: None,
            features: None,
            ..Default::default()
        };
        let a = bluetooth.register_rfcomm_profile(profile).await;
        Self {
            blue: a.unwrap(),
        }
    }

    #[cfg(feature = "wireless")]
    pub async fn bluetooth_listen(&mut self) -> Result<(), String> {
        use futures::StreamExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        println!("Listening for connections on android auto profile");
        loop {
            if let Some(cr) = self.blue.next().await {
                tokio::task::spawn(async move {
                    println!("Got a connection to android auto profile on {:?}", cr);
                    let stream = cr.accept().unwrap();
                    let (mut read, mut write) = stream.into_split();
                    let mut s = SocketInfoRequest::SocketInfoRequest::new();
                    s.set_ip_address("10.42.0.1".to_string());

                    let m1 = AndroidAutoBluetoothMessage::SocketInfoRequest(s);
                    let m : AndroidAutoMessage = m1.as_message();
                    let mdata: Vec<u8> = m.into();
                    println!("Sending packet {:x?}", mdata);
                    let r1 = write.write_all(&mdata).await;
                    println!("Result of sending first packet is {:?}", r1);
                    loop {
                        let mut ty = [0u8;2];
                        let mut len = [0u8;2];
                        let r2 = read.read_exact(&mut len).await;
                        println!("Results of read packet are {:?}", r2);
                        let r3 = read.read_exact(&mut ty).await;
                        println!("Results of read packet are {:?} {:?}", r2, r3);
                        println!("I think the length is {:?} type {:?}", len, ty);
                        let len = u16::from_be_bytes(len);
                        let ty = u16::from_be_bytes(ty);
                        let mut message = vec![0; len as usize];
                        read.read_exact(&mut message).await.map_err(|e| e.to_string())?;
                        println!("Got a packet type {} len {}", ty, len);
                        match ty {
                            1 => {
                                println!("Got a socket info request {:x?}", message);
                            }
                            2 => {
                                println!("Got a request for network info {:x?}", message);
                            }
                            7 => {
                                println!("Got socket info response {:x?}", message);
                            }
                            _ => {
                                println!("Unknown packet {}", ty);
                                break;
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Ok::<(), String>(())
                });
            }
        }
    }

    #[cfg(feature = "wireless")]
    pub async fn wifi_listen() -> Result<(), String> {
        println!("Listening on port 5277 for android auto stuff");
        if let Ok(a) = tokio::net::TcpListener::bind("0.0.0.0:5277").await {
            loop {
                if let Ok((stream, addr)) = a.accept().await {
                    tokio::task::spawn(async move {
                        println!("Got a connection on port 5000 from {:?}", addr);
                    });
                }
            }
        }
        else {
            Err("Failed to listen on port 5000 tcp".to_string())
        }
    }

    #[cfg(not(feature = "wireless"))]
    pub async fn new() -> Self {
        Self {}
    }
}