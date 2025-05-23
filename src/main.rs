mod bluetooth;
mod settings;
mod video;

#[cfg(feature = "wifi")]
mod wifi;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::{
    egui::{self, Vec2},
    glow::PACK_COMPRESSED_BLOCK_SIZE,
};
use ringbuf::traits::{Consumer, Observer, Producer};
use uobradio_comms::PendingAudioCommand;

#[enum_dispatch::enum_dispatch]
trait SubwindowTrait {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow>;
}

struct MainPage {}

impl SubwindowTrait for MainPage {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        _common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        let r = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            let min_size = CommonWindowProperties::min_size(ui);
            ui.label(format!("Size 1: {}", ui.pixels_per_point()));
            let quit = ui.add(egui::Button::new("Quit").min_size(min_size));
            if quit.clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        r
    }
}

#[enum_dispatch::enum_dispatch(SubwindowTrait)]
enum Subwindow {
    MainPage(MainPage),
    BluetoothConfig(bluetooth::BluetoothConfig),
    Video(video::Video),
    #[cfg(feature = "wifi")]
    Wifi(wifi::Screen),
    Settings(settings::Settings),
}

impl Default for Subwindow {
    fn default() -> Self {
        Subwindow::MainPage(MainPage {})
    }
}

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "Uob Radio Gui",
        options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    )
    .unwrap();
}

struct CommonWindowProperties {
    radio: uobradio_comms::UobRadio,
    pub settings: uobradio_comms::NonvolatileSettings,
    android_auto_video_decoder: openh264::decoder::Decoder,
    android_auto_texture: Option<egui::TextureHandle>,
}

impl CommonWindowProperties {
    pub fn new() -> Self {
        Self {
            radio: uobradio_comms::UobRadio::localhost(),
            settings: uobradio_comms::NonvolatileSettings::default(),
            android_auto_video_decoder: openh264::decoder::Decoder::new().unwrap(),
            android_auto_texture: None,
        }
    }

    /// Get the minimum size for ui elements
    pub fn min_size(ui: &egui::Ui) -> egui::Vec2 {
        let m = ui.pixels_per_point();
        egui::vec2(30.0 * m, 30.0 * m)
    }
}

struct MyEguiApp {
    subwindow: Subwindow,
    check: bool,
    common: CommonWindowProperties,
    audio_output: Option<cpal::Device>,
    audio_input: Option<cpal::Device>,
    cpal_host: cpal::Host,
    media_stream: Option<(AudioProducer, cpal::Stream)>,
    sys_stream: Option<(AudioProducer, cpal::Stream)>,
    speech_stream: Option<(AudioProducer, cpal::Stream)>,
    input_stream: Option<(AudioConsumer, cpal::Stream)>,
}

type AudioProducer = ringbuf::HeapProd<i16>;
type AudioConsumer = ringbuf::HeapCons<i16>;

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let h = cpal::default_host();
        let mut ao = h.default_output_device();
        let mut ai = h.default_input_device();
        let mut media_stream = None;
        let mut sys_stream = None;
        let mut speech_stream = None;
        let mut input_stream = None;
        if let Some(ai) = &mut ai {
            if let Ok(c) = ai.supported_input_configs() {
                let mut in_config = None;
                for c in c {
                    const IN_RATE: u32 = 16000;
                    const IN_CHANNELS: u16 = 1;
                    if c.min_sample_rate().0 <= IN_RATE && c.max_sample_rate().0 >= IN_RATE {
                        if c.channels() == IN_CHANNELS {
                            if c.sample_format() == cpal::SampleFormat::I16 {
                                in_config = c.try_with_sample_rate(cpal::SampleRate(IN_RATE));
                            }
                        }
                    }
                }
                if let Some(mc) = in_config {
                    let rb = ringbuf::HeapRb::new(16000);
                    let (mut producer, consumer) = ringbuf::traits::Split::split(rb);
                    let s = ai.build_input_stream(
                        &mc.config(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            producer.push_slice(data);
                        },
                        move |err| {
                            log::error!("Error in media audio output: {:?}", err);
                        },
                        None,
                    );
                    if let Ok(s) = s {
                        input_stream = Some((consumer, s));
                    }
                }
            }
        }
        if let Some(ao) = &mut ao {
            if let Ok(c) = ao.supported_output_configs() {
                {
                    let mut media_config = None;
                    let mut sys_config = None;
                    let mut speech_config = None;
                    for c in c {
                        const MEDIA_RATE: u32 = 48000;
                        const MEDIA_CHANNELS: u16 = 2;
                        if c.min_sample_rate().0 <= MEDIA_RATE
                            && c.max_sample_rate().0 >= MEDIA_RATE
                        {
                            if c.channels() == MEDIA_CHANNELS {
                                if c.sample_format() == cpal::SampleFormat::I16 {
                                    media_config =
                                        c.try_with_sample_rate(cpal::SampleRate(MEDIA_RATE));
                                }
                            }
                        }

                        const SYS_RATE: u32 = 16000;
                        const SYS_CHANNELS: u16 = 1;
                        if c.min_sample_rate().0 <= SYS_RATE && c.max_sample_rate().0 >= SYS_RATE {
                            if c.channels() == SYS_CHANNELS {
                                if c.sample_format() == cpal::SampleFormat::I16 {
                                    sys_config = c.try_with_sample_rate(cpal::SampleRate(SYS_RATE));
                                }
                            }
                        }

                        const SPEECH_RATE: u32 = 16000;
                        const SPEECH_CHANNELS: u16 = 1;
                        if c.min_sample_rate().0 <= SPEECH_RATE
                            && c.max_sample_rate().0 >= SPEECH_RATE
                        {
                            if c.channels() == SPEECH_CHANNELS {
                                if c.sample_format() == cpal::SampleFormat::I16 {
                                    speech_config =
                                        c.try_with_sample_rate(cpal::SampleRate(SPEECH_RATE));
                                }
                            }
                        }
                    }
                    if let Some(mc) = media_config {
                        let rb = ringbuf::HeapRb::new(48000);
                        let (producer, mut consumer) = ringbuf::traits::Split::split(rb);
                        let s = ao.build_output_stream(
                            &mc.config(),
                            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                                let mut index = 0;
                                while index < data.len() {
                                    let c = ringbuf::traits::Consumer::pop_slice(
                                        &mut consumer,
                                        &mut data[index..],
                                    );
                                    if c == 0 {
                                        break;
                                    }
                                    index += c;
                                }
                            },
                            move |err| {
                                log::error!("Error in media audio output: {:?}", err);
                            },
                            None,
                        );
                        if let Ok(s) = s {
                            media_stream = Some((producer, s));
                        }
                    }
                    if let Some(mc) = sys_config {
                        let rb = ringbuf::HeapRb::new(16000);
                        let (producer, mut consumer) = ringbuf::traits::Split::split(rb);
                        let s = ao.build_output_stream(
                            &mc.config(),
                            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                                let mut index = 0;
                                while index < data.len() {
                                    let c = ringbuf::traits::Consumer::pop_slice(
                                        &mut consumer,
                                        &mut data[index..],
                                    );
                                    if c == 0 {
                                        break;
                                    }
                                    index += c;
                                }
                            },
                            move |err| {
                                log::error!("Error in media audio output: {:?}", err);
                            },
                            None,
                        );
                        if let Ok(s) = s {
                            sys_stream = Some((producer, s));
                        }
                    }
                    if let Some(mc) = speech_config {
                        let rb = ringbuf::HeapRb::new(16000);
                        let (producer, mut consumer) = ringbuf::traits::Split::split(rb);
                        let s = ao.build_output_stream(
                            &mc.config(),
                            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                                let mut index = 0;
                                while index < data.len() {
                                    let c = ringbuf::traits::Consumer::pop_slice(
                                        &mut consumer,
                                        &mut data[index..],
                                    );
                                    if c == 0 {
                                        break;
                                    }
                                    index += c;
                                }
                            },
                            move |err| {
                                log::error!("Error in media audio output: {:?}", err);
                            },
                            None,
                        );
                        if let Ok(s) = s {
                            speech_stream = Some((producer, s));
                        }
                    }
                }
            }
        }
        Self {
            subwindow: Subwindow::MainPage(MainPage {}),
            check: false,
            common: CommonWindowProperties::new(),
            audio_output: ao,
            audio_input: ai,
            cpal_host: h,
            media_stream,
            sys_stream,
            speech_stream,
            input_stream,
        }
    }
}

impl eframe::App for MyEguiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.common.radio.connect();
        if self.common.radio.ping().is_err() {
            self.common.radio.disconnect();
        }
        self.common.radio.get_cameras();
        self.common.radio.try_get_bluetooth();
        self.common.radio.try_get_android_auto();
        if let Some(ai) = &mut self.input_stream {
            if !ai.0.is_empty() {
                let len = ai.0.occupied_len();
                let mut v = vec![0; len];
                let olen = ai.0.pop_slice(&mut v);
                self.common.radio.transmit_audio(v[0..olen].to_vec());
            }
        }
        self.common.radio.process_pending_audio_commands(|c, cmd| {
            log::error!("Processing command {:?} for {:?}", cmd, c);
            match c {
                android_auto::AudioChannelType::Media => {
                    if let Some((p, s)) = &mut self.media_stream {
                        match cmd {
                            PendingAudioCommand::Start => {
                                s.play();
                            }
                            PendingAudioCommand::Stop => {
                                s.pause();
                            }
                        }
                    }
                }
                android_auto::AudioChannelType::System => {
                    if let Some((p, s)) = &mut self.sys_stream {
                        match cmd {
                            PendingAudioCommand::Start => {
                                s.play();
                            }
                            PendingAudioCommand::Stop => {
                                s.pause();
                            }
                        }
                    }
                }
                android_auto::AudioChannelType::Speech => {
                    if let Some((p, s)) = &mut self.speech_stream {
                        match cmd {
                            PendingAudioCommand::Start => {
                                s.play();
                            }
                            PendingAudioCommand::Stop => {
                                s.pause();
                            }
                        }
                    }
                }
            }
            log::error!("DONE Processing command {:?} for {:?}", cmd, c);
        });
        self.common.radio.process_received_audio(|c, data| {
            log::error!("Received {} bytes of data for {:?}", data.len(), c);
            match c {
                android_auto::AudioChannelType::Media => {
                    if let Some((p, _s)) = &mut self.media_stream {
                        p.push_slice(data);
                    }
                }
                android_auto::AudioChannelType::System => {
                    if let Some((p, _s)) = &mut self.sys_stream {
                        p.push_slice(data);
                    }
                }
                android_auto::AudioChannelType::Speech => {
                    if let Some((p, _s)) = &mut self.speech_stream {
                        p.push_slice(data);
                    }
                }
            }
        });
        if let Some(vdata) = self.common.radio.get_android_auto_video_buf() {
            let mut units = openh264::nal_units(&vdata).peekable();
            while let Some(p) = units.next() {
                match self.common.android_auto_video_decoder.decode(p) {
                    Err(e) => {
                        log::error!("Failed to decode android auto video {:?}", e);
                    }
                    Ok(Some(image)) => {
                        if units.peek().is_none() {
                            use openh264::formats::YUVSource;
                            let rgb_len = image.rgb8_len();
                            let mut rgb_raw = vec![0; rgb_len];
                            image.write_rgb8(&mut rgb_raw);
                            let (w, h) = image.dimensions_uv();
                            let ei = uobradio_comms::video::PixelData::Rgb(rgb_raw);
                            let image = egui::ColorImage {
                                size: [w * 2usize, h * 2usize],
                                pixels: ei.get_egui(),
                            };
                            if self.common.android_auto_texture.is_none() {
                                self.common.android_auto_texture = Some(ctx.load_texture(
                                    "android_auto",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            } else if let Some(t) = &mut self.common.android_auto_texture {
                                t.set_partial([0, 0], image, egui::TextureOptions::LINEAR);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Err(e) = self.common.radio.process_received(|packet| match packet {
            uobradio_comms::MessageToApp::AndroidAutoMessage(_) => {}
            uobradio_comms::MessageToApp::AndroidAutoHandlerResult(_) => {}
            uobradio_comms::MessageToApp::BluetoothMessage(_) => {}
            uobradio_comms::MessageToApp::BluetoothHandlerResult(_) => {}
            uobradio_comms::MessageToApp::CamerasBtreeMap(_) => {}
            uobradio_comms::MessageToApp::PingReply(_) => {}
            uobradio_comms::MessageToApp::CameraDataJpeg(_index, _jpeg) => {}
            uobradio_comms::MessageToApp::NewSettings(s) => {
                self.common.settings = s.clone();
            }
        }) {
            log::error!("Reconnecting to radio due to error: {:?}", e);
            self.common.radio.disconnect();
        }
        egui_extras::install_image_loaders(ctx);

        if let Some(pass) = &self.common.radio.display_passkey {
            let id: egui::ViewportId = egui::ViewportId::from_hash_of("bluetooth_show_passkey");
            let builder = egui::ViewportBuilder::default()
                .with_title("Bluetooth passkey")
                .with_always_on_top()
                .with_max_inner_size(ctx.screen_rect().size() / 2.0);
            ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label(format!("Passkey: {:06}", 1));
                    ui.label(format!("Passkey: {:06}", pass));
                });
            });
        } else if let Some(pass) = self.common.radio.confirm_passkey {
            let id: egui::ViewportId = egui::ViewportId::from_hash_of("bluetooth_show_passkey");
            let builder = egui::ViewportBuilder::default()
                .with_title("Bluetooth passkey")
                .with_always_on_top()
                .with_max_inner_size(ctx.screen_rect().size() / 2.0);
            ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let t = egui::RichText::new(format!("Passkey: {}", pass)).heading();
                        ui.label(t);
                        let min_size = CommonWindowProperties::min_size(ui);
                        if ui
                            .add(egui::Button::new("Confirm").min_size(min_size))
                            .clicked()
                        {
                            let r = bluetooth_rust::ResponseToPasskey::Yes;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            let _ = self.common.radio.send_packet(packet);
                            log::info!("Got confirm request from user for bluetooth passkey");
                        }
                        if ui
                            .add(egui::Button::new("Reject").min_size(min_size))
                            .clicked()
                        {
                            let r = bluetooth_rust::ResponseToPasskey::No;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            let _ = self.common.radio.send_packet(packet);
                            log::info!("Got reject request from user for bluetooth passkey");
                        }
                        if ui
                            .add(egui::Button::new("Cancel").min_size(min_size))
                            .clicked()
                        {
                            let r = bluetooth_rust::ResponseToPasskey::Cancel;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            let _ = self.common.radio.send_packet(packet);
                            log::info!("Got cancel request from user for bluetooth passkey");
                        }
                    })
                });
            });
        }

        if self.common.radio.android_auto_frontend() {
            egui::CentralPanel::default().show(ctx, |ui| {
                let size = ui.available_size();
                if let Some(t) = &self.common.android_auto_texture {
                    let isize = t.size()[1];
                    let zoom = isize as f32 / size.y;
                    let dsize = t.size_vec2() / zoom;
                    let p = ui.cursor();
                    let r = ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture {
                            id: t.id(),
                            size: dsize,
                        })
                        .sense(egui::Sense::drag()),
                    );
                    let o = if let Some(mut o) = r.interact_pointer_pos() {
                        o.x -= p.left();
                        o.y -= p.top();
                        o.x *= zoom;
                        o.y *= zoom;
                        Some(o)
                    } else if let Some(mut o) = r.hover_pos() {
                        o.x -= p.left();
                        o.y -= p.top();
                        o.x *= zoom;
                        o.y *= zoom;
                        Some(o)
                    } else {
                        None
                    };
                    if let Some(o) = o {
                        let mut i_event = android_auto::Wifi::InputEventIndication::new();
                        let timestamp: u64 = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_micros() as u64;
                        i_event.set_timestamp(timestamp);
                        let mut te = android_auto::Wifi::TouchEvent::new();
                        let mut tl = android_auto::Wifi::TouchLocation::new();
                        tl.set_x(o.x as u32);
                        tl.set_y(o.y as u32);
                        tl.set_pointer_id(0);
                        te.touch_location = vec![tl];
                        let mut do_touch = true;
                        if r.drag_started() {
                            te.set_touch_action(android_auto::Wifi::touch_action::Enum::PRESS);
                        } else if r.drag_stopped() {
                            te.set_touch_action(android_auto::Wifi::touch_action::Enum::RELEASE);
                        } else if r.dragged() {
                            te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                        } else if r.hovered() {
                            te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                        } else {
                            do_touch = false;
                        }
                        if do_touch {
                            i_event.touch_event = android_auto::protobuf::MessageField::some(te);
                            let e = android_auto::AndroidAutoMessage::Input(i_event);
                            let m2 = uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(
                                e.sendable(),
                            );
                            let _ = self.common.radio.send_packet(
                                uobradio_comms::MessageFromApp::AndroidAutoMessage(m2),
                            );
                        }
                    }
                }
            });
        } else {
            egui::TopBottomPanel::bottom("Bottom Icons")
                .min_height(74.0)
                .max_height(74.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(cameras) = self.common.radio.cameras() {
                            if !cameras.is_empty()
                                && ui
                                    .button(
                                        eframe::egui::RichText::new("V")
                                            .font(eframe::egui::FontId::proportional(64.0)),
                                    )
                                    .clicked()
                            {
                                self.subwindow = Subwindow::Video(video::Video::new());
                            }
                        }
                        #[cfg(feature = "wifi")]
                        {
                            if ui
                                .button(
                                    eframe::egui::RichText::new("W")
                                        .font(eframe::egui::FontId::proportional(64.0)),
                                )
                                .clicked()
                            {
                                self.subwindow = Subwindow::Wifi(wifi::Screen::new());
                            }
                        }
                        if ui
                            .button(
                                eframe::egui::RichText::new("B")
                                    .font(eframe::egui::FontId::proportional(64.0)),
                            )
                            .clicked()
                        {
                            self.subwindow =
                                Subwindow::BluetoothConfig(bluetooth::BluetoothConfig::new());
                        }
                        if ui
                            .add(
                                egui::Image::new(egui::include_image!("../refresh.png"))
                                    .maintain_aspect_ratio(true)
                                    .fit_to_exact_size(Vec2 { x: 64.0, y: 64.0 })
                                    .max_height(64.0)
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .button(
                                eframe::egui::RichText::new("S")
                                    .font(eframe::egui::FontId::proportional(64.0)),
                            )
                            .clicked()
                        {
                            self.subwindow = Subwindow::Settings(settings::Settings::new());
                        }
                        ui.label(format!("Focus: {:?}", ui.input(|r| r.viewport().focused)));
                        if self.check {
                            ui.label("LABEL");
                            self.check = false;
                        } else {
                            ui.label("POTATO");
                            self.check = true;
                        }
                    })
                });
            if let Some(sub) = self.subwindow.update(ctx, frame, &mut self.common) {
                self.subwindow = sub;
            }
        }
    }
}
