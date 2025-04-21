mod bluetooth;
mod settings;
mod video;

#[path = "../android2/src/comms.rs"]
mod comms;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

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
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
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
    video_sources: Vec<video::VideoSource>,
    app_rx: Option<std::sync::mpsc::Receiver<comms::MessageFromAppWithAddr>>,
    app_tx: BTreeMap<std::net::SocketAddr, std::sync::mpsc::Sender<comms::MessageToApp>>,
    user_rx: Option<std::sync::mpsc::Receiver<comms::MessageAboutAppUser>>,
}

impl CommonWindowProperties {
    pub fn new() -> Self {
        let mut vs = Vec::new();
        if let Ok(d) = v4l::Device::new(0) {
            vs.push(video::Video::video_start(d));
        }
        Self {
            video_sources: vs,
            app_rx: None,
            app_tx: BTreeMap::new(),
            user_rx: None,
        }
    }
}

struct MyEguiApp {
    subwindow: Subwindow,
    check: bool,
    common: CommonWindowProperties,
}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
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

        egui_extras::install_image_loaders(ctx);
        if let Some(rx) = &mut self.common.user_rx {
            if let Ok(m) = rx.try_recv() {
                println!("Received message about app user {}", m.addr);
                self.common.app_tx.insert(m.addr, m.send);
            }
        }
        if let Some(rx) = &mut self.common.app_rx {
            while let Ok(m) = rx.try_recv() {
                match m.message {
                    comms::MessageFromApp::GpioControl(g) => match g {
                        comms::Gpio::WinchControl(forwards, backwards) => {
                            println!("Winch control {} {}", forwards, backwards);
                        }
                        comms::Gpio::CameraLedControl(cam, s) => {
                            println!("Camera {} set led to {}", cam, s);
                        }
                        comms::Gpio::LockDoors => {
                            println!("Request to lock all the doors");
                        }
                        comms::Gpio::UnlockDoors => {
                            println!("Request to unlock all the doors");
                        }
                        comms::Gpio::WindowControl { id, up, down } => {
                            println!("Request to control window {} with {} {}", id, up, down);
                        }
                    },
                    comms::MessageFromApp::Ping(val) => {
                        println!("Got ping from app {}", val);
                    }
                    comms::MessageFromApp::RequestCamera(index) => {
                        if let Some(send) = self.common.app_tx.get(&m.addr) {
                            if let Some(v) = self.common.video_sources.get(index as usize) {
                                let frame = v.image.lock().unwrap();
                                let jpeg = frame.get_jpeg();
                                let _ = send.send(comms::MessageToApp::CameraDataJpeg(index, jpeg));
                            }
                        }
                    }
                }
            }
        }
        egui::TopBottomPanel::bottom("Bottom Icons")
            .min_height(74.0)
            .max_height(74.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if !self.common.video_sources.is_empty() {
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
                        .clicked
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
