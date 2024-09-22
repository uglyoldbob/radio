mod bluetooth;
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

enum MessageFromAsync {
    NewBluetoothDevice(bluer::Address),
    OldBluetoothDevice(bluer::Address),
    BluetoothDeviceProperty(bluer::Address, bluer::DeviceProperty),
    BluetoothPresent(bool),
}

enum MessageToAsync {
    BluetoothScan(bool),
    Quit,
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
}

impl Default for Subwindow {
    fn default() -> Self {
        Subwindow::MainPage(MainPage {})
    }
}

fn main() {
    let (tx, rx) = tokio::sync::mpsc::channel(20);
    let (tx2, rx2) = tokio::sync::mpsc::channel(20);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_always_on_top(),
        ..Default::default()
    };
    let threaded_rt = tokio::runtime::Runtime::new().unwrap();
    threaded_rt.spawn(async {
        async_main(tx, rx2).await;
    });
    eframe::run_native(
        "Uob Radio Gui",
        options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc, rx, tx2)))),
    ).unwrap();
}

struct CommonWindowProperties {
    bluetooth: bluetooth::BluetoothData,
    rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
    tx: tokio::sync::mpsc::Sender<MessageToAsync>,
}

impl CommonWindowProperties {
    pub fn new(rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
        tx: tokio::sync::mpsc::Sender<MessageToAsync>,) -> Self {
        Self {
            bluetooth: bluetooth::BluetoothData::new(),
            rx,
            tx,
        }
    }
}

struct MyEguiApp {
    subwindow: Subwindow,
    check: bool,
    common: CommonWindowProperties,
}

async fn async_main(
    tx: tokio::sync::mpsc::Sender<MessageFromAsync>,
    mut rx: tokio::sync::mpsc::Receiver<MessageToAsync>,
) {
    bluetooth::bluetooth(tx, &mut rx).await;
}

impl MyEguiApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
        tx: tokio::sync::mpsc::Sender<MessageToAsync>,
    ) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self {
            subwindow: Subwindow::MainPage(MainPage {}),
            check: false,
            common: CommonWindowProperties::new(rx, tx),
        }
    }
}

impl eframe::App for MyEguiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.common.tx.blocking_send(MessageToAsync::Quit);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();

        egui_extras::install_image_loaders(ctx);
        while let Ok(m) = self.common.rx.try_recv() {
            match m {
                MessageFromAsync::NewBluetoothDevice(addr) => {
                    self.common.bluetooth.devices.insert(addr, bluetooth::BluetoothDeviceInfo::new());
                }
                MessageFromAsync::OldBluetoothDevice(addr) => {
                    //self.common.bluetooth_devices.remove_entry(&addr);
                }
                MessageFromAsync::BluetoothDeviceProperty(addr, prop) => {
                    println!("Received bluetooth device property: {:?}: {:?}", addr, prop);
                    if let Some(d) = self.common.bluetooth.devices.get_mut(&addr) {
                        d.update(prop);
                    }
                }
                MessageFromAsync::BluetoothPresent(p) => {
                    println!("Bluetooth presence: {}", p);
                }
            }
        }
        egui::TopBottomPanel::bottom("Bottom Icons")
            .min_height(74.0)
            .max_height(74.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(eframe::egui::RichText::new("V").font(eframe::egui::FontId::proportional(64.0))).clicked() {
                        self.subwindow = Subwindow::Video(video::Video::new());
                    }
                    if ui.button(eframe::egui::RichText::new("B").font(eframe::egui::FontId::proportional(64.0))).clicked() {
                        self.subwindow = Subwindow::BluetoothConfig(bluetooth::BluetoothConfig::new());
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
