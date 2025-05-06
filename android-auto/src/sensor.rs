use super::{
    AndroidAutoCommonMessage, AndroidAutoConfiguration, AndroidAutoControlMessage,
    AndroidAutoFrame, AndroidAutoMainTrait, AvChannelMessage, ChannelDescriptor,
    ChannelHandlerTrait, ChannelId, FrameHeader, FrameHeaderContents, FrameHeaderType,
    OpensslSocket,
};
use crate::Wifi::{self, DrivingStatus};
use protobuf::{Enum, EnumOrUnknown, Message};

#[derive(Debug)]
pub enum SensorMessage {
    SensorStartRequest(ChannelId, Wifi::SensorStartRequestMessage),
    SensorStartResponse(ChannelId, Wifi::SensorStartResponseMessage),
    Event(ChannelId, Wifi::SensorEventIndication),
}

impl Into<AndroidAutoFrame> for SensorMessage {
    fn into(self) -> AndroidAutoFrame {
        match self {
            Self::SensorStartRequest(_, _) => todo!(),
            Self::SensorStartResponse(chan, m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::sensor_channel_message::Enum::SENSOR_START_RESPONSE as u16;
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
            Self::Event(chan, m) => {
                let mut data = m.write_to_bytes().unwrap();
                let t = Wifi::sensor_channel_message::Enum::SENSOR_EVENT_INDICATION as u16;
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
        }
    }
}

impl TryFrom<&AndroidAutoFrame> for SensorMessage {
    type Error = String;
    fn try_from(value: &AndroidAutoFrame) -> Result<Self, Self::Error> {
        use protobuf::Enum;
        let mut ty = [0u8; 2];
        ty.copy_from_slice(&value.data[0..2]);
        let ty = u16::from_be_bytes(ty);
        if let Some(sys) = Wifi::sensor_channel_message::Enum::from_i32(ty as i32) {
            match sys {
                Wifi::sensor_channel_message::Enum::SENSOR_START_REQUEST => {
                    let m = Wifi::SensorStartRequestMessage::parse_from_bytes(&value.data[2..]);
                    match m {
                        Ok(m) => Ok(Self::SensorStartRequest(value.header.channel_id, m)),
                        Err(e) => Err(e.to_string()),
                    }
                }
                Wifi::sensor_channel_message::Enum::SENSOR_START_RESPONSE => unimplemented!(),
                Wifi::sensor_channel_message::Enum::SENSOR_EVENT_INDICATION => unimplemented!(),
                Wifi::sensor_channel_message::Enum::NONE => unimplemented!(),
            }
        } else {
            Err(format!("Not converted message: {:x?}", value.data))
        }
    }
}

pub struct SensorChannelHandler {}

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
            sensor1.set_type(Wifi::sensor_type::Enum::DRIVING_STATUS);
            sensor1
        });
        sensors.push({
            let mut sensor1 = Wifi::Sensor::new();
            sensor1.set_type(Wifi::sensor_type::Enum::NIGHT_DATA);
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
        let msg2: Result<SensorMessage, String> = (&msg).try_into();
        if let Ok(msg2) = msg2 {
            match msg2 {
                SensorMessage::Event(chan, m) => unimplemented!(),
                SensorMessage::SensorStartResponse(_, _) => unimplemented!(),
                SensorMessage::SensorStartRequest(chan, m) => {
                    log::error!("Sensor start request {:?}", m);
                    let mut m3 = Wifi::SensorEventIndication::new();
                    let mut ds = Wifi::DrivingStatus::new();
                    ds.set_status(Wifi::DrivingStatusEnum::UNRESTRICTED as i32);
                    m3.driving_status.push(ds);
                    let d: AndroidAutoFrame = SensorMessage::Event(channel, m3).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;

                    let mut m2 = Wifi::SensorStartResponseMessage::new();
                    m2.set_status(Wifi::status::Enum::OK);
                    let d: AndroidAutoFrame =
                        SensorMessage::SensorStartResponse(channel, m2).into();
                    let d2: Vec<u8> = d.build_vec(Some(openssl_stream));
                    openssl_stream.get_mut().plain.write_all(&d2)?;
                }
            }
            return Ok(());
        }
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
