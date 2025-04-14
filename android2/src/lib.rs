use eframe::egui;
use eframe::{NativeOptions, Renderer};

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[derive(Default)]
struct DemoApp {
    demo_windows: egui_demo_lib::DemoWindows,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("I am groot");
        });
    }
}

impl DemoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        log::error!("I am GROOT");
        Self::default()
    }
}

fn _main(mut options: NativeOptions) {
    log::error!("I am groot");
    options.renderer = Renderer::Wgpu;
    log::error!("I am groot 2");
    let run = eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
    );
    log::error!("I am NOT groot {:?}", run);
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    std::env::set_var("RUST_BACKTRACE", "1");
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Trace));

    let mut options = NativeOptions::default();
    options.viewport.fullscreen = Some(true);
    options.android_app = Some(app);
    _main(options);
}

#[cfg(not(target_os = "android"))]
fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn) // Default Log Level
        .parse_default_env()
        .init();

    _main(NativeOptions::default());
}