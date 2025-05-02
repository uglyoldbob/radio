use std::collections::{BTreeMap, VecDeque};

use openssl::ssl::SslVerifyMode;

mod cert;

use Wifi::ChannelDescriptor;
use protobuf::{Enum, EnumOrUnknown, Message};

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
pub struct BluetoothInformation {
    pub address: String,
}

#[derive(Clone)]
pub struct AndroidAutoConfiguration {
    pub network: NetworkInformation,
    pub bluetooth: BluetoothInformation,
    pub unit: HeadUnitInfo,
}

/// The channel identifier for a frame
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
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
    from into FrameHeaderType, get_frame_type, set_frame_type: 1, 0;
    /// True when frame is for control, false when specific
    get_control, set_control: 2;
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
    ) -> Result<Option<FrameHeader>, std::io::Error> {
        use std::io::Read;
        if self.channel_id.is_none() {
            let mut b = [0u8];
            stream.read_exact(&mut b)?;
            self.channel_id = ChannelId::try_from(b[0]).ok();
        }
        if let Some(channel_id) = &self.channel_id {
            let mut b = [0u8];
            stream.read_exact(&mut b)?;
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

    fn build_vec(&self, stream: Option<&mut openssl::ssl::SslStream<OpensslSocket>>) -> Vec<u8> {
        let mut buf = Vec::new();
        self.header.add_to(&mut buf);
        log::error!(
            "Sending frame {:?} {:x?} {:x?}",
            self.header,
            buf,
            self.data
        );
        if self.header.frame.get_encryption() {
            if let Some(stream) = stream {
                stream.ssl_write(&self.data).unwrap();
                let mut data = Vec::with_capacity(self.data.len());
                stream.get_mut().get_tx_data(&mut data);
                let mut p = (data.len() as u16).to_be_bytes().to_vec();
                buf.append(&mut p);
                buf.append(&mut data);
            } else {
                panic!("No ssl object when encryption was required");
            }
        } else {
            let mut data = self.data.clone();
            let mut p = (data.len() as u16).to_be_bytes().to_vec();
            buf.append(&mut p);
            buf.append(&mut data);
        }
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

    fn read_plain(
        &mut self,
        header: &FrameHeader,
        stream: &mut std::net::TcpStream,
    ) -> Result<Option<AndroidAutoFrame>, String> {
        use std::io::Read;
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.read_exact(&mut p).map_err(|e| e.to_string())?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream
                .read_exact(&mut self.data[0..*len as usize])
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
    ) -> Result<Option<AndroidAutoFrame>, std::io::Error> {
        use std::io::Read;
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.get_mut().plain.read_exact(&mut p)?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream
                .get_mut()
                .plain
                .read_exact(&mut self.data[0..*len as usize])?;
            let data = if header.frame.get_encryption() {
                stream.get_mut().relay_data(&self.data);
                let mut data = vec![0; *len as usize];
                stream.ssl_read(&mut data).map_err(|e| {
                    let e2 = e.to_string();
                    std::io::Error::new(std::io::ErrorKind::Other, e2)
                })?;
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
    AudioFocusRequest(Wifi::AudioFocusRequest),
    AudioFocusResponse(Wifi::AudioFocusResponse),
    ChannelOpenRequest(Wifi::ChannelOpenRequest),
    ChannelOpenResponse(ChannelId, Wifi::ChannelOpenResponse),
    PingRequest(Wifi::PingRequest),
    PingResponse(Wifi::PingResponse),
    SpecificMessage(Vec<u8>),
}

#[cfg(feature = "wireless")]
impl TryFrom<AndroidAutoFrame> for (ChannelId, AndroidAutoWifiMessage) {
    type Error = String;
    fn try_from(value: AndroidAutoFrame) -> Result<Self, Self::Error> {
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if value.header.channel_id == ChannelId::CONTROL || value.header.frame.get_control() {
            let w = Wifi::ControlMessage::from_i32(ty as i32);
            if let Some(m) = w {
                let v = match m {
                    Wifi::ControlMessage::VERSION_REQUEST => unimplemented!(),
                    Wifi::ControlMessage::AUTH_COMPLETE => unimplemented!(),
                    Wifi::ControlMessage::MESSAGE_NONE => unimplemented!(),
                    Wifi::ControlMessage::SERVICE_DISCOVERY_RESPONSE => unimplemented!(),
                    Wifi::ControlMessage::CHANNEL_OPEN_RESPONSE => unimplemented!(),
                    Wifi::ControlMessage::PING_REQUEST => {
                        let mut bytes = value
                            .data
                            .clone()
                            .into_iter()
                            .rev()
                            .skip_while(|&byte| byte == 0)
                            .collect::<Vec<_>>();
                        bytes.reverse();
                        let m = Wifi::PingRequest::parse_from_bytes(&bytes[2..]);
                        match m {
                            Ok(m) => Ok(AndroidAutoWifiMessage::PingRequest(m)),
                            Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                        }
                    }
                    Wifi::ControlMessage::NAVIGATION_FOCUS_REQUEST => unimplemented!(),
                    Wifi::ControlMessage::NAVIGATION_FOCUS_RESPONSE => unimplemented!(),
                    Wifi::ControlMessage::SHUTDOWN_REQUEST => unimplemented!(),
                    Wifi::ControlMessage::SHUTDOWN_RESPONSE => unimplemented!(),
                    Wifi::ControlMessage::VOICE_SESSION_REQUEST => unimplemented!(),
                    Wifi::ControlMessage::AUDIO_FOCUS_RESPONSE => unimplemented!(),
                    Wifi::ControlMessage::PING_RESPONSE => {
                        let mut bytes = value
                            .data
                            .clone()
                            .into_iter()
                            .rev()
                            .skip_while(|&byte| byte == 0)
                            .collect::<Vec<_>>();
                        bytes.reverse();
                        let m = Wifi::PingResponse::parse_from_bytes(&bytes[2..]);
                        match m {
                            Ok(m) => Ok(AndroidAutoWifiMessage::PingResponse(m)),
                            Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                        }
                    }
                    Wifi::ControlMessage::AUDIO_FOCUS_REQUEST => {
                        let mut bytes = value
                            .data
                            .clone()
                            .into_iter()
                            .rev()
                            .skip_while(|&byte| byte == 0)
                            .collect::<Vec<_>>();
                        bytes.reverse();
                        let m = Wifi::AudioFocusRequest::parse_from_bytes(&bytes[2..]);
                        match m {
                            Ok(m) => Ok(AndroidAutoWifiMessage::AudioFocusRequest(m)),
                            Err(e) => Err(format!("Invalid audio focus request: {}", e.to_string())),
                        }
                    }
                    Wifi::ControlMessage::VERSION_RESPONSE => {
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
                    }
                    Wifi::ControlMessage::SSL_HANDSHAKE => Ok(AndroidAutoWifiMessage::SslHandshake(
                        value.data[2..].to_vec(),
                    )),
                    Wifi::ControlMessage::CHANNEL_OPEN_REQUEST => {
                        let mut bytes = value
                            .data
                            .clone()
                            .into_iter()
                            .rev()
                            .skip_while(|&byte| byte == 0)
                            .collect::<Vec<_>>();
                        bytes.reverse();
                        let m = Wifi::ChannelOpenRequest::parse_from_bytes(&bytes[2..]);
                        match m {
                            Ok(m) => Ok(AndroidAutoWifiMessage::ChannelOpenRequest(m)),
                            Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                        }
                    }
                    Wifi::ControlMessage::SERVICE_DISCOVERY_REQUEST => {
                        let mut bytes = value
                            .data
                            .clone()
                            .into_iter()
                            .rev()
                            .skip_while(|&byte| byte == 0)
                            .collect::<Vec<_>>();
                        bytes.reverse();
                        let m = Wifi::ServiceDiscoveryRequest::parse_from_bytes(&bytes[2..]);
                        match m {
                            Ok(m) => Ok(AndroidAutoWifiMessage::ServiceDiscoveryRequest(m)),
                            Err(e) => Err(format!(
                                "Invalid service discovery request: {}",
                                e.to_string()
                            )),
                        }
                    }
                };
                Ok((value.header.channel_id, v?))
            } else {
                Err(format!("Unknown packet type 0x{:x}", ty))
            }
        }
        else {
            Err(format!("Unhandled specific message for channel {:?} {:x?}", value.header.channel_id, &value.data[2..]))
        }
    }
}

#[cfg(feature = "wireless")]
impl Into<AndroidAutoFrame> for AndroidAutoWifiMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::ControlMessage::PING_REQUEST as u16;
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
            AndroidAutoWifiMessage::ChannelOpenResponse(chan, m) => {
                log::error!("Channel open response {}", m.is_initialized());
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::ControlMessage::CHANNEL_OPEN_RESPONSE as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: chan,
                        frame: FrameHeaderContents::new(true, FrameHeaderType::Single, true),
                    },
                    data: m,
                }
            }
            AndroidAutoWifiMessage::ChannelOpenRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(m) => {
                log::error!("Audio focus response {}", m.is_initialized());
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::ControlMessage::AUDIO_FOCUS_RESPONSE as u16;
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
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(m) => {
                log::error!("Service discovery response {}", m.is_initialized());
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
                        frame: FrameHeaderContents::new(false, FrameHeaderType::Single, false),
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
                let status = if status {
                    Wifi::AuthCompleteIndicationStatus::OK
                } else {
                    Wifi::AuthCompleteIndicationStatus::FAIL
                };
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
    send: VecDeque<u8>,
    handshake: bool,
}

impl OpensslSocket {
    fn new(plain: std::net::TcpStream) -> Self {
        Self {
            plain,
            handshake: true,
            recvd: VecDeque::new(),
            send: VecDeque::new(),
        }
    }

    fn relay_data(&mut self, d: &[u8]) {
        for b in d {
            self.recvd.push_back(*b);
        }
    }

    fn get_tx_data(&mut self, d: &mut Vec<u8>) {
        while let Some(b) = self.send.pop_front() {
            d.push(b);
        }
    }

    fn receive_frame(&mut self) -> Result<(ChannelId, AndroidAutoWifiMessage), String> {
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
        if self.recvd.len() < buf.len() {
            match self.receive_frame() {
                Ok((chan, m)) => match m {
                    AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
                    AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
                    AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
                    AndroidAutoWifiMessage::ChannelOpenRequest(_) => unimplemented!(),
                    AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                    AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
                    AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
                    AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                    AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                    AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
                    AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
                    AndroidAutoWifiMessage::VersionResponse {
                        major: _,
                        minor: _,
                        status: _,
                    } => unimplemented!(),
                    AndroidAutoWifiMessage::SslHandshake(items) => {
                        for i in items {
                            self.recvd.push_back(i);
                        }
                    }
                },
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
        if self.handshake {
            let m = AndroidAutoWifiMessage::SslHandshake(buf.to_vec());
            let d: AndroidAutoFrame = m.into();
            let d2: Vec<u8> = d.build_vec(None);
            self.plain.write_all(&d2)?;
        } else {
            for b in buf {
                self.send.push_back(*b);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.plain.flush()
    }
}

fn channels(config: &AndroidAutoConfiguration) -> Vec<ChannelDescriptor> {
    let mut c = Vec::new();

    // av input channel
    {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::AV_INPUT as u8 as u32);
        let mut avchan = Wifi::AVInputChannel::new();
        avchan.set_available_while_in_call(true);
        avchan.set_stream_type(Wifi::avstream_type::Enum::AUDIO);
        let mut ac = Wifi::AudioConfig::new();
        ac.set_bit_depth(16);
        ac.set_channel_count(1);
        ac.set_sample_rate(16000);
        avchan.audio_config.0.replace(Box::new(ac));
        chan.av_input_channel.0.replace(Box::new(avchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // system audio channel
    {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::SYSTEM_AUDIO as u8 as u32);
        let mut avchan = Wifi::AVChannel::new();
        avchan.set_audio_type(Wifi::audio_type::Enum::SYSTEM);
        avchan.set_available_while_in_call(true);
        avchan.set_stream_type(Wifi::avstream_type::Enum::AUDIO);
        let mut ac = Wifi::AudioConfig::new();
        ac.set_bit_depth(16);
        ac.set_channel_count(1);
        ac.set_sample_rate(16000);
        avchan.audio_configs.push(ac);
        chan.av_channel.0.replace(Box::new(avchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // speech audio channel
    {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::SPEECH_AUDIO as u8 as u32);
        let mut avchan = Wifi::AVChannel::new();
        avchan.set_audio_type(Wifi::audio_type::Enum::SPEECH);
        avchan.set_available_while_in_call(true);
        avchan.set_stream_type(Wifi::avstream_type::Enum::AUDIO);
        let mut ac = Wifi::AudioConfig::new();
        ac.set_bit_depth(16);
        ac.set_channel_count(1);
        ac.set_sample_rate(16000);
        avchan.audio_configs.push(ac);
        chan.av_channel.0.replace(Box::new(avchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // media audio channel
    if false {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::MEDIA_AUDIO as u8 as u32);
        let mut avchan = Wifi::AVChannel::new();
        avchan.set_audio_type(Wifi::audio_type::Enum::MEDIA);
        avchan.set_available_while_in_call(true);
        avchan.set_stream_type(Wifi::avstream_type::Enum::AUDIO);
        let mut ac = Wifi::AudioConfig::new();
        ac.set_bit_depth(16);
        ac.set_channel_count(2);
        ac.set_sample_rate(48000);
        avchan.audio_configs.push(ac);
        chan.av_channel.0.replace(Box::new(avchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // sensor channel
    {
        let mut chan = ChannelDescriptor::new();
        let mut sensor = Wifi::SensorChannel::new();
        let mut sensors = Vec::new();
        sensors.push({
            let mut sensor1 = Wifi::Sensor::new();
            sensor1.set_type(Wifi::sensor_type::Enum::COMPASS);
            sensor1
        });
        for s in sensors {
            sensor.sensors.push(s);
        }
        chan.sensor_channel.0.replace(Box::new(sensor));
        chan.set_channel_id(ChannelId::SENSOR as u8 as u32);
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // video channel
    {
        let mut chan = ChannelDescriptor::new();
        let mut avchan = Wifi::AVChannel::new();
        chan.set_channel_id(ChannelId::VIDEO as u8 as u32);
        avchan.set_stream_type(Wifi::avstream_type::Enum::VIDEO);
        avchan.set_available_while_in_call(true);
        avchan.set_audio_type(Wifi::audio_type::Enum::SYSTEM);
        let mut vconfs = Vec::new();
        vconfs.push({
            let mut vc = Wifi::VideoConfig::new();
            vc.set_video_resolution(Wifi::video_resolution::Enum::_480p);
            vc.set_video_fps(Wifi::video_fps::Enum::_30);
            vc.set_dpi(300);
            vc.set_additional_depth(0);
            vc.set_margin_height(0);
            vc.set_margin_width(0);
            if !vc.is_initialized() {
                panic!();
            }
            vc
        });
        for v in vconfs {
            avchan.video_configs.push(v);
        }

        chan.av_channel.0.replace(Box::new(avchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    //navigation status channel
    {
        let mut chan = ChannelDescriptor::new();
        let mut navchan = Wifi::NavigationChannel::new();
        navchan.set_minimum_interval_ms(1000);
        navchan.set_type(Wifi::navigation_turn_type::Enum::IMAGE);
        let mut io = Wifi::NavigationImageOptions::new();
        io.set_colour_depth_bits(16);
        io.set_dunno(255);
        io.set_height(256);
        io.set_width(256);
        navchan.image_options.0.replace(Box::new(io));
        chan.set_channel_id(ChannelId::NAVIGATION as u8 as u32);
        chan.navigation_channel.0.replace(Box::new(navchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // media status service channel
    {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::MEDIA_STATUS as u8 as u32);
        let mchan = Wifi::MediaInfoChannel::new();
        chan.media_infoChannel.0.replace(Box::new(mchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    // input channel
    {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::INPUT as u8 as u32);
        let mut ichan = Wifi::InputChannel::new();
        let mut tc = Wifi::TouchConfig::new();
        tc.set_height(480);
        tc.set_width(800);
        ichan.touch_screen_config.0.replace(Box::new(tc));
        chan.input_channel.0.replace(Box::new(ichan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    //bluetooth channel
    if false {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(ChannelId::BLUETOOTH as u8 as u32);
        let mut bchan = Wifi::BluetoothChannel::new();
        bchan.set_adapter_address(config.bluetooth.address.clone());
        let meth = Wifi::bluetooth_pairing_method::Enum::HFP;
        bchan
            .supported_pairing_methods
            .push(EnumOrUnknown::new(meth));
        chan.bluetooth_channel.0.replace(Box::new(bchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        c.push(chan);
    }
    c
}

#[enum_dispatch::enum_dispatch]
trait ChannelHandlerTrait {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error>;
}

struct InputChannelHandler {}

impl ChannelHandlerTrait for InputChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for input: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct MediaAudioChannelHandler {}

impl ChannelHandlerTrait for MediaAudioChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for media audio: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct MediaStatusChannelHandler {}

impl ChannelHandlerTrait for MediaStatusChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for media status: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct NavigationChannelHandler {}

impl ChannelHandlerTrait for NavigationChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for navigation: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct VideoChannelHandler {}

impl ChannelHandlerTrait for VideoChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for video: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct SensorChannelHandler {}

impl ChannelHandlerTrait for SensorChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for sensor: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct SpeechAudioChannelHandler {}

impl ChannelHandlerTrait for SpeechAudioChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for speech audio: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct SystemAudioChannelHandler {}

impl ChannelHandlerTrait for SystemAudioChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for system audio: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct AvInputChannelHandler {}

impl ChannelHandlerTrait for AvInputChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for av input: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

struct BluetoothChannelHandler {}

impl ChannelHandlerTrait for BluetoothChannelHandler {
    fn receive_data(
        &mut self,
        channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => todo!("{:x?}", m),
            AndroidAutoWifiMessage::PingResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslAuthComplete(_) => unimplemented!(),
            AndroidAutoWifiMessage::SslHandshake(_) => unimplemented!(),
            AndroidAutoWifiMessage::VersionRequest => unimplemented!(),
            AndroidAutoWifiMessage::VersionResponse {
                major: _,
                minor: _,
                status: _,
            } => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => {
                log::info!("Got channel open request for bluetooth: {:?}", m);
                let mut m2 = Wifi::ChannelOpenResponse::new();
                m2.set_status(Wifi::status::Enum::OK);
                let d: AndroidAutoFrame =
                    AndroidAutoWifiMessage::ChannelOpenResponse(channel, m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}
struct ControlChannelHandler {}

impl ChannelHandlerTrait for ControlChannelHandler {
    fn receive_data(
        &mut self,
        _channel: ChannelId,
        msg: &AndroidAutoWifiMessage,
        skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        match msg {
            AndroidAutoWifiMessage::SpecificMessage(m) => {
                todo!("{:x?}", m);
            }
            AndroidAutoWifiMessage::PingResponse(_) => {
                *skip_ping = true;
            }
            AndroidAutoWifiMessage::PingRequest(_) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenResponse(_, _) => unimplemented!(),
            AndroidAutoWifiMessage::ChannelOpenRequest(m) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::AudioFocusRequest(m) => {
                let mut m2 = Wifi::AudioFocusResponse::new();
                let s = if m.has_audio_focus_type() {
                    match m.audio_focus_type() {
                        Wifi::audio_focus_type::Enum::NONE => Wifi::audio_focus_state::Enum::NONE,
                        Wifi::audio_focus_type::Enum::GAIN => Wifi::audio_focus_state::Enum::GAIN,
                        Wifi::audio_focus_type::Enum::GAIN_TRANSIENT => {
                            Wifi::audio_focus_state::Enum::GAIN_TRANSIENT
                        }
                        Wifi::audio_focus_type::Enum::GAIN_NAVI => {
                            Wifi::audio_focus_state::Enum::GAIN
                        }
                        Wifi::audio_focus_type::Enum::RELEASE => {
                            Wifi::audio_focus_state::Enum::LOSS
                        }
                    }
                } else {
                    Wifi::audio_focus_state::Enum::NONE
                };
                log::error!("Audio focus state is {:?}", s);
                m2.set_audio_focus_state(s);
                let d: AndroidAutoFrame = AndroidAutoWifiMessage::AudioFocusResponse(m2).into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                log::info!("Sending audio focus response {:x?}", d2);
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
            AndroidAutoWifiMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
            AndroidAutoWifiMessage::ServiceDiscoveryRequest(m) => {
                log::error!("Got service discovery request: {:?}", m);
                let mut m2 = Wifi::ServiceDiscoveryResponse::new();
                m2.set_car_model(config.unit.car_model.clone());
                m2.set_can_play_native_media_during_vr(config.unit.native_media);
                m2.set_car_serial(config.unit.car_serial.clone());
                m2.set_car_year(config.unit.car_year.clone());
                m2.set_head_unit_name(config.unit.name.clone());
                m2.set_headunit_manufacturer(config.unit.head_manufacturer.clone());
                m2.set_headunit_model(config.unit.head_model.clone());
                if let Some(hide) = config.unit.hide_clock {
                    m2.set_hide_clock(hide);
                }
                m2.set_left_hand_drive_vehicle(config.unit.left_hand);
                m2.set_sw_build(config.unit.sw_build.clone());
                m2.set_sw_version(config.unit.sw_version.clone());
                for s in channels(&config) {
                    m2.channels.push(s);
                }
                let m4d = vec![
                    0x0a, 0x0f, 0x08, 0x07, 0x2a, 0x0b, 0x08, 0x01, 0x12, 0x07, 0x08, 0x80, 0x7d,
                    0x10, 0x10, 0x18, 0x01, 0x0a, 0x14, 0x08, 0x04, 0x1a, 0x10, 0x08, 0x01, 0x10,
                    0x03, 0x1a, 0x08, 0x08, 0x80, 0xf7, 0x02, 0x10, 0x10, 0x18, 0x02, 0x28, 0x01,
                    0x0a, 0x13, 0x08, 0x05, 0x1a, 0x0f, 0x08, 0x01, 0x10, 0x01, 0x1a, 0x07, 0x08,
                    0x80, 0x7d, 0x10, 0x10, 0x18, 0x01, 0x28, 0x01, 0x0a, 0x13, 0x08, 0x06, 0x1a,
                    0x0f, 0x08, 0x01, 0x10, 0x02, 0x1a, 0x07, 0x08, 0x80, 0x7d, 0x10, 0x10, 0x18,
                    0x01, 0x28, 0x01, 0x0a, 0x0c, 0x08, 0x02, 0x12, 0x08, 0x0a, 0x02, 0x08, 0x0d,
                    0x0a, 0x02, 0x08, 0x0a, 0x0a, 0x14, 0x08, 0x03, 0x1a, 0x10, 0x08, 0x03, 0x22,
                    0x0a, 0x08, 0x01, 0x10, 0x02, 0x18, 0x00, 0x20, 0x00, 0x28, 0x6f, 0x28, 0x01,
                    0x0a, 0x19, 0x08, 0x08, 0x32, 0x15, 0x0a, 0x11, 0x30, 0x30, 0x3a, 0x39, 0x33,
                    0x3a, 0x33, 0x37, 0x3a, 0x45, 0x46, 0x3a, 0x42, 0x37, 0x3a, 0x35, 0x37, 0x10,
                    0x04, 0x0a, 0x16, 0x08, 0x09, 0x42, 0x12, 0x08, 0xe8, 0x07, 0x10, 0x01, 0x1a,
                    0x0b, 0x08, 0x80, 0x02, 0x10, 0x80, 0x02, 0x18, 0x10, 0x20, 0xff, 0x01, 0x0a,
                    0x04, 0x08, 0x0a, 0x4a, 0x00, 0x0a, 0x0c, 0x08, 0x01, 0x22, 0x08, 0x12, 0x06,
                    0x08, 0x80, 0x0f, 0x10, 0xb8, 0x08, 0x12, 0x08, 0x4f, 0x70, 0x65, 0x6e, 0x41,
                    0x75, 0x74, 0x6f, 0x1a, 0x09, 0x55, 0x6e, 0x69, 0x76, 0x65, 0x72, 0x73, 0x61,
                    0x6c, 0x22, 0x04, 0x32, 0x30, 0x31, 0x38, 0x2a, 0x08, 0x32, 0x30, 0x31, 0x38,
                    0x30, 0x33, 0x30, 0x31, 0x30, 0x01, 0x3a, 0x03, 0x66, 0x31, 0x78, 0x42, 0x10,
                    0x4f, 0x70, 0x65, 0x6e, 0x41, 0x75, 0x74, 0x6f, 0x20, 0x41, 0x75, 0x74, 0x6f,
                    0x61, 0x70, 0x70, 0x4a, 0x01, 0x31, 0x52, 0x03, 0x31, 0x2e, 0x30, 0x58, 0x00,
                    0x60, 0x00,
                ];
                let m4 = Wifi::ServiceDiscoveryResponse::parse_from_bytes(&m4d);
                log::error!("Golden response is {:?}", m4);
                let m3 = AndroidAutoWifiMessage::ServiceDiscoveryResponse(m2);
                let d: AndroidAutoFrame = m3.into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
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
                if *status == 0xFFFF {
                    log::error!("Version mismatch");
                    return Err(std::io::Error::other("Version mismatch"));
                }
                log::info!("Android auto client version: {}.{}", major, minor);
                openssl_stream
                    .do_handshake()
                    .map_err(|e| e.to_string())
                    .expect("Failed to ssl connect?");
                openssl_stream.get_mut().handshake = false;
                let m = AndroidAutoWifiMessage::SslAuthComplete(true);
                let d: AndroidAutoFrame = m.into();
                let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                openssl_stream.get_mut().plain.write_all(&d2)?;
            }
        }
        Ok(())
    }
}

#[enum_dispatch::enum_dispatch(ChannelHandlerTrait)]
enum ChannelHandler {
    Control(ControlChannelHandler),
    Bluetooth(BluetoothChannelHandler),
    AvInput(AvInputChannelHandler),
    SystemAudio(SystemAudioChannelHandler),
    SpeechAudio(SpeechAudioChannelHandler),
    Sensor(SensorChannelHandler),
    Video(VideoChannelHandler),
    Navigation(NavigationChannelHandler),
    MediaStatus(MediaStatusChannelHandler),
    Input(InputChannelHandler),
    MediaAudio(MediaAudioChannelHandler),
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
                                log::error!("Unknown bluetooth packet {} {:x?}", ty, message);
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Ok::<(), String>(())
                });
            }
        }
    }

    fn handle_client(
        stream: std::net::TcpStream,
        addr: std::net::SocketAddr,
        config: AndroidAutoConfiguration,
    ) -> Result<(), String> {
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        use std::io::Write;

        let mut channel_handlers: BTreeMap<ChannelId, ChannelHandler> = BTreeMap::new();
        channel_handlers.insert(ChannelId::BLUETOOTH, BluetoothChannelHandler {}.into());
        channel_handlers.insert(ChannelId::CONTROL, ControlChannelHandler {}.into());
        channel_handlers.insert(ChannelId::AV_INPUT, AvInputChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SYSTEM_AUDIO, SystemAudioChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SPEECH_AUDIO, SpeechAudioChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SENSOR, SensorChannelHandler {}.into());
        channel_handlers.insert(ChannelId::VIDEO, VideoChannelHandler {}.into());
        channel_handlers.insert(ChannelId::NAVIGATION, NavigationChannelHandler {}.into());
        channel_handlers.insert(ChannelId::MEDIA_STATUS, MediaStatusChannelHandler {}.into());
        channel_handlers.insert(ChannelId::INPUT, InputChannelHandler {}.into());
        channel_handlers.insert(ChannelId::MEDIA_AUDIO, MediaAudioChannelHandler {}.into());
        log::debug!(
            "Got a connection on port {} from {:?}",
            config.network.port,
            addr
        );
        let openssl_socket = OpensslSocket::new(stream);
        let client_cert = openssl::x509::X509::from_pem(cert::CERTIFICATE.as_bytes())
            .expect("Failed to load client ssl certificate");
        let client_key =
            openssl::pkey::PKey::private_key_from_pem(cert::PRIVATE_KEY.as_bytes()).unwrap();
        let mut ssl_con =
            openssl::ssl::SslContext::builder(openssl::ssl::SslMethod::tls_client()).unwrap();
        ssl_con.set_certificate(&*client_cert).unwrap();
        ssl_con.set_private_key(&client_key).unwrap();
        let ssl_con = ssl_con.build();
        let mut ssl = openssl::ssl::Ssl::new(&(*ssl_con)).unwrap();
        ssl.set_connect_state();
        ssl.set_verify(SslVerifyMode::NONE);
        let mut openssl_stream = openssl::ssl::SslStream::new(ssl, openssl_socket)
            .expect("Failed to build openssl stream");
        let m = AndroidAutoWifiMessage::VersionRequest;
        let d: AndroidAutoFrame = m.into();
        let d2: Vec<u8> = d.build_vec(Some(&mut openssl_stream));
        openssl_stream
            .get_mut()
            .plain
            .write_all(&d2)
            .map_err(|e| e.to_string())?;
        loop {
            let mut skip_ping = false;
            let mut fr = FrameHeaderReceiver::new();
            let f = loop {
                match fr.read(&mut openssl_stream.get_mut().plain) {
                    Ok(Some(f)) => break Some(f),
                    Err(e) => {
                        log::error!("Error reading frame header: {} {}", e.kind(), e);
                        match e.kind() {
                            std::io::ErrorKind::NotFound => todo!(),
                            std::io::ErrorKind::PermissionDenied => todo!(),
                            std::io::ErrorKind::ConnectionRefused => todo!(),
                            std::io::ErrorKind::ConnectionReset => todo!(),
                            std::io::ErrorKind::HostUnreachable => todo!(),
                            std::io::ErrorKind::NetworkUnreachable => todo!(),
                            std::io::ErrorKind::ConnectionAborted => todo!(),
                            std::io::ErrorKind::NotConnected => todo!(),
                            std::io::ErrorKind::AddrInUse => todo!(),
                            std::io::ErrorKind::AddrNotAvailable => todo!(),
                            std::io::ErrorKind::NetworkDown => todo!(),
                            std::io::ErrorKind::BrokenPipe => todo!(),
                            std::io::ErrorKind::AlreadyExists => todo!(),
                            std::io::ErrorKind::WouldBlock => break None,
                            std::io::ErrorKind::NotADirectory => todo!(),
                            std::io::ErrorKind::IsADirectory => todo!(),
                            std::io::ErrorKind::DirectoryNotEmpty => todo!(),
                            std::io::ErrorKind::ReadOnlyFilesystem => todo!(),
                            std::io::ErrorKind::StaleNetworkFileHandle => todo!(),
                            std::io::ErrorKind::InvalidInput => todo!(),
                            std::io::ErrorKind::InvalidData => todo!(),
                            std::io::ErrorKind::TimedOut => todo!(),
                            std::io::ErrorKind::WriteZero => todo!(),
                            std::io::ErrorKind::StorageFull => todo!(),
                            std::io::ErrorKind::NotSeekable => todo!(),
                            std::io::ErrorKind::QuotaExceeded => todo!(),
                            std::io::ErrorKind::FileTooLarge => todo!(),
                            std::io::ErrorKind::ResourceBusy => todo!(),
                            std::io::ErrorKind::ExecutableFileBusy => todo!(),
                            std::io::ErrorKind::Deadlock => todo!(),
                            std::io::ErrorKind::CrossesDevices => todo!(),
                            std::io::ErrorKind::TooManyLinks => todo!(),
                            std::io::ErrorKind::ArgumentListTooLong => todo!(),
                            std::io::ErrorKind::Interrupted => todo!(),
                            std::io::ErrorKind::Unsupported => todo!(),
                            std::io::ErrorKind::UnexpectedEof => todo!(),
                            std::io::ErrorKind::OutOfMemory => todo!(),
                            std::io::ErrorKind::Other => todo!(),
                            _ => return Err("Unknown error reading frame header".to_string()),
                        }
                    }
                    _ => break None,
                }
            };
            let f2 = if let Some(f) = f {
                let mut fr2 = AndroidAutoFrameReceiver::new();
                let f2 = loop {
                    match fr2.read(&f, &mut openssl_stream) {
                        Ok(Some(f2)) => break f2,
                        Err(e) => {
                            log::error!("Error reading frame header: {} {}", e.kind(), e);
                            match e.kind() {
                                std::io::ErrorKind::NotFound => todo!(),
                                std::io::ErrorKind::PermissionDenied => todo!(),
                                std::io::ErrorKind::ConnectionRefused => todo!(),
                                std::io::ErrorKind::ConnectionReset => todo!(),
                                std::io::ErrorKind::HostUnreachable => todo!(),
                                std::io::ErrorKind::NetworkUnreachable => todo!(),
                                std::io::ErrorKind::ConnectionAborted => todo!(),
                                std::io::ErrorKind::NotConnected => todo!(),
                                std::io::ErrorKind::AddrInUse => todo!(),
                                std::io::ErrorKind::AddrNotAvailable => todo!(),
                                std::io::ErrorKind::NetworkDown => todo!(),
                                std::io::ErrorKind::BrokenPipe => todo!(),
                                std::io::ErrorKind::AlreadyExists => todo!(),
                                std::io::ErrorKind::WouldBlock => {}
                                std::io::ErrorKind::NotADirectory => todo!(),
                                std::io::ErrorKind::IsADirectory => todo!(),
                                std::io::ErrorKind::DirectoryNotEmpty => todo!(),
                                std::io::ErrorKind::ReadOnlyFilesystem => todo!(),
                                std::io::ErrorKind::StaleNetworkFileHandle => todo!(),
                                std::io::ErrorKind::InvalidInput => todo!(),
                                std::io::ErrorKind::InvalidData => todo!(),
                                std::io::ErrorKind::TimedOut => todo!(),
                                std::io::ErrorKind::WriteZero => todo!(),
                                std::io::ErrorKind::StorageFull => todo!(),
                                std::io::ErrorKind::NotSeekable => todo!(),
                                std::io::ErrorKind::QuotaExceeded => todo!(),
                                std::io::ErrorKind::FileTooLarge => todo!(),
                                std::io::ErrorKind::ResourceBusy => todo!(),
                                std::io::ErrorKind::ExecutableFileBusy => todo!(),
                                std::io::ErrorKind::Deadlock => todo!(),
                                std::io::ErrorKind::CrossesDevices => todo!(),
                                std::io::ErrorKind::TooManyLinks => todo!(),
                                std::io::ErrorKind::ArgumentListTooLong => todo!(),
                                std::io::ErrorKind::Interrupted => todo!(),
                                std::io::ErrorKind::Unsupported => todo!(),
                                std::io::ErrorKind::UnexpectedEof => todo!(),
                                std::io::ErrorKind::OutOfMemory => todo!(),
                                std::io::ErrorKind::Other => todo!(),
                                _ => return Err("Unknown error reading frame header".to_string()),
                            }
                        }
                        _ => {}
                    }
                };
                Some(f2)
            } else {
                None
            };
            if let Some(f2) = f2 {
                let thing: Result<(ChannelId, AndroidAutoWifiMessage), String> = f2.try_into();
                if let Ok((chan, m)) = thing {
                    if let Some(handler) = channel_handlers.get_mut(&chan) {
                        handler
                            .receive_data(chan, &m, &mut skip_ping, &mut openssl_stream, &config)
                            .map_err(|e| e.to_string())?;
                    } else {
                        panic!("Unknown channel id: {:?}", chan);
                    }
                } else {
                    panic!("Error parsing frame: {:?}", thing.err());
                }
            }
            if !skip_ping && !openssl_stream.get_ref().handshake {
                let mut m = Wifi::PingRequest::new();
                m.set_timestamp(42);
                let m = AndroidAutoWifiMessage::PingRequest(m);
                let d: AndroidAutoFrame = m.into();
                let d2: Vec<u8> = d.build_vec(Some(&mut openssl_stream));
                openssl_stream
                    .get_mut()
                    .plain
                    .write_all(&d2)
                    .map_err(|e| e.to_string())?;
            }
        }
        log::info!("Disconnecting normally");
        Ok(())
    }

    #[cfg(feature = "wireless")]
    pub fn wifi_listen(config: AndroidAutoConfiguration) -> Result<(), String> {
        log::debug!(
            "Listening on port {} for android auto stuff",
            config.network.port
        );
        if let Ok(a) = std::net::TcpListener::bind(format!("0.0.0.0:{}", config.network.port)) {
            loop {
                if let Ok((stream, addr)) = a.accept() {
                    let config2 = config.clone();
                    if let Err(e) = Self::handle_client(stream, addr, config2) {
                        log::error!("Disconnect from client: {:?}", e);
                    }
                }
            }
        } else {
            Err(format!(
                "Failed to listen on port {} tcp",
                config.network.port
            ))
        }
    }

    #[cfg(not(feature = "wireless"))]
    pub async fn new() -> Self {
        Self {}
    }
}
