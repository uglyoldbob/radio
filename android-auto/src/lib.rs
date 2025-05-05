use std::collections::{BTreeMap, VecDeque};

use openssl::ssl::SslVerifyMode;

mod cert;

use Wifi::ChannelDescriptor;
use protobuf::{EnumOrUnknown, Message};

mod control;
use control::*;
mod nonspecific;
mod common;
use common::*;

pub trait AndroidAutoMainTrait {
    #[inline(always)]
    fn supports_video(&mut self) -> Option<&mut dyn AndroidAutoVideoChannelTrait> { None }
}

pub trait AndroidAutoVideoChannelTrait : AndroidAutoMainTrait {
    fn receive_video(&mut self, data: &[u8]);
}

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

#[derive(Debug, PartialEq)]
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
    rx_sofar: Vec<u8>,
}

impl AndroidAutoFrameReceiver {
    fn new() -> Self {
        Self {
            len: None,
            data: Vec::new(),
            rx_sofar: Vec::new(),
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
        if let Some(len) = self.len.take() {
            stream
                .get_mut()
                .plain
                .read_exact(&mut self.data[0..len as usize])?;
            let data = if header.frame.get_frame_type() == FrameHeaderType::Single {
                let d = self.data.clone();
                self.data.clear();
                Some(d)
            } else {
                self.rx_sofar.append(&mut self.data);
                if header.frame.get_frame_type() == FrameHeaderType::Last {
                    let d = self.rx_sofar.clone();
                    self.rx_sofar.clear();
                    Some(d)
                } else {
                    None
                }
            };
            if let Some(data) = data {
                let data = if header.frame.get_encryption() {
                    stream.get_mut().relay_data(&data);
                    let mut data = vec![0; AndroidAutoFrame::MAX_FRAME_DATA_SIZE];
                    let newlen = stream.ssl_read(&mut data).map_err(|e| {
                        let e2 = e.to_string();
                        std::io::Error::new(std::io::ErrorKind::Other, e2)
                    })?;
                    data[0..newlen].to_vec()
                } else {
                    data.clone()
                };
                let f = AndroidAutoFrame {
                    header: header.clone(),
                    data,
                };
                let f = Some(f);
                return Ok(f);
            }
        }
        Ok(None)
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

    fn receive_frame(&mut self) -> Result<(ChannelId, AndroidAutoControlMessage), String> {
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
        let r: Result<AndroidAutoControlMessage, String> = (&f2).try_into();
        match r {
            Ok(m) => Ok((f.channel_id, m)),
            Err(e) => Err(e),
        }
    }
}

impl std::io::Read for OpensslSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.recvd.len() < buf.len() {
            match self.receive_frame() {
                Ok((chan, m)) => match m {
                    AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                    AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                    AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                    AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                    AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                    AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                    AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                    AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                    AndroidAutoControlMessage::VersionResponse {
                        major: _,
                        minor: _,
                        status: _,
                    } => unimplemented!(),
                    AndroidAutoControlMessage::SslHandshake(items) => {
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
            let m = AndroidAutoControlMessage::SslHandshake(buf.to_vec());
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

#[enum_dispatch::enum_dispatch]
trait ChannelHandlerTrait {
    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error>;

    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor>;

    fn set_channels(&mut self, chans: Vec<ChannelDescriptor>) {}
}

enum InputMessage {
    Control(AndroidAutoControlMessage),
    BindingRequest(ChannelId, Wifi::BindingRequest),
    BindingResponse(ChannelId, Wifi::BindingResponse),
}

impl Into<AndroidAutoFrame> for InputMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            Self::Control(c) => c.into(),
            Self::BindingRequest(_, _) => unimplemented!(),
            Self::BindingResponse(chan, m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::input_channel_message::Enum::BINDING_RESPONSE as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: chan,
                        frame: FrameHeaderContents::new(true, FrameHeaderType::Single, false),
                    },
                    data: m,
                }
            }
        }
    }
}

impl TryFrom<&AndroidAutoFrame> for InputMessage {
    type Error = String;
    fn try_from(value: &AndroidAutoFrame) -> Result<Self, Self::Error> {
        use protobuf::Enum;
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if let Some(sys) = Wifi::input_channel_message::Enum::from_i32(ty as i32) {
            match sys {
                Wifi::input_channel_message::Enum::BINDING_REQUEST => {
                    let m = Wifi::BindingRequest::parse_from_bytes(&value.data[2..]);
                    match m {
                        Ok(m) => Ok(Self::BindingRequest(value.header.channel_id, m)),
                        Err(e) => Err(format!("Invalid input bind request: {}", e.to_string())),
                    }
                }
                Wifi::input_channel_message::Enum::BINDING_RESPONSE => unimplemented!(),
                Wifi::input_channel_message::Enum::INPUT_EVENT_INDICATION => todo!(),
                Wifi::input_channel_message::Enum::NONE => todo!(),
            }
        } else if Wifi::ControlMessage::from_i32(ty as i32).is_some() {
            let w: Result<AndroidAutoControlMessage, String> = value.try_into();
            w.map(|v| Self::Control(v))
        } else {
            Err(format!("Not converted message: {:x?}", value.data))
        }
    }
}

struct InputChannelHandler {}

impl ChannelHandlerTrait for InputChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
        let mut ichan = Wifi::InputChannel::new();
        let mut tc = Wifi::TouchConfig::new();
        tc.set_height(480);
        tc.set_width(800);
        ichan.touch_screen_config.0.replace(Box::new(tc));
        chan.input_channel.0.replace(Box::new(ichan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<InputMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                InputMessage::BindingRequest(chan, m) => {
                    let mut m2 = Wifi::BindingResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame = InputMessage::BindingResponse(chan, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                InputMessage::BindingResponse(_, _) => unimplemented!(),
                InputMessage::Control(m) => todo!(),
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for input: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!();
    }
}

struct MediaAudioChannelHandler {}

impl ChannelHandlerTrait for MediaAudioChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for media audio: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AvChannelMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AvChannelMessage::Control(m) => unimplemented!(),
                AvChannelMessage::MediaIndication(_, _, _) => {
                    log::error!("Received media data for media audio");
                }
                AvChannelMessage::SetupRequest(chan, m) => {
                    log::info!("Got channel setup request for {:?} audio: {:?}", chan, m);
                    let mut m2 = Wifi::AVChannelSetupResponse::new();
                    m2.set_max_unacked(10);
                    m2.set_media_status(Wifi::avchannel_setup_status::Enum::OK);
                    m2.configs.push(0);
                    let d: AndroidAutoFrame = AvChannelMessage::SetupResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::SetupResponse(chan, m) => unimplemented!(),
                AvChannelMessage::VideoFocusRequest(chan, m) => {
                    let mut m2 = Wifi::VideoFocusIndication::new();
                    m2.set_focus_mode(Wifi::video_focus_mode::Enum::FOCUSED);
                    m2.set_unrequested(false);
                    let d: AndroidAutoFrame =
                        AvChannelMessage::VideoIndicationResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::VideoIndicationResponse(_, _) => unimplemented!(),
                AvChannelMessage::StartIndication(_, _) => {}
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

#[derive(Debug)]
enum MediaStatusMessage {
    Playback(ChannelId, Wifi::MediaInfoChannelPlaybackData),
    Metadata(ChannelId, Wifi::MediaInfoChannelMetadataData),
    Invalid,
}

impl Into<AndroidAutoFrame> for MediaStatusMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            Self::Playback(_, _) => todo!(),
            Self::Metadata(_, _) => todo!(),
            Self::Invalid => unimplemented!(),
        }
    }
}

impl TryFrom<&AndroidAutoFrame> for MediaStatusMessage {
    type Error = String;
    fn try_from(value: &AndroidAutoFrame) -> Result<Self, Self::Error> {
        use protobuf::Enum;
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if let Some(sys) = Wifi::media_info_channel_message::Enum::from_i32(ty as i32) {
            match sys {
                Wifi::media_info_channel_message::Enum::PLAYBACK => {
                    let m = Wifi::MediaInfoChannelPlaybackData::parse_from_bytes(&value.data);
                    match m {
                        Ok(m) => Ok(Self::Playback(value.header.channel_id, m)),
                        Err(e) => Ok(Self::Invalid),
                    }
                }
                Wifi::media_info_channel_message::Enum::METADATA => {
                    let m = Wifi::MediaInfoChannelMetadataData::parse_from_bytes(&value.data);
                    match m {
                        Ok(m) => Ok(Self::Metadata(value.header.channel_id, m)),
                        Err(e) => Ok(Self::Invalid),
                    }
                }
                Wifi::media_info_channel_message::Enum::NONE => todo!(),
            }
        } else {
            Err(format!("Not converted message: {:x?}", value.data))
        }
    }
}

struct MediaStatusChannelHandler {}

impl ChannelHandlerTrait for MediaStatusChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
        let mchan = Wifi::MediaInfoChannel::new();
        chan.media_infoChannel.0.replace(Box::new(mchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<MediaStatusMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                MediaStatusMessage::Metadata(_, m) => {
                    log::info!("Metadata {:?}", m);
                }
                MediaStatusMessage::Playback(_, m) => {
                    log::info!("Playback {:?}", m);
                }
                MediaStatusMessage::Invalid => {
                    log::error!("Received invalid media info frame");
                }
            }
            return Ok(());
        }
        let msg3: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg3 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for media status: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg4: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg4 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!("{:?} {:?} {:?}", msg2, msg3, msg4);
    }
}

struct NavigationChannelHandler {}

impl ChannelHandlerTrait for NavigationChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
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
        chan.set_channel_id(chanid as u8 as u32);
        chan.navigation_channel.0.replace(Box::new(navchan));
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for navigation: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

struct VideoChannelHandler {}

impl ChannelHandlerTrait for VideoChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        let mut avchan = Wifi::AVChannel::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for video: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AvChannelMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AvChannelMessage::Control(m) => unimplemented!(),
                AvChannelMessage::MediaIndication(chan, time, data) => {
                    if let Some(a) = main.supports_video() {
                        a.receive_video(&data);
                    }
                }
                AvChannelMessage::SetupRequest(chan, m) => {
                    log::info!("Got channel setup request for channel {:?}: {:?}", chan, m);
                    let mut m2 = Wifi::AVChannelSetupResponse::new();
                    m2.set_max_unacked(10);
                    m2.set_media_status(Wifi::avchannel_setup_status::Enum::OK);
                    m2.configs.push(0);
                    let d: AndroidAutoFrame = AvChannelMessage::SetupResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::SetupResponse(chan, m) => unimplemented!(),
                AvChannelMessage::VideoFocusRequest(chan, m) => {
                    let mut m2 = Wifi::VideoFocusIndication::new();
                    m2.set_focus_mode(Wifi::video_focus_mode::Enum::FOCUSED);
                    m2.set_unrequested(false);
                    let d: AndroidAutoFrame =
                        AvChannelMessage::VideoIndicationResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::VideoIndicationResponse(_, _) => unimplemented!(),
                AvChannelMessage::StartIndication(_, _) => {}
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

struct SensorChannelHandler {}

impl ChannelHandlerTrait for SensorChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
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
        chan.set_channel_id(chanid as u8 as u32);
        if !chan.is_initialized() {
            panic!("Channel not initialized?");
        }
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for sensor: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

struct SpeechAudioChannelHandler {}

impl ChannelHandlerTrait for SpeechAudioChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for speech audio: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AvChannelMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AvChannelMessage::Control(m) => unimplemented!(),
                AvChannelMessage::MediaIndication(_, _, _) => {
                    log::error!("Received media data for speech audio");
                }
                AvChannelMessage::SetupRequest(chan, m) => {
                    log::info!("Got channel setup request for {:?} audio: {:?}", chan, m);
                    let mut m2 = Wifi::AVChannelSetupResponse::new();
                    m2.set_max_unacked(10);
                    m2.set_media_status(Wifi::avchannel_setup_status::Enum::OK);
                    m2.configs.push(0);
                    let d: AndroidAutoFrame = AvChannelMessage::SetupResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::SetupResponse(chan, m) => unimplemented!(),
                AvChannelMessage::VideoFocusRequest(chan, m) => {
                    let mut m2 = Wifi::VideoFocusIndication::new();
                    m2.set_focus_mode(Wifi::video_focus_mode::Enum::FOCUSED);
                    m2.set_unrequested(false);
                    let d: AndroidAutoFrame =
                        AvChannelMessage::VideoIndicationResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::VideoIndicationResponse(_, _) => unimplemented!(),
                AvChannelMessage::StartIndication(_, _) => {}
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

enum AvChannelMessage {
    Control(AndroidAutoControlMessage),
    SetupRequest(ChannelId, Wifi::AVChannelSetupRequest),
    SetupResponse(ChannelId, Wifi::AVChannelSetupResponse),
    VideoFocusRequest(ChannelId, Wifi::VideoFocusRequest),
    VideoIndicationResponse(ChannelId, Wifi::VideoFocusIndication),
    StartIndication(ChannelId, Wifi::AVChannelStartIndication),
    MediaIndication(ChannelId, Option<u64>, Vec<u8>),
}

impl Into<AndroidAutoFrame> for AvChannelMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            Self::Control(c) => c.into(),
            Self::SetupRequest(_, _) => unimplemented!(),
            Self::SetupResponse(chan, m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::avchannel_message::Enum::SETUP_RESPONSE as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: chan,
                        frame: FrameHeaderContents::new(true, FrameHeaderType::Single, false),
                    },
                    data: m,
                }
            }
            Self::MediaIndication(_, _, _) => unimplemented!(),
            Self::VideoFocusRequest(chan, m) => unimplemented!(),
            Self::VideoIndicationResponse(chan, m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::avchannel_message::Enum::VIDEO_FOCUS_INDICATION as u16;
                let t = t.to_be_bytes();
                let mut m = Vec::new();
                m.push(t[0]);
                m.push(t[1]);
                m.append(&mut data);
                AndroidAutoFrame {
                    header: FrameHeader {
                        channel_id: chan,
                        frame: FrameHeaderContents::new(true, FrameHeaderType::Single, false),
                    },
                    data: m,
                }
            }
            Self::StartIndication(_, _) => unimplemented!(),
        }
    }
}

impl TryFrom<&AndroidAutoFrame> for AvChannelMessage {
    type Error = String;
    fn try_from(value: &AndroidAutoFrame) -> Result<Self, Self::Error> {
        use protobuf::Enum;
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if let Some(sys) = Wifi::avchannel_message::Enum::from_i32(ty as i32) {
            match sys {
                Wifi::avchannel_message::Enum::AV_MEDIA_WITH_TIMESTAMP_INDICATION => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&value.data[2..10]);
                    let ts: u64 = u64::from_be_bytes(b);
                    Ok(Self::MediaIndication(
                        value.header.channel_id,
                        Some(ts),
                        value.data[10..].to_vec(),
                    ))
                }
                Wifi::avchannel_message::Enum::AV_MEDIA_INDICATION => Ok(Self::MediaIndication(
                    value.header.channel_id,
                    None,
                    value.data[2..].to_vec(),
                )),
                Wifi::avchannel_message::Enum::SETUP_REQUEST => {
                    let m = Wifi::AVChannelSetupRequest::parse_from_bytes(&value.data[2..]);
                    match m {
                        Ok(m) => Ok(Self::SetupRequest(value.header.channel_id, m)),
                        Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                    }
                }
                Wifi::avchannel_message::Enum::START_INDICATION => {
                    let m = Wifi::AVChannelStartIndication::parse_from_bytes(&value.data[2..]);
                    match m {
                        Ok(m) => Ok(Self::StartIndication(value.header.channel_id, m)),
                        Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                    }
                }
                Wifi::avchannel_message::Enum::STOP_INDICATION => todo!(),
                Wifi::avchannel_message::Enum::SETUP_RESPONSE => unimplemented!(),
                Wifi::avchannel_message::Enum::AV_MEDIA_ACK_INDICATION => todo!(),
                Wifi::avchannel_message::Enum::AV_INPUT_OPEN_REQUEST => todo!(),
                Wifi::avchannel_message::Enum::AV_INPUT_OPEN_RESPONSE => todo!(),
                Wifi::avchannel_message::Enum::VIDEO_FOCUS_REQUEST => {
                    let m = Wifi::VideoFocusRequest::parse_from_bytes(&value.data[2..]);
                    match m {
                        Ok(m) => Ok(Self::VideoFocusRequest(value.header.channel_id, m)),
                        Err(e) => Err(format!("Invalid channel open request: {}", e.to_string())),
                    }
                }
                Wifi::avchannel_message::Enum::VIDEO_FOCUS_INDICATION => unimplemented!(),
            }
        } else if Wifi::ControlMessage::from_i32(ty as i32).is_some() {
            let w: Result<AndroidAutoControlMessage, String> = value.try_into();
            w.map(|v| Self::Control(v))
        } else {
            Err(format!("Not converted message: {:x?}", value.data))
        }
    }
}

struct SystemAudioChannelHandler {}

impl ChannelHandlerTrait for SystemAudioChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for system audio: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AvChannelMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AvChannelMessage::Control(m) => unimplemented!(),
                AvChannelMessage::MediaIndication(_, _, _) => {
                    log::error!("Received media data for system audio");
                }
                AvChannelMessage::SetupRequest(chan, m) => {
                    log::info!("Got channel setup request for {:?} audio: {:?}", chan, m);
                    let mut m2 = Wifi::AVChannelSetupResponse::new();
                    m2.set_max_unacked(10);
                    m2.set_media_status(Wifi::avchannel_setup_status::Enum::OK);
                    m2.configs.push(0);
                    let d: AndroidAutoFrame = AvChannelMessage::SetupResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::SetupResponse(chan, m) => unimplemented!(),
                AvChannelMessage::VideoFocusRequest(chan, m) => {
                    let mut m2 = Wifi::VideoFocusIndication::new();
                    m2.set_focus_mode(Wifi::video_focus_mode::Enum::FOCUSED);
                    m2.set_unrequested(false);
                    let d: AndroidAutoFrame =
                        AvChannelMessage::VideoIndicationResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AvChannelMessage::VideoIndicationResponse(_, _) => unimplemented!(),
                AvChannelMessage::StartIndication(_, _) => {}
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

struct AvInputChannelHandler {}

impl ChannelHandlerTrait for AvInputChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for av input: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}

struct BluetoothChannelHandler {}

impl ChannelHandlerTrait for BluetoothChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let mut chan = ChannelDescriptor::new();
        chan.set_channel_id(chanid as u8 as u32);
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
        Some(chan)
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        _skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        _config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let channel = msg.header.channel_id;
        let msg2: Result<AndroidAutoCommonMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoCommonMessage::ChannelOpenResponse(_, _) => unimplemented!(),
                AndroidAutoCommonMessage::ChannelOpenRequest(m) => {
                    log::info!("Got channel open request for bluetooth: {:?}", m);
                    let mut m2 = Wifi::ChannelOpenResponse::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        AndroidAutoCommonMessage::ChannelOpenResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::PingRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(_) => unimplemented!(),
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major: _,
                    minor: _,
                    status: _,
                } => unimplemented!(),
            }
            return Ok(());
        }
        todo!("{:x?}", msg);
    }
}
struct ControlChannelHandler {
    channels: Vec<ChannelDescriptor>,
}

impl ChannelHandlerTrait for ControlChannelHandler {
    fn set_channels(&mut self, chans: Vec<ChannelDescriptor>) {
        self.channels = chans;
    }

    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<ChannelDescriptor> {
        None
    }

    fn receive_data<T: AndroidAutoMainTrait>(
        &mut self,
        msg: AndroidAutoFrame,
        skip_ping: &mut bool,
        openssl_stream: &mut openssl::ssl::SslStream<OpensslSocket>,
        config: &AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), std::io::Error> {
        use std::io::Write;
        let msg2: Result<AndroidAutoControlMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                AndroidAutoControlMessage::PingResponse(_) => {
                    *skip_ping = true;
                }
                AndroidAutoControlMessage::PingRequest(a) => {
                    let mut m = Wifi::PingResponse::new();
                    m.set_timestamp(a.timestamp());
                    let m = AndroidAutoControlMessage::PingResponse(m);
                    let d: AndroidAutoFrame = m.into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AndroidAutoControlMessage::AudioFocusResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::AudioFocusRequest(m) => {
                    let mut m2 = Wifi::AudioFocusResponse::new();
                    let s = if m.has_audio_focus_type() {
                        match m.audio_focus_type() {
                            Wifi::audio_focus_type::Enum::NONE => {
                                Wifi::audio_focus_state::Enum::NONE
                            }
                            Wifi::audio_focus_type::Enum::GAIN => {
                                Wifi::audio_focus_state::Enum::GAIN
                            }
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
                    m2.set_audio_focus_state(s);
                    let d: AndroidAutoFrame =
                        AndroidAutoControlMessage::AudioFocusResponse(m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AndroidAutoControlMessage::ServiceDiscoveryResponse(_) => unimplemented!(),
                AndroidAutoControlMessage::ServiceDiscoveryRequest(m) => {
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
                    for s in &self.channels {
                        m2.channels.push(s.clone());
                    }
                    let m3 = AndroidAutoControlMessage::ServiceDiscoveryResponse(m2);
                    let d: AndroidAutoFrame = m3.into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
                AndroidAutoControlMessage::SslAuthComplete(_) => unimplemented!(),
                AndroidAutoControlMessage::SslHandshake(data) => {
                    log::info!("SSL Handshake data is {:x?}", data);
                    todo!();
                }
                AndroidAutoControlMessage::VersionRequest => unimplemented!(),
                AndroidAutoControlMessage::VersionResponse {
                    major,
                    minor,
                    status,
                } => {
                    if status == 0xFFFF {
                        log::error!("Version mismatch");
                        return Err(std::io::Error::other("Version mismatch"));
                    }
                    log::info!("Android auto client version: {}.{}", major, minor);
                    openssl_stream
                        .do_handshake()
                        .map_err(|e| e.to_string())
                        .expect("Failed to ssl connect?");
                    openssl_stream.get_mut().handshake = false;
                    let m = AndroidAutoControlMessage::SslAuthComplete(true);
                    let d: AndroidAutoFrame = m.into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
        } else {
            todo!("{:?} {:x?}", msg2.err(), msg);
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

    fn handle_client<T: AndroidAutoMainTrait>(
        stream: std::net::TcpStream,
        addr: std::net::SocketAddr,
        config: AndroidAutoConfiguration,
        main: &mut T,
    ) -> Result<(), String> {
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        use std::io::Write;

        let mut channel_handlers: BTreeMap<ChannelId, ChannelHandler> = BTreeMap::new();
        //channel_handlers.insert(ChannelId::BLUETOOTH, BluetoothChannelHandler {}.into());
        channel_handlers.insert(
            ChannelId::CONTROL,
            ControlChannelHandler {
                channels: Vec::new(),
            }
            .into(),
        );
        channel_handlers.insert(ChannelId::AV_INPUT, AvInputChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SYSTEM_AUDIO, SystemAudioChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SPEECH_AUDIO, SpeechAudioChannelHandler {}.into());
        channel_handlers.insert(ChannelId::SENSOR, SensorChannelHandler {}.into());
        if main.supports_video().is_some() {
            channel_handlers.insert(ChannelId::VIDEO, VideoChannelHandler {}.into());
        }
        channel_handlers.insert(ChannelId::NAVIGATION, NavigationChannelHandler {}.into());
        channel_handlers.insert(ChannelId::MEDIA_STATUS, MediaStatusChannelHandler {}.into());
        channel_handlers.insert(ChannelId::INPUT, InputChannelHandler {}.into());
        channel_handlers.insert(ChannelId::MEDIA_AUDIO, MediaAudioChannelHandler {}.into());
        let mut chans = Vec::new();
        for (chanid, handler) in channel_handlers.iter() {
            if let Some(chan) = handler.build_channel(&config, *chanid) {
                chans.push(chan);
            }
        }
        channel_handlers
            .get_mut(&ChannelId::CONTROL)
            .unwrap()
            .set_channels(chans);
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
        let m = AndroidAutoControlMessage::VersionRequest;
        let d: AndroidAutoFrame = m.into();
        let d2: Vec<u8> = d.build_vec(Some(&mut openssl_stream));
        openssl_stream
            .get_mut()
            .plain
            .write_all(&d2)
            .map_err(|e| e.to_string())?;
        let mut fr2 = AndroidAutoFrameReceiver::new();
        loop {
            let mut skip_ping = false;
            let mut fr = FrameHeaderReceiver::new();
            let f = loop {
                match fr.read(&mut openssl_stream.get_mut().plain) {
                    Ok(Some(f)) => break Some(f),
                    Err(e) => {
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
                let f2 = loop {
                    match fr2.read(&f, &mut openssl_stream) {
                        Ok(Some(f2)) => break Some(f2),
                        Ok(None) => {
                            skip_ping = true;
                            break None;
                        }
                        Err(e) => {
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
                f2
            } else {
                None
            };
            if let Some(f2) = f2 {
                if let Some(handler) = channel_handlers.get_mut(&f2.header.channel_id) {
                    handler
                        .receive_data(f2, &mut skip_ping, &mut openssl_stream, &config, main)
                        .map_err(|e| e.to_string())?;
                } else {
                    panic!("Unknown channel id: {:?}", f2.header.channel_id);
                }
            }
            if !skip_ping && !openssl_stream.get_ref().handshake {
                let mut m = Wifi::PingRequest::new();
                m.set_timestamp(42);
                let m = AndroidAutoControlMessage::PingRequest(m);
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
    pub fn wifi_listen<T: AndroidAutoMainTrait>(config: AndroidAutoConfiguration, mut main: T) -> Result<(), String> {
        log::debug!(
            "Listening on port {} for android auto stuff",
            config.network.port
        );
        if let Ok(a) = std::net::TcpListener::bind(format!("0.0.0.0:{}", config.network.port)) {
            loop {
                if let Ok((stream, addr)) = a.accept() {
                    let config2 = config.clone();
                    if let Err(e) = Self::handle_client(stream, addr, config2, &mut main) {
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
