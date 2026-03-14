#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! The gui portion of the automotive radio solution.

mod hvac;
mod keyboard;
mod offroad;
mod settings;
mod video;
#[cfg(any(feature = "wifi", feature = "bluetooth"))]
mod wireless;

#[cfg(feature = "ffmpeg")]
mod ffmpeg;
#[cfg(feature = "ffmpeg")]
use ffmpeg::*;

#[cfg(feature = "androidauto")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui::{self};
#[cfg(feature = "androidauto")]
use ringbuf::traits::{Consumer, Observer, Producer};
#[cfg(feature = "androidauto")]
use uobradio_comms::PendingAudioCommand;

/// The trait that all main page elements must implement for the application
#[enum_dispatch::enum_dispatch]
trait SubwindowTrait {
    /// Show the window, return a new subwindow if the subwindow needs to change
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut GraphicsTheme,
    ) -> Option<Subwindow>;
    /// Perform and processing required for a received packet
    fn process_packet(
        &mut self,
        settings: &mut uobradio_comms::NonvolatileSettings,
        vsettings: &mut uobradio_comms::VolatileSettings,
        packet: &uobradio_comms::MessageToApp,
    );
    /// Get the icon for the left panel of the gui
    fn card(&self, active: bool, theme: &mut GraphicsTheme, ui: &mut egui::Ui) -> bool;
}

/// A trait to add functionality to egui ui object
trait ConvenienceGui {
    /// Make a button big enough for fingers to touch
    fn big_button(&mut self, theme: &GraphicsTheme, text: &str) -> egui::Response;

    /// A selectable button that is big enough for fingers
    fn selectable_button(
        &mut self,
        theme: &GraphicsTheme,
        selected: bool,
        text: &str,
    ) -> egui::Response;

    fn big_selectable_value<T: PartialEq>(
        &mut self,
        theme: &GraphicsTheme,
        val: &mut T,
        select: T,
        text: &str,
    ) -> egui::Response;
}

impl ConvenienceGui for egui::Ui {
    fn big_button(&mut self, theme: &GraphicsTheme, text: &str) -> egui::Response {
        let button = egui::Button::new(
            egui::RichText::new(text)
                .size(16.0)
                .color(theme.text_secondary),
        )
        .fill(theme.bg_secondary)
        .min_size(egui::vec2(70.0, 70.0))
        .corner_radius(12.0);
        self.add(button)
    }

    fn selectable_button(
        &mut self,
        theme: &GraphicsTheme,
        selected: bool,
        text: &str,
    ) -> egui::Response {
        let button_color = if selected {
            theme.accent_primary
        } else {
            theme.bg_secondary
        };
        let text_color = if selected {
            egui::Color32::WHITE
        } else {
            theme.text_secondary
        };

        let button = egui::Button::new(egui::RichText::new(text).size(16.0).color(text_color))
            .fill(button_color)
            .min_size(egui::vec2(70.0, 70.0))
            .corner_radius(12.0);

        self.add(button)
    }

    fn big_selectable_value<T: PartialEq>(
        &mut self,
        theme: &GraphicsTheme,
        val: &mut T,
        select: T,
        text: &str,
    ) -> egui::Response {
        let selected = *val == select;
        let button_color = if selected {
            theme.accent_primary
        } else {
            theme.bg_secondary
        };
        let text_color = if selected {
            egui::Color32::WHITE
        } else {
            theme.text_secondary
        };
        let button = egui::Button::new(egui::RichText::new(text).size(16.0).color(text_color))
            .fill(button_color)
            .selected(selected)
            .min_size(egui::vec2(70.0, 70.0))
            .corner_radius(12.0);
        let mut r = self.add(button);
        if r.clicked() {
            *val = select;
            r.mark_changed();
        }
        r
    }
}

/// The main page for the gui
#[derive(Clone, Copy)]
struct MainPage {}

struct GraphicsTheme {
    bg_primary: egui::Color32,
    bg_secondary: egui::Color32,
    bg_card: egui::Color32,
    accent_primary: egui::Color32,
    accent_warm: egui::Color32,
    text_primary: egui::Color32,
    text_secondary: egui::Color32,
}

impl GraphicsTheme {
    fn dark() -> Self {
        Self {
            bg_primary: egui::Color32::from_rgb(12, 14, 18),
            bg_secondary: egui::Color32::from_rgb(20, 24, 30),
            bg_card: egui::Color32::from_rgb(28, 32, 40),
            accent_primary: egui::Color32::from_rgb(0, 180, 255),
            accent_warm: egui::Color32::from_rgb(255, 140, 60),
            text_primary: egui::Color32::from_rgb(240, 242, 245),
            text_secondary: egui::Color32::from_rgb(160, 165, 175),
        }
    }
}

impl SubwindowTrait for MainPage {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut GraphicsTheme,
    ) -> Option<Subwindow> {
        let r = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            #[cfg(feature = "androidauto")]
            {
                if common.radio.android_auto_frontend() {
                    let size = ui.available_size();
                    if let Some(t) = &common.android_auto_texture {
                        let isize = t.size();
                        let zoom = isize[1] as f32 / size.y;
                        let zoom2 = isize[0] as f32 / size.x;
                        let zoom = zoom.max(zoom2);
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
                                .as_micros()
                                as u64;
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
                                te.set_touch_action(
                                    android_auto::Wifi::touch_action::Enum::RELEASE,
                                );
                            } else if r.dragged() {
                                te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                            } else if r.hovered() {
                                te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                            } else {
                                do_touch = false;
                            }
                            if do_touch {
                                i_event.touch_event =
                                    android_auto::protobuf::MessageField::some(te);
                                let e = android_auto::AndroidAutoMessage::Input(i_event);
                                let m2 = uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(
                                    e.sendable(),
                                );
                                let _ = common.radio.send_packet(
                                    uobradio_comms::MessageFromApp::AndroidAutoMessage(m2),
                                );
                            }
                        }
                    }
                }
            }

            if ui.big_button(&theme, "Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        r
    }

    fn card(&self, active: bool, theme: &mut GraphicsTheme, ui: &mut egui::Ui) -> bool {
        ui.selectable_button(&theme, active, &format!("{}\n{}", "H", "Home"))
            .clicked()
    }

    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _vsettings: &mut uobradio_comms::VolatileSettings,
        _packet: &uobradio_comms::MessageToApp,
    ) {
    }
}

#[derive(Clone, Copy)]
#[enum_dispatch::enum_dispatch(SubwindowTrait)]
/// The types of subwindows that can exist for the main page
enum Subwindow {
    /// The home screen or main page
    MainPage(MainPage),
    /// The video configuration page
    Video(video::Video),
    /// The settings page
    Settings(settings::Settings),
    /// The hvac control page
    Hvac(hvac::Window),
    /// The offroad control page
    Offroad(offroad::Window),
    /// The wireless control page
    #[cfg(any(feature = "wifi", feature = "bluetooth"))]
    Wireless(wireless::Config),
}

impl Default for Subwindow {
    fn default() -> Self {
        Subwindow::MainPage(MainPage {})
    }
}

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let vb = if std::option_env!("AUTO_FULLSCREEN").is_some() {
        egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_always_on_top()
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([800.0, 480.0])
            .with_resizable(false)
            .with_position([0.0, 0.0])
    };
    let options = eframe::NativeOptions {
        viewport: vb,
        ..Default::default()
    };
    eframe::run_native(
        "Uob Radio Gui",
        options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    )
    .unwrap();
}

pub enum H264Decoder {
    Openh264(openh264::decoder::Decoder),
    #[cfg(feature = "ffmpeg")]
    Ffmpeg(ffmpeg::Decoder),
}

impl H264Decoder {
    pub fn new() -> Result<Self, String> {
        #[cfg(feature = "ffmpeg")]
        {
            
        }
        Ok(Self::Openh264(openh264::decoder::Decoder::new().map_err(|e| e.to_string())?))
    }
}

/// The properties common to every window in the application
struct CommonWindowProperties {
    /// The object to communicate with the radio service
    radio: uobradio_comms::UobRadio,
    /// The non-volatile settings for the program
    pub settings: uobradio_comms::NonvolatileSettings,
    /// The volatile settings for the program
    pub vsettings: uobradio_comms::VolatileSettings,
    #[cfg(feature = "wifi")]
    /// The list of known wifi networks by ssid
    pub known_networks: uobradio_comms::Pollable<Vec<String>>,
    #[cfg(feature = "wifi")]
    /// The list of available wifi networks by ssid
    pub available_networks: uobradio_comms::Pollable<Vec<nmrs::Network>>,
    #[cfg(feature = "wifi")]
    /// The details for the current wifi network, ssid and password
    wifi_details: uobradio_comms::Pollable<(String, Option<String>)>,
    #[cfg(feature = "androidauto")]
    android_auto_video_decoder: H264Decoder,
    #[cfg(feature = "androidauto")]
    android_auto_texture: Option<egui::TextureHandle>,
    /// the onscreen keyboard
    keyboard: crate::keyboard::VirtualKeyboard,
}

impl CommonWindowProperties {
    /// Construct a new Self with default settings
    pub fn new() -> Self {
        Self {
            vsettings: uobradio_comms::VolatileSettings::default(),
            radio: uobradio_comms::UobRadio::localhost(),
            settings: uobradio_comms::NonvolatileSettings::default(),
            #[cfg(feature = "wifi")]
            available_networks: Default::default(),
            #[cfg(feature = "wifi")]
            known_networks: Default::default(),
            #[cfg(feature = "wifi")]
            wifi_details: Default::default(),
            #[cfg(feature = "androidauto")]
            android_auto_video_decoder: H264Decoder::new().expect("No h264 decoder"),
            #[cfg(feature = "androidauto")]
            android_auto_texture: None,
            keyboard: Default::default(),
        }
    }
}

/// The main struct for the application
struct MyEguiApp {
    /// the color theme
    theme: GraphicsTheme,
    /// The specific subwindow being displayed in the gui
    subwindow: Subwindow,
    /// The properties common to all windows in the application
    common: CommonWindowProperties,
    #[cfg(feature = "androidauto")]
    audio_output: Option<cpal::Device>,
    #[cfg(feature = "androidauto")]
    audio_input: Option<cpal::Device>,
    #[cfg(feature = "androidauto")]
    cpal_host: cpal::Host,
    #[cfg(feature = "androidauto")]
    media_stream: Option<(AudioProducer, cpal::Stream)>,
    #[cfg(feature = "androidauto")]
    sys_stream: Option<(AudioProducer, cpal::Stream)>,
    #[cfg(feature = "androidauto")]
    speech_stream: Option<(AudioProducer, cpal::Stream)>,
    #[cfg(feature = "androidauto")]
    input_stream: Option<(AudioConsumer, cpal::Stream)>,
}

#[cfg(feature = "androidauto")]
type AudioProducer = ringbuf::HeapProd<i16>;
#[cfg(feature = "androidauto")]
type AudioConsumer = ringbuf::HeapCons<i16>;

impl MyEguiApp {
    /// construct a new Self
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(feature = "androidauto")]
        let (ao, ai, h, media_stream, sys_stream, speech_stream, input_stream) = {
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
                            if c.min_sample_rate().0 <= SYS_RATE
                                && c.max_sample_rate().0 >= SYS_RATE
                            {
                                if c.channels() == SYS_CHANNELS {
                                    if c.sample_format() == cpal::SampleFormat::I16 {
                                        sys_config =
                                            c.try_with_sample_rate(cpal::SampleRate(SYS_RATE));
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
            (
                ao,
                ai,
                h,
                media_stream,
                sys_stream,
                speech_stream,
                input_stream,
            )
        };
        Self {
            theme: GraphicsTheme::dark(),
            subwindow: Subwindow::MainPage(MainPage {}),
            common: CommonWindowProperties::new(),
            #[cfg(feature = "androidauto")]
            audio_output: ao,
            #[cfg(feature = "androidauto")]
            audio_input: ai,
            #[cfg(feature = "androidauto")]
            cpal_host: h,
            #[cfg(feature = "androidauto")]
            media_stream,
            #[cfg(feature = "androidauto")]
            sys_stream,
            #[cfg(feature = "androidauto")]
            speech_stream,
            #[cfg(feature = "androidauto")]
            input_stream,
        }
    }
}

impl eframe::App for MyEguiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        #[cfg(feature = "test")]
        let _ = self
            .common
            .radio
            .send_packet(uobradio_comms::MessageFromApp::Exit);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.common.keyboard.bump_events(ctx, raw_input);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.common.radio.connect();
        if self.common.radio.ping().is_err() {
            self.common.radio.disconnect();
        }
        self.common.radio.get_cameras();
        #[cfg(feature = "bluetooth")]
        self.common.radio.try_get_bluetooth();
        #[cfg(feature = "androidauto")]
        self.common.radio.try_get_android_auto();
        #[cfg(feature = "androidauto")]
        if let Some(ai) = &mut self.input_stream {
            if !ai.0.is_empty() {
                let len = ai.0.occupied_len();
                let mut v = vec![0; len];
                let olen = ai.0.pop_slice(&mut v);
                self.common.radio.transmit_audio(v[0..olen].to_vec());
            }
        }
        #[cfg(feature = "androidauto")]
        self.common.radio.process_pending_audio_commands(|c, cmd| {
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
        #[cfg(feature = "androidauto")]
        self.common.radio.process_received_audio(|c, data| match c {
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
        });
        #[cfg(feature = "androidauto")]
        if let Some(vdata) = self.common.radio.get_android_auto_video_buf() {
            let mut units = openh264::nal_units(&vdata).peekable();
            while let Some(p) = units.next() {
                match self.common.android_auto_video_decoder.decode(p) {
                    Err(e) => {
                        log::error!("Failed to decode android auto video {:?}", e);
                    }
                    Ok(Some(image)) => {
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
                    _ => {}
                }
            }
        }
        let mut settings_changed = false;
        if let Err(e) = self.common.radio.process_received(|packet| {
            let mut newsettings = self.common.settings.clone();
            self.subwindow
                .process_packet(&mut newsettings, &mut self.common.vsettings, packet);
            if self.common.settings != newsettings {
                self.common.settings = newsettings;
                settings_changed = true;
            }
            match packet {
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::KnownWifiNetworks(list) => {
                    self.common
                        .known_networks
                        .new_value_optional(Some(list.to_owned()));
                }
                uobradio_comms::MessageToApp::NoUpdateInProgress => {
                    self.common.vsettings.settings.update_status_pending = false;
                }
                uobradio_comms::MessageToApp::UpdateProgress(step, percent) => {
                    self.common.vsettings.settings.update_status_pending = false;
                    self.common.vsettings.settings.download_status =
                        uobradio_comms::settings::UpdateStatus::UpdateProgress(*step, *percent);
                }
                uobradio_comms::MessageToApp::ServerFileDownloadProgress(p) => {
                    if !matches!(
                        self.common.vsettings.settings.download_status,
                        uobradio_comms::settings::UpdateStatus::Completed(_)
                    ) {
                        self.common.vsettings.settings.download_status =
                            uobradio_comms::settings::UpdateStatus::Downloading(*p);
                    }
                }
                uobradio_comms::MessageToApp::ServerFileDownloadComplete(success) => {
                    self.common.vsettings.settings.download_status =
                        uobradio_comms::settings::UpdateStatus::Completed(*success);
                }
                uobradio_comms::MessageToApp::ListOfServerUpdateFiles { files } => {
                    self.common.vsettings.settings.list = files.to_owned();
                }
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::FailedToScanForWifiNetworks { reason: _ } => {}
                uobradio_comms::MessageToApp::Ac(c) => match c {
                    uobradio_comms::AcResponse::CurrentHvacTemperature(_) => todo!(),
                    uobradio_comms::AcResponse::TemperatureSetStatus(_) => todo!(),
                    uobradio_comms::AcResponse::FanSpeedAcknowledge => todo!(),
                    uobradio_comms::AcResponse::CurrentCabinTemperature(_) => todo!(),
                },
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::FailedToConnectToWifiNetwork { ssid } => {
                    service::log::info!("Failed to connect to {ssid}");
                    self.common.wifi_details = uobradio_comms::Pollable::Idle { last_known: None };
                }
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::NoCurrentWifiNetwork => {
                    self.common.wifi_details = Default::default();
                }
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::ConnectedToWifiNetwork { ssid, password } => {
                    service::log::info!("Wifi connected2: {ssid}");
                    self.common
                        .wifi_details
                        .new_value_optional(Some((ssid.clone(), password.clone())));
                }
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::WifiList(list) => {
                    let mut list2 = list.clone();
                    list2.sort_by(|a, b| b.strength.cmp(&a.strength));
                    self.common
                        .available_networks
                        .new_value_optional(Some(list2));
                }
                #[cfg(feature = "wifi")]
                uobradio_comms::MessageToApp::WifiDetails { ssid, password } => {
                    self.common
                        .wifi_details
                        .new_value_optional(Some((ssid.clone(), password.clone())));
                }
                #[cfg(feature = "androidauto")]
                uobradio_comms::MessageToApp::AndroidAutoMessage(_) => {}
                #[cfg(feature = "androidauto")]
                uobradio_comms::MessageToApp::AndroidAutoHandlerResult(_) => {}
                #[cfg(feature = "bluetooth")]
                uobradio_comms::MessageToApp::BluetoothMessage(_) => {}
                #[cfg(feature = "bluetooth")]
                uobradio_comms::MessageToApp::BluetoothHandlerResult(_) => {}
                uobradio_comms::MessageToApp::CamerasBtreeMap(_) => {}
                uobradio_comms::MessageToApp::PingReply(_) => {}
                uobradio_comms::MessageToApp::CameraDataJpeg(_index, _jpeg) => {}
                uobradio_comms::MessageToApp::NewSettings(s) => {
                    self.common.settings = s.clone();
                }
            }
        }) {
            log::error!("Reconnecting to radio due to error: {:?}", e);
            self.common.radio.disconnect();
        }
        if settings_changed {
            let _ = self
                .common
                .radio
                .send_packet(uobradio_comms::MessageFromApp::NewSettings {
                    settings: self.common.settings.clone(),
                    #[cfg(feature = "wifi")]
                    wifi_reconnect: settings_changed,
                });
        }
        egui_extras::install_image_loaders(ctx);
        #[cfg(feature = "bluetooth")]
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
                        let t = egui::RichText::new(format!("Passkey: {:06}", pass)).heading();
                        ui.label(t);
                        if ui.big_button(&self.theme, "Confirm").clicked() {
                            let r = bluetooth_rust::ResponseToPasskey::Yes;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            let _ = self.common.radio.send_packet(packet);
                            log::info!("Got confirm request from user for bluetooth passkey");
                        }
                        if ui.big_button(&self.theme, "Reject").clicked() {
                            let r = bluetooth_rust::ResponseToPasskey::No;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            let _ = self.common.radio.send_packet(packet);
                            log::info!("Got reject request from user for bluetooth passkey");
                        }
                        if ui.big_button(&self.theme, "Cancel").clicked() {
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
        {
            egui::TopBottomPanel::top("status_bar")
                .frame(
                    egui::Frame::new()
                        .fill(self.theme.bg_primary)
                        .inner_margin(10.0),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 20.0;

                        let time = chrono::Local::now().format("%I:%M %p").to_string();
                        // Time
                        ui.label(
                            egui::RichText::new(time)
                                .size(18.0)
                                .color(self.theme.text_primary),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Temperature
                            if let Some(t) = &self.common.vsettings.hvac.current_temperature {
                                ui.label(
                                    egui::RichText::new(format!("{:.1}°F", t))
                                        .size(16.0)
                                        .color(self.theme.text_secondary),
                                );
                            }
                        });
                    });
                });
            egui::SidePanel::left("Main Icons")
                .resizable(false)
                .frame(
                    egui::Frame::side_top_panel(&ctx.style())
                        .fill(self.theme.bg_primary)
                        .inner_margin(10.0)
                        .outer_margin(0.0),
                )
                .show(ctx, |ui| {
                    {
                        let vw = Subwindow::MainPage(MainPage {});
                        if vw.card(
                            matches!(self.subwindow, Subwindow::MainPage(_)),
                            &mut self.theme,
                            ui,
                        ) {
                            self.subwindow = vw;
                        }
                    }
                    if let Some(cameras) = self.common.radio.cameras() {
                        if !cameras.is_empty() {
                            let vw = Subwindow::Video(video::Video::new());
                            if vw.card(
                                matches!(self.subwindow, Subwindow::Video(_)),
                                &mut self.theme,
                                ui,
                            ) {
                                self.subwindow = vw;
                            }
                        }
                    }
                    #[cfg(any(feature = "wifi", feature = "bluetooth"))]
                    {
                        let vw = Subwindow::Wireless(wireless::Config::new());
                        if vw.card(
                            matches!(self.subwindow, Subwindow::Wireless(_)),
                            &mut self.theme,
                            ui,
                        ) {
                            self.subwindow = vw;
                        }
                    }
                    {
                        let vw = Subwindow::Hvac(hvac::Window::new());
                        if vw.card(
                            matches!(self.subwindow, Subwindow::Hvac(_)),
                            &mut self.theme,
                            ui,
                        ) {
                            self.subwindow = vw;
                        }
                    }
                    {
                        let vw = Subwindow::Offroad(offroad::Window::new());
                        if vw.card(
                            matches!(self.subwindow, Subwindow::Offroad(_)),
                            &mut self.theme,
                            ui,
                        ) {
                            self.subwindow = vw;
                        }
                    }
                    {
                        let vw = Subwindow::Settings(settings::Settings::new());
                        if vw.card(
                            matches!(self.subwindow, Subwindow::Settings(_)),
                            &mut self.theme,
                            ui,
                        ) {
                            self.subwindow = vw;
                        }
                    }
                });

            if let Some(sub) = self
                .subwindow
                .update(ctx, frame, &mut self.common, &mut self.theme)
            {
                self.subwindow = sub;
            }
        }
    }
}
