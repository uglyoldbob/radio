use futures::SinkExt;
use tokio::io::AsyncReadExt;

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

bitfield::bitfield!{
    pub struct FrameHeaderType(u8);
    impl Debug;
    impl new;
    u8;
    get_encryption, set_encryption: 3;
    /// First = 1,
    /// Middle = 0,
    /// Last = 2,
    /// Single = 3,
    get_frame_type, set_frame_type: 2, 0;
    get_control, set_control: 4;
}

/// Represents the header of a frame sent to the android auto client
#[derive(Debug)]
struct FrameHeader {
    channel_id: ChannelId,
    frame: FrameHeaderType,
}

impl Clone for FrameHeader {
    fn clone(&self) -> Self {
        let mut a = FrameHeaderType::new(false, 0, false);
        a.0 = self.frame.0;
        Self {
            channel_id: self.channel_id.clone(),
            frame: a,
        }
    }
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
        Self {
            channel_id: None,
        }
    }
    pub async fn read(&mut self, stream: &mut tokio::net::TcpStream) -> Result<Option<FrameHeader>, String> {
        if self.channel_id.is_none() {
            let mut b = [0u8];
            stream.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            self.channel_id = ChannelId::try_from(b[0]).ok();
        }
        if let Some(channel_id) = &self.channel_id {
            let mut b = [0u8];
            stream.read_exact(&mut b).await.map_err(|e| e.to_string())?;
            let mut a = FrameHeaderType::new(false, 0, false);
            a.0 = b[0];
            let fh = FrameHeader {
                channel_id: *channel_id,
                frame: a,
            };
            return Ok(Some(fh))
        }
        Ok(None)
    }
}

#[derive(Debug)]
struct AndroidAutoFrame {
    header: FrameHeader, 
    data: Vec<u8>,
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

    async fn read(&mut self, header: &FrameHeader, stream: &mut tokio::net::TcpStream) -> Result<Option<AndroidAutoFrame>, String> {
        if self.len.is_none() {
            let mut p = [0u8; 2];
            stream.read_exact(&mut p).await.map_err(|e| e.to_string())?;
            let len = u16::from_be_bytes(p);
            self.data = vec![0; len as usize];
            self.len.replace(len);
        }
        if let Some(len) = &self.len {
            stream.read_exact(&mut self.data[0..*len as usize]).await.map_err(|e| e.to_string())?;
            let f = AndroidAutoFrame { header: header.clone(), data: self.data.clone(), };
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
    VersionResponse,
}

#[cfg(feature = "wireless")]
impl TryFrom<AndroidAutoFrame> for AndroidAutoWifiMessage {
    type Error = String;
    fn try_from(value: AndroidAutoFrame) -> Result<Self, Self::Error> {
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if ty == Wifi::ControlMessageType::MESSAGE_VERSION_RESPONSE as u16 {
            Ok(AndroidAutoWifiMessage::VersionResponse)
        }
        else {
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
                let t = Wifi::ControlMessageType::MESSAGE_VERSION_REQUEST as u16;
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
                        frame: FrameHeaderType::new(false, 3, true),
                    },
                    data: m,
                }
            }
            AndroidAutoWifiMessage::VersionResponse => {
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

    #[cfg(feature = "wireless")]
    pub async fn wifi_listen(network: NetworkInformation) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;

        log::info!("Listening on port {} for android auto stuff", network.port);
        if let Ok(a) = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", network.port)).await {
            loop {
                if let Ok((mut stream, addr)) = a.accept().await {
                    tokio::task::spawn(async move {
                        log::debug!("Got a connection on port {} from {:?}", network.port, addr);
                        let m = AndroidAutoWifiMessage::VersionRequest;
                        let d: AndroidAutoFrame = m.into();
                        let d2: Vec<u8> = d.into();
                        let a = stream.write_all(&d2).await;
                        log::debug!("Sent packet {:x?} to wifi user: {:?}", d2, a);
                        let mut fr = FrameHeaderReceiver::new();
                        let f = loop {
                            if let Ok(Some(f)) = fr.read(&mut stream).await {
                                break f;
                            }
                        };
                        log::debug!("Received frame header {:x?}", f);
                        let mut fr2 = AndroidAutoFrameReceiver::new();
                        let f2 = loop {
                            if let Ok(Some(f2)) = fr2.read(&f, &mut stream).await {
                                break f2;
                            }
                        };
                        log::info!("Received a full frame {:x?}", f2);
                        let message : Result<AndroidAutoWifiMessage, String> = f2.try_into();
                        log::info!("Message is {:?}", message);
                        log::info!("Disconnecting");
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
