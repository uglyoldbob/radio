use rustls::sign::CertifiedKey;
use tokio::io::AsyncReadExt;

mod cert;

pub struct AndriodAutoBluettothServer {
    #[cfg(feature = "wireless")]
    blue: bluetooth_rust::RfcommProfileHandle,
}

include!(concat!(env!("OUT_DIR"), "/protobuf/mod.rs"));

const VERSION: (u16, u16) = (1, 1);

#[cfg(feature = "wireless")]
struct AndroidAutoMessage {
    t: u16,
    message: Vec<u8>,
}

#[derive(Clone)]
pub struct NetworkInformation {
    pub ssid: String,
    pub psk: String,
    pub mac_addr: String,
    pub ip: String,
    pub port: u16,
    pub security_mode: Bluetooth::SecurityMode,
    pub ap_type: Bluetooth::AccessPointType,
}

/// The channel identifier for a frame
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
enum ChannelId {
    CONTROL,
    INPUT,
    SENSOR,
    VIDEO,
    MEDIA_AUDIO,
    SPEECH_AUDIO,
    SYSTEM_AUDIO,
    AV_INPUT,
    BLUETOOTH,
    NAVIGATION,
    MEDIA_STATUS,
    NONE = 255,
}

impl TryFrom<u8> for ChannelId {
    type Error = ();
    fn try_from(val: u8) -> Result<Self, Self::Error> {
        if val == ChannelId::CONTROL as u8 {
            Ok(ChannelId::CONTROL)
        } else if val == ChannelId::INPUT as u8 {
            Ok(ChannelId::INPUT)
        } else if val == ChannelId::SENSOR as u8 {
            Ok(ChannelId::SENSOR)
        } else if val == ChannelId::VIDEO as u8 {
            Ok(ChannelId::VIDEO)
        } else if val == ChannelId::MEDIA_AUDIO as u8 {
            Ok(ChannelId::MEDIA_AUDIO)
        } else if val == ChannelId::SPEECH_AUDIO as u8 {
            Ok(ChannelId::SPEECH_AUDIO)
        } else if val == ChannelId::SYSTEM_AUDIO as u8 {
            Ok(ChannelId::SYSTEM_AUDIO)
        } else if val == ChannelId::AV_INPUT as u8 {
            Ok(ChannelId::AV_INPUT)
        } else if val == ChannelId::BLUETOOTH as u8 {
            Ok(ChannelId::BLUETOOTH)
        } else if val == ChannelId::NAVIGATION as u8 {
            Ok(ChannelId::NAVIGATION)
        } else if val == ChannelId::MEDIA_STATUS as u8 {
            Ok(ChannelId::MEDIA_STATUS)
        } else if val == ChannelId::NONE as u8 {
            Ok(ChannelId::NONE)
        } else {
            Err(())
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
pub enum FrameHeaderType {
    Middle = 0,
    First = 1,
    Last = 2,
    Single = 3,
}

impl From<u8> for FrameHeaderType {
    fn from(value: u8) -> Self {
        match value & 3 {
            0 => FrameHeaderType::Middle,
            1 => FrameHeaderType::First,
            2 => FrameHeaderType::Last,
            _ => FrameHeaderType::Single,
        }
    }
}

impl Into<u8> for FrameHeaderType {
    fn into(self) -> u8 {
        self as u8
    }
}

bitfield::bitfield! {
    #[derive(Copy, Clone)]
    pub struct FrameHeaderContents(u8);
    impl Debug;
    impl new;
    u8;
    /// True indicates the frame is encrypted
    get_encryption, set_encryption: 3;
    from into FrameHeaderType, get_frame_type, set_frame_type: 2, 0;
    /// True when frame is for control, false when specific
    get_control, set_control: 4;
}

/// Represents the header of a frame sent to the android auto client
#[derive(Copy, Clone, Debug)]
struct FrameHeader {
    channel_id: ChannelId,
    frame: FrameHeaderContents,
}

impl FrameHeader {
    /// Add self to the given buffer to build part of a complete frame
    pub fn add_to(&self, buf: &mut Vec<u8>) {
        buf.push(self.channel_id as u8);
        buf.push(self.frame.0);
    }
}

struct FrameHeaderReceiver {
    channel_id: Option<ChannelId>,
}

impl FrameHeaderReceiver {
    pub fn new() -> Self {
        Self { channel_id: None }
    }
    pub async fn read(
        &mut self,
        stream: &mut tokio::net::TcpStream,
    ) -> Result<Option<FrameHeader>, String> {
        if self.channel_id.is_none() {
            let mut b = [0u8];
            stream.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            self.channel_id = ChannelId::try_from(b[0]).ok();
        }
        if let Some(channel_id) = &self.channel_id {
            let mut b = [0u8];
            stream.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            let mut a = FrameHeaderContents::new(false, FrameHeaderType::Single, false);
            a.0 = b[0];
            let fh = FrameHeader {
                channel_id: *channel_id,
                frame: a,
            };
            return Ok(Some(fh));
        }
        Ok(None)
    }
}

#[derive(Debug)]
struct AndroidAutoFrame {
    header: FrameHeader,
    data: Vec<u8>,
}

impl AndroidAutoFrame {
    const MAX_FRAME_DATA_SIZE: usize = 0x4000;
    fn build_multi_frame(f: FrameHeader, d: Vec<u8>) -> Vec<Self> {
        let mut m = Vec::new();
        if d.len() < Self::MAX_FRAME_DATA_SIZE {
            let fr = AndroidAutoFrame { header: f, data: d };
            m.push(fr);
        } else {
            let packets = d.chunks(Self::MAX_FRAME_DATA_SIZE);
            let max = packets.len();
            for (i, p) in packets.enumerate() {
                let first = i == 0;
                let last = i == (max - 1);
                let mut h = f.clone();
                if first {
                    h.frame.set_frame_type(FrameHeaderType::First);
                } else if last {
                    h.frame.set_frame_type(FrameHeaderType::Last);
                } else {
                    h.frame.set_frame_type(FrameHeaderType::Middle);
                }
                let fr = AndroidAutoFrame {
                    header: h,
                    data: p.to_vec(),
                };
                m.push(fr);
            }
        }
        m
    }
}

impl Into<Vec<u8>> for AndroidAutoFrame {
    fn into(mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.header.add_to(&mut buf);
        let mut p = (self.data.len() as u16).to_be_bytes().to_vec();
        buf.append(&mut p);
        buf.append(&mut self.data);
        buf
    }
}

struct AndroidAutoFrameReceiver {
    len: Option<u16>,
    data: Vec<u8>,
}

impl AndroidAutoFrameReceiver {
    fn new() -> Self {
        Self {
            len: None,
            data: Vec::new(),
        }
    }

    async fn read(
        &mut self,
        header: &FrameHeader,
        stream: &mut tokio::net::TcpStream,
    ) -> Result<Option<AndroidAutoFrame>, String> {
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.read_exact(&mut p).await.map_err(|e| e.to_string())?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream
                .read_exact(&mut self.data[0..*len as usize])
                .await
                .map_err(|e| e.to_string())?;
            let f = AndroidAutoFrame {
                header: header.clone(),
                data: self.data.clone(),
            };
            let f = Some(f);
            return Ok(f);
        }
        Ok(None)
    }
}

#[cfg(feature = "wireless")]
#[derive(Debug)]
enum AndroidAutoWifiMessage {
    VersionRequest,
    VersionResponse { major: u16, minor: u16, status: u16 },
    SslHandshake(Vec<u8>),
}

#[cfg(feature = "wireless")]
impl TryFrom<AndroidAutoFrame> for AndroidAutoWifiMessage {
    type Error = String;
    fn try_from(value: AndroidAutoFrame) -> Result<Self, Self::Error> {
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if ty == Wifi::ControlMessage::VERSION_RESPONSE as u16 {
            if value.data.len() == 8 {
                let major = u16::from_be_bytes([value.data[2], value.data[3]]);
                let minor = u16::from_be_bytes([value.data[4], value.data[5]]);
                let status = u16::from_be_bytes([value.data[6], value.data[7]]);
                Ok(AndroidAutoWifiMessage::VersionResponse {
                    major,
                    minor,
                    status,
                })
            } else {
                Err("Invalid version response packet".to_string())
            }
        } else if ty == Wifi::ControlMessage::SSL_HANDSHAKE as u16 {
            Ok(AndroidAutoWifiMessage::SslHandshake(value.data[2..].to_vec()))
        } else {
            Err(format!("Unknown packet type 0x{:x}", ty))
        }
    }
}

#[cfg(feature = "wireless")]
impl Into<AndroidAutoFrame> for AndroidAutoWifiMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            AndroidAutoWifiMessage::VersionRequest => {
                let mut m = Vec::with_capacity(4);
                let t = Wifi::ControlMessage::VERSION_REQUEST as u16;
                let t = t.to_be_bytes();
                let major = VERSION.0.to_be_bytes();
                let minor = VERSION.1.to_be_bytes();
                m.push(t[0]);
                m.push(t[1]);
                m.push(major[0]);
                m.push(major[1]);
                m.push(minor[0]);
                m.push(minor[1]);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: ChannelId::CONTROL,
                        frame: FrameHeaderContents::new(false, FrameHeaderType::Single, true),
                    },
                    data: m,
                }
            }
            AndroidAutoWifiMessage::SslHandshake(mut data) => {
                let mut m = Vec::with_capacity(4);
                let t = Wifi::ControlMessage::SSL_HANDSHAKE as u16;
                let t = t.to_be_bytes();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: ChannelId::CONTROL,
                        frame: FrameHeaderContents::new(false, FrameHeaderType::Single, false),
                    },
                    data: m,
                }
            }
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => {
                unimplemented!();
            }
        }
    }
}

enum AndroidAutoBluetoothMessage {
    SocketInfoRequest(Bluetooth::SocketInfoRequest),
    NetworkInfoMessage(Bluetooth::NetworkInfo),
}

impl AndroidAutoBluetoothMessage {
    fn as_message(&self) -> AndroidAutoMessage {
        use protobuf::Message;
        match self {
            AndroidAutoBluetoothMessage::SocketInfoRequest(m) => AndroidAutoMessage {
                t: Bluetooth::MessageId::BLUETOOTH_SOCKET_INFO_REQUEST as u16,
                message: m.write_to_bytes().unwrap(),
            },
            AndroidAutoBluetoothMessage::NetworkInfoMessage(m) => AndroidAutoMessage {
                t: Bluetooth::MessageId::BLUETOOTH_NETWORK_INFO_MESSAGE as u16,
                message: m.write_to_bytes().unwrap(),
            },
        }
    }
}

impl Into<Vec<u8>> for AndroidAutoMessage {
    fn into(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let b = self.message.len() as u16;
        let a = b.to_be_bytes();
        buf.push(a[0]);
        buf.push(a[1]);
        let a = self.t.to_be_bytes();
        buf.push(a[0]);
        buf.push(a[1]);
        for b in &self.message {
            buf.push(*b);
        }
        buf
    }
}

impl AndriodAutoBluettothServer {
    #[cfg(feature = "wireless")]
    pub async fn new(bluetooth: &mut bluetooth_rust::BluetoothHandler) -> Self {
        let profile = bluetooth_rust::RfcommProfile {
            uuid: bluetooth_rust::Uuid::parse_str(
                bluetooth_rust::BluetoothUuid::AndroidAuto.as_str(),
            )
            .unwrap(),
            name: Some("Android Auto Bluetooth Service".to_string()),
            service: bluetooth_rust::Uuid::parse_str(
                bluetooth_rust::BluetoothUuid::AndroidAuto.as_str(),
            )
            .ok(),
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
        Self { blue: a.unwrap() }
    }

    #[cfg(feature = "wireless")]
    pub async fn bluetooth_listen(&mut self, network: NetworkInformation) -> Result<(), String> {
        use futures::StreamExt;
        use protobuf::Message;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        log::info!("Listening for connections on android auto profile");
        loop {
            if let Some(cr) = self.blue.next().await {
                let network2 = network.clone();
                tokio::task::spawn(async move {
                    log::info!("Got a connection to android auto profile on {:?}", cr);
                    let stream = cr.accept().unwrap();
                    let (mut read, mut write) = stream.into_split();
                    let mut s = Bluetooth::SocketInfoRequest::new();
                    s.set_ip_address(network2.ip.clone());
                    s.set_port(network2.port as u32);

                    let m1 = AndroidAutoBluetoothMessage::SocketInfoRequest(s);
                    let m: AndroidAutoMessage = m1.as_message();
                    let mdata: Vec<u8> = m.into();
                    let r1 = write.write_all(&mdata).await;
                    loop {
                        let mut ty = [0u8; 2];
                        let mut len = [0u8; 2];
                        let r2 = read.read_exact(&mut len).await;
                        let r3 = read.read_exact(&mut ty).await;
                        let len = u16::from_be_bytes(len);
                        let ty = u16::from_be_bytes(ty);
                        let mut message = vec![0; len as usize];
                        read.read_exact(&mut message)
                            .await
                            .map_err(|e| e.to_string())?;
                        use protobuf::Enum;
                        match Bluetooth::MessageId::from_i32(ty as i32) {
                            Some(m) => match m {
                                Bluetooth::MessageId::BLUETOOTH_SOCKET_INFO_REQUEST => {
                                    log::error!("Got a socket info request {:x?}", message);
                                    break;
                                }
                                Bluetooth::MessageId::BLUETOOTH_NETWORK_INFO_REQUEST => {
                                    let mut response = Bluetooth::NetworkInfo::new();
                                    response.set_ssid(network2.ssid.clone());
                                    response.set_psk(network2.psk.clone());
                                    response.set_mac_addr(network2.mac_addr.clone());
                                    response.set_security_mode(network2.security_mode.clone());
                                    response.set_ap_type(network2.ap_type.clone());
                                    let response =
                                        AndroidAutoBluetoothMessage::NetworkInfoMessage(response);
                                    let m: AndroidAutoMessage = response.as_message();
                                    let mdata: Vec<u8> = m.into();
                                    let r1 = write.write_all(&mdata).await;
                                }
                                Bluetooth::MessageId::BLUETOOTH_SOCKET_INFO_RESPONSE => {
                                    let message =
                                        Bluetooth::SocketInfoResponse::parse_from_bytes(&message);
                                    log::info!("Message is now {:?}", message);
                                }
                                _ => {}
                            },
                            _ => {
                                log::error!("Unknown packet {} {:x?}", ty, message);
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

    async fn handle_client(stream: &mut tokio::net::TcpStream, addr: std::net::SocketAddr, network: NetworkInformation) -> Result<(), String> {
        use std::sync::Arc;
        use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
        use tokio::io::AsyncWriteExt;

        let mut root_store = rustls::RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        );
        
        let aautocertder = {
            let mut br = std::io::Cursor::new(cert::AAUTO_CERT.to_string().as_bytes().to_vec());
            let aautocertpem = rustls::pki_types::pem::from_buf(&mut br).expect("Failed to parse pem for aauto server").expect("Invalid pem cert for aauto server");
            CertificateDer::from_pem(aautocertpem.0, aautocertpem.1).unwrap()
        };
        let cert = {
            let mut br = std::io::Cursor::new(cert::CERTIFICATE.to_string().as_bytes().to_vec());
            let aautocertpem = rustls::pki_types::pem::from_buf(&mut br).expect("Failed to parse pem for aauto client").expect("Invalid pem cert for aauto client");
            CertificateDer::from_pem(aautocertpem.0, aautocertpem.1).unwrap()
        };
        let key = {
            let mut br = std::io::Cursor::new(cert::PRIVATE_KEY.to_string().as_bytes().to_vec());
            let aautocertpem = rustls::pki_types::pem::from_buf(&mut br).expect("Failed to parse pem for aauto client").expect("Invalid pem cert for aauto client");
            PrivateKeyDer::from_pem(aautocertpem.0, aautocertpem.1).unwrap()
        };
        let cert = vec![cert];
        log::debug!("AAuto cert: {:?}", aautocertder);
        let ckey = CertifiedKey::from_der(cert, key, rustls::crypto::CryptoProvider::get_default().unwrap()).unwrap();
        let ckey = Arc::new(ckey);
        let resolver = Arc::new(rustls::client::AlwaysResolvesClientRawPublicKeys::new(ckey));
        root_store.add(aautocertder).expect("Failed to load android auto server cert");
        let ssl_client_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_cert_resolver(resolver);
        let config = Arc::new(ssl_client_config);
        let server = "idontknow.com".try_into().unwrap();
        let mut ssl_client = rustls::ClientConnection::new(config, server).expect("Failed to build ssl client");
        log::error!("SSL WANTS RX {} TX {}", ssl_client.wants_read(), ssl_client.wants_write());
        log::debug!("Got a connection on port {} from {:?}", network.port, addr);
        let m = AndroidAutoWifiMessage::VersionRequest;
        let d: AndroidAutoFrame = m.into();
        let d2: Vec<u8> = d.into();
        stream.write_all(&d2).await;
        loop {
            let mut fr = FrameHeaderReceiver::new();
            let f = loop {
                if let Ok(Some(f)) = fr.read(stream).await {
                    break f;
                }
            };
            let mut fr2 = AndroidAutoFrameReceiver::new();
            let f2 = loop {
                if let Ok(Some(f2)) = fr2.read(&f, stream).await {
                    break f2;
                }
            };
            let message: Result<AndroidAutoWifiMessage, String> = f2.try_into();
            match message {
                Err(e) => {
                    log::error!("Error receiving packet: {}", e);
                    break;
                }
                Ok(m) => match m {
                    AndroidAutoWifiMessage::SslHandshake(data) => {
                        log::info!("SSL Handshake data is {:x?}", data);
                        log::error!("SSL WANTS RX {} TX {}", ssl_client.wants_read(), ssl_client.wants_write());
                        if ssl_client.wants_read() {
                            let mut dc = std::io::Cursor::new(data);
                            let asdf = ssl_client.read_tls(&mut dc);
                            log::error!("SSL Client process received handshake is {:?}", asdf);
                            let asdg = ssl_client.process_new_packets();
                            log::error!("Process new packets from SSL: {:?}", asdg);
                        }
                        log::error!("SSL WANTS RX {} TX {}", ssl_client.wants_read(), ssl_client.wants_write());
                        log::error!("ssl handshaking {}", ssl_client.is_handshaking());
                        if ssl_client.wants_write() {
                            let mut s = Vec::new();
                            let l = ssl_client.write_tls(&mut s);
                            if let Ok(l) = l {
                                log::debug!("Got buffer length {} to send for ssl stuff {:x?}", l, s);
                                let m = AndroidAutoWifiMessage::SslHandshake(s);
                                let d: AndroidAutoFrame = m.into();
                                let d2: Vec<u8> = d.into();
                                stream.write_all(&d2).await;
                            }
                        }
                    }
                    AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
                    AndroidAutoWifiMessage::VersionResponse {
                        major,
                        minor,
                        status,
                    } => {
                        if status == 0xFFFF {
                            log::error!("Version mismatch");
                            break;
                        }
                        log::info!(
                            "Android auto client version: {}.{}",
                            major,
                            minor
                        );
                        let mut s = Vec::new();
                        if ssl_client.wants_write() {
                            let l = ssl_client.write_tls(&mut s);
                            if let Ok(l) = l {
                                log::debug!("Got buffer length {} to send for ssl stuff {:x?}", l, s);
                                let m = AndroidAutoWifiMessage::SslHandshake(s);
                                let d: AndroidAutoFrame = m.into();
                                let d2: Vec<u8> = d.into();
                                let a = stream.write_all(&d2).await;
                            }
                        }
                    }
                },
            }
        }
        log::info!("Disconnecting normally");
        Ok(())
    }

    #[cfg(feature = "wireless")]
    pub async fn wifi_listen(network: NetworkInformation) -> Result<(), String> {
        let cp = rustls::crypto::ring::default_provider();
        cp.install_default().expect("Failed to set ssl provider");

        log::info!("Listening on port {} for android auto stuff", network.port);
        if let Ok(a) = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", network.port)).await {
            loop {
                if let Ok((mut stream, addr)) = a.accept().await {
                    let network2 = network.clone();
                    tokio::task::spawn(async move {
                        if let Err(e) = Self::handle_client(&mut stream, addr, network2).await {
                            log::error!("Disconnect from client: {:?}", e);
                        }
                    });
                }
            }
        } else {
            Err(format!("Failed to listen on port {} tcp", network.port))
        }
    }

    #[cfg(not(feature = "wireless"))]
    pub async fn new() -> Self {
        Self {}
    }
}
