use std::collections::VecDeque;

use openssl::ssl::SslVerifyMode;

mod cert;

use protobuf::Message;
use Wifi::ChannelDescriptor;

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

#[derive(Clone)]
pub struct HeadUnitInfo {
    pub name: String,
    pub car_model: String,
    pub car_year: String,
    pub car_serial: String,
    pub left_hand: bool,
    pub head_manufacturer: String,
    pub head_model: String,
    pub sw_build: String,
    pub sw_version: String,
    pub native_media: bool,
    pub hide_clock: Option<bool>,
}

#[derive(Clone)]
pub struct AndroidAutoConfiguration {
    pub network: NetworkInformation,
    pub unit: HeadUnitInfo,
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
    pub fn read(
        &mut self,
        stream: &mut std::net::TcpStream,
    ) -> Result<Option<FrameHeader>, String> {
        use std::io::Read;
        if self.channel_id.is_none() {
            let mut b = [0u8];
            stream.read_exact(&mut b).map_err(|e| e.to_string())?;
            self.channel_id = ChannelId::try_from(b[0]).ok();
        }
        if let Some(channel_id) = &self.channel_id {
            let mut b = [0u8];
            stream.read_exact(&mut b).map_err(|e| e.to_string())?;
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

    fn build_vec(&self) -> Vec<u8> {
        let mut data = self.data.clone();
        let mut buf = Vec::new();
        self.header.add_to(&mut buf);
        let mut p = (self.data.len() as u16).to_be_bytes().to_vec();
        buf.append(&mut p);
        buf.append(&mut data);
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

    fn read_plain(&mut self, header: &FrameHeader, stream: &mut std::net::TcpStream) -> Result<Option<AndroidAutoFrame>, String> {
        use std::io::Read;
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.read_exact(&mut p).map_err(|e| e.to_string())?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream.read_exact(&mut self.data[0..*len as usize])
                .map_err(|e| e.to_string())?;
            log::info!("Got {} bytes of frame data", len - 2);
            let f = AndroidAutoFrame {
                header: header.clone(),
                data: self.data.clone(),
            };
            let f = Some(f);
            return Ok(f);
        }
        Ok(None)
    }

    fn read(
        &mut self,
        header: &FrameHeader,
        stream: &mut openssl::ssl::SslStream<OpensslSocket>,
    ) -> Result<Option<AndroidAutoFrame>, String> {
        use std::io::Read;
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.get_mut().plain.read_exact(&mut p).map_err(|e| e.to_string())?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream.get_mut().plain
                .read_exact(&mut self.data[0..*len as usize])
                .map_err(|e| e.to_string())?;
            let data = if header.frame.get_encryption() {
                stream.get_mut().relay_data(&self.data);
                let mut data = vec![0; *len as usize];
                stream.ssl_read(&mut data);
                data
            } else {
                self.data.clone()
            };
            let f = AndroidAutoFrame {
                header: header.clone(),
                data,
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
    SslAuthComplete(bool),
    ServiceDiscoveryRequest(Wifi::ServiceDiscoveryRequest),
    ServiceDiscoveryResponse(Wifi::ServiceDiscoveryResponse),
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
        } else if ty == Wifi::ControlMessage::SERVICE_DISCOVERY_REQUEST as u16 {
            let mut bytes = value.data.clone()
                .into_iter()
                .rev()
                .skip_while(|&byte| byte == 0)
                .collect::<Vec<_>>();
            bytes.reverse();
            let m = Wifi::ServiceDiscoveryRequest::parse_from_bytes(&bytes[2..]);
            match m {
                Ok(m) => Ok(AndroidAutoWifiMessage::ServiceDiscoveryRequest(m)),
                Err(e) => Err(format!("Invalid service discovery request: {}", e.to_string()))
            }
        } else {
            Err(format!("Unknown packet type 0x{:x}", ty))
        }
    }
}

#[cfg(feature = "wireless")]
impl Into<AndroidAutoFrame> for AndroidAutoWifiMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::ControlMessage::SERVICE_DISCOVERY_RESPONSE as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: ChannelId::CONTROL,
                        frame: FrameHeaderContents::new(true, FrameHeaderType::Single, false),
                    },
                    data: m,
                }
            }
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
            AndroidAutoWifiMessage::SslAuthComplete(status) => {
                let mut m = Wifi::AuthCompleteIndication::new();
                let status = if status { Wifi::AuthCompleteIndicationStatus::OK } else { Wifi::AuthCompleteIndicationStatus::FAIL };
                m.set_status(status);
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::ControlMessage::AUTH_COMPLETE as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
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
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
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

#[derive(Debug)]
struct OpensslSocket {
    pub plain: std::net::TcpStream,
    recvd: VecDeque<u8>,
}

impl OpensslSocket {
    fn new(plain: std::net::TcpStream,) -> Self {
        Self {
            plain,
            recvd: VecDeque::new(),
        }
    }

    fn relay_data(&mut self, d: &[u8]) {
        for b in d {
            self.recvd.push_back(*b);
        }
    }

    fn receive_frame(&mut self) -> Result<AndroidAutoWifiMessage, String> {
        let mut fr = FrameHeaderReceiver::new();
        let f = loop {
            if let Ok(Some(f)) = fr.read(&mut self.plain) {
                break f;
            }
        };
        let mut fr2 = AndroidAutoFrameReceiver::new();
        let f2 = loop {
            if let Ok(Some(f2)) = fr2.read_plain(&f, &mut self.plain) {
                break f2;
            }
        };
        f2.try_into()
    }
}

impl std::io::Read for OpensslSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        log::info!("Reading from openssl socket, len: {}", buf.len());
        if self.recvd.len() < buf.len() {
            match self.receive_frame() {
                Ok(m) => {
                    log::info!("SSL GOT FRAME {:?}", m);
                    match m {
                        AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                        AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                        AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
                        AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
                        AndroidAutoWifiMessage::VersionResponse { major: _, minor: _, status: _ } => unimplemented!(),
                        AndroidAutoWifiMessage::SslHandshake(items) => {
                            for i in items {
                                self.recvd.push_back(i);
                            }
                        },
                    }
                }
                Err(e) => {
                    return Err(std::io::Error::other(e));
                }
            }
        }
        let len = buf.len();
        for b in buf {
            *b = self.recvd.pop_front().unwrap();
        }
        Ok(len)
    }
}

impl std::io::Write for OpensslSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let m = AndroidAutoWifiMessage::SslHandshake(buf.to_vec());
        let d: AndroidAutoFrame = m.into();
        let d2: Vec<u8> = d.build_vec();
        log::info!("Writing to openssl socket: {:x?}", d2);
        self.plain.write_all(&d2)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.plain.flush()
    }
}

fn channels() -> Vec<ChannelDescriptor> {
    Vec::new()
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
                        read.read_exact(&mut len).await.map_err(|e| e.to_string())?;
                        read.read_exact(&mut ty).await.map_err(|e| e.to_string())?;
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

    fn handle_client(mut stream: std::net::TcpStream, addr: std::net::SocketAddr, config: AndroidAutoConfiguration) -> Result<(), String> {
        use std::io::Write;

        log::debug!("Got a connection on port {} from {:?}", config.network.port, addr);
        let openssl_socket = OpensslSocket::new(stream);
        let client_cert = openssl::x509::X509::from_pem(cert::CERTIFICATE.as_bytes()).expect("Failed to load client ssl certificate");
        let client_key = openssl::pkey::PKey::private_key_from_pem(cert::PRIVATE_KEY.as_bytes()).unwrap();
        let mut ssl_con = openssl::ssl::SslContext::builder(openssl::ssl::SslMethod::tls_client()).unwrap();
        ssl_con.set_certificate(&*client_cert).unwrap();
        ssl_con.set_private_key(&client_key).unwrap();
        let ssl_con = ssl_con.build();
        let mut ssl = openssl::ssl::Ssl::new(&(*ssl_con)).unwrap();
        ssl.set_connect_state();
        ssl.set_verify(SslVerifyMode::NONE);
        let mut openssl_stream = openssl::ssl::SslStream::new(ssl, openssl_socket).expect("Failed to build openssl stream");
        let m = AndroidAutoWifiMessage::VersionRequest;
        let d: AndroidAutoFrame = m.into();
        let d2: Vec<u8> = d.build_vec();
        openssl_stream.get_mut().plain.write_all(&d2).map_err(|e| e.to_string())?;
        loop {
            let mut fr = FrameHeaderReceiver::new();
            let f = loop {
                if let Ok(Some(f)) = fr.read(&mut openssl_stream.get_mut().plain) {
                    break f;
                }
            };
            let mut fr2 = AndroidAutoFrameReceiver::new();
            let f2 = loop {
                if let Ok(Some(f2)) = fr2.read(&f, &mut openssl_stream) {
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
                    AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                    AndroidAutoWifiMessage::ServiceDiscoveryRequest(m) => {
                        log::error!("Got service discovery request: {:?}", m);
                        let mut m = Wifi::ServiceDiscoveryResponse::new();
                        m.set_car_model(config.unit.car_model.clone());
                        m.set_can_play_native_media_during_vr(config.unit.native_media);
                        m.set_car_serial(config.unit.car_serial.clone());
                        m.set_car_year(config.unit.car_year.clone());
                        m.set_head_unit_name(config.unit.name.clone());
                        m.set_headunit_manufacturer(config.unit.head_manufacturer.clone());
                        m.set_headunit_model(config.unit.head_model.clone());
                        if let Some(hide) = config.unit.hide_clock {
                            m.set_hide_clock(hide);
                        }
                        m.set_left_hand_drive_vehicle(config.unit.left_hand);
                        m.set_sw_build(config.unit.sw_build.clone());
                        m.set_sw_version(config.unit.sw_version.clone());
                        for s in channels() {
                            m.channels.push(s);
                        }
                        let m = AndroidAutoWifiMessage::ServiceDiscoveryResponse(m);
                        let d: AndroidAutoFrame = m.into();
                        let d2: Vec<u8> = d.build_vec();
                        openssl_stream.get_mut().plain.write_all(&d2).map_err(|e| e.to_string())?;
                    }
                    AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
                    AndroidAutoWifiMessage::SslHandshake(data) => {
                        log::info!("SSL Handshake data is {:x?}", data);
                        todo!();
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
                        openssl_stream.do_handshake().map_err(|e| e.to_string()).expect("Failed to ssl connect?");
                        log::error!("Stuff after trying to connect: {:x?}", openssl_stream.get_ref());
                        let m = AndroidAutoWifiMessage::SslAuthComplete(true);
                        let d: AndroidAutoFrame = m.into();
                        let d2: Vec<u8> = d.build_vec();
                        openssl_stream.get_mut().plain.write_all(&d2).map_err(|e| e.to_string())?;
                    }
                },
            }
        }
        log::info!("Disconnecting normally");
        Ok(())
    }

    #[cfg(feature = "wireless")]
    pub fn wifi_listen(config: AndroidAutoConfiguration) -> Result<(), String> {
        log::info!("Listening on port {} for android auto stuff", config.network.port);
        if let Ok(a) = std::net::TcpListener::bind(format!("0.0.0.0:{}", config.network.port)) {
            loop {
                if let Ok((stream, addr)) = a.accept() {
                    let config2 = config.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_client(stream, addr, config2) {
                            log::error!("Disconnect from client: {:?}", e);
                        }
                    });
                }
            }
        } else {
            Err(format!("Failed to listen on port {} tcp", config.network.port))
        }
    }

    #[cfg(not(feature = "wireless"))]
    pub async fn new() -> Self {
        Self {}
    }
}
