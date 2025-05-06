mod bluetooth;
mod settings;
mod video;

#[cfg(feature = "wifi")]
mod wifi;

use eframe::{
    egui::{self, Vec2},
    glow::PACK_IMAGE_HEIGHT,
};

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
}

impl CommonWindowProperties {
    pub fn new() -> Self {
        Self {
            radio: uobradio_comms::UobRadio::localhost(),
            settings: uobradio_comms::NonvolatileSettings::default(),
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
}

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            subwindow: Subwindow::MainPage(MainPage {}),
            check: false,
            common: CommonWindowProperties::new(),
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
        if let Some(vdata) = self.common.radio.get_android_video_buf() {
            log::error!("Got some video data length {}", vdata.len());
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
                    ui.label(&format!("Passkey: {:06}", 1));
                    ui.label(&format!("Passkey: {:06}", pass));
                });
            });
        } else if let Some(pass) = self.common.radio.confirm_passkey.clone() {
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
                            self.common.radio.send_packet(packet);
                            log::info!("Got confirm request from user for bluetooth passkey");
                        }
                        if ui
                            .add(egui::Button::new("Reject").min_size(min_size))
                            .clicked()
                        {
                            let r = bluetooth_rust::ResponseToPasskey::No;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            self.common.radio.send_packet(packet);
                            log::info!("Got reject request from user for bluetooth passkey");
                        }
                        if ui
                            .add(egui::Button::new("Cancel").min_size(min_size))
                            .clicked()
                        {
                            let r = bluetooth_rust::ResponseToPasskey::Cancel;
                            let m = bluetooth_rust::MessageFromBluetoothHost::PasskeyMessage(r);
                            let packet = uobradio_comms::MessageFromApp::BluetoothMessage(m);
                            self.common.radio.send_packet(packet);
                            log::info!("Got cancel request from user for bluetooth passkey");
                        }
                    })
                });
            });
        }

        egui::TopBottomPanel::bottom("Bottom Icons")
            .min_height(74.0)
            .max_height(74.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(cameras) = self.common.radio.cameras() {
                        if !cameras.is_empty() {
                            if ui
                                .button(
                                    eframe::egui::RichText::new("V")
                                        .font(eframe::egui::FontId::proportional(64.0)),
                                )
                                .clicked()
                            {
                                self.subwindow = Subwindow::Video(video::Video::new());
                            }
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
