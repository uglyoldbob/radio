use eframe::egui;

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Uob Radio Gui",
        options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    );
}

#[derive(Default)]
struct MyEguiApp {}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui_extras::install_image_loaders(ctx);
        egui::TopBottomPanel::bottom("Bottom Icons").min_height(32.0).max_height(32.0).show(ctx, |ui| {
            if ui.add(egui::Image::new(egui::include_image!("../refresh.png"))
                .rounding(5.0).sense(egui::Sense::click())).clicked {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            ui.label("bottom panel");
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
            if ui.button("quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}
