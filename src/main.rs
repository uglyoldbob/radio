mod bluetooth;
mod settings;
mod video;

use eframe::egui::{self, Vec2};

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
            ui.heading("Hello World!");
            if ui.button("quit").clicked() {
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
}

impl CommonWindowProperties {
    pub fn new() -> Self {
        Self {
            radio: uobradio_comms::UobRadio::localhost(),
        }
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
        if let Err(e) = self.common.radio.process_received(|packet| match packet {
            uobradio_comms::MessageToApp::CamerasBtreeMap(map) => {
                log::error!("Got camera btreemap: with {} items", map.len());
            }
            uobradio_comms::MessageToApp::PingReply(port) => {
                log::error!("got ping packet in update method port {}", port);
            }
            uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg) => {
                log::error!("Recieved data for camera {} length {}", index, jpeg.len());
            }
        }) {
            log::error!("Reconnecting to radio due to error: {:?}", e);
            self.common.radio.disconnect();
        }
        egui_extras::install_image_loaders(ctx);
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
