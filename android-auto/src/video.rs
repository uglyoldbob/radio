use super::{
    AndroidAutoCommonMessage, AndroidAutoConfiguration, AndroidAutoFrame, AndroidAutoMainTrait, AvChannelMessage,
    ChannelHandlerTrait,
    ChannelId, FrameHeader, FrameHeaderContents, FrameHeaderType, OpensslSocket,
};
use crate::Wifi;
use protobuf::{Enum, Message};

pub struct VideoChannelHandler {}

impl ChannelHandlerTrait for VideoChannelHandler {
    fn build_channel(
        &self,
        config: &AndroidAutoConfiguration,
        chanid: ChannelId,
    ) -> Option<Wifi::ChannelDescriptor> {
        let mut chan = Wifi::ChannelDescriptor::new();
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
