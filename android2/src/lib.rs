//! This is the code for the android app that pairs with the custom electronics and software in an automotive radio.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

use std::sync::{Arc, Mutex};

use eframe::egui;
use eframe::{NativeOptions, Renderer};

mod bluetooth;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
struct AppConfig {
    asdf: bool,
}

#[derive(Debug)]
enum AppConfigError {
    NotLoaded,
    Corrupt,
    UnableToCreate,
}

#[ouroboros::self_referencing]
pub struct Java {
    app: AndroidApp,
    java: jni::JavaVM,
    #[borrows(java)]
    #[not_covariant]
    env: jni::AttachGuard<'this>,
}

impl Java {
    /// Use the java environment with a closure that returns a type. Generally used to make calls to java code.
    pub fn use_env<T, F: FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> T>(
        &mut self,
        f: F,
    ) -> T {
        let context = unsafe {
            jni::objects::JObject::from_raw(
                self.borrow_app().activity_as_ptr() as *mut jni::sys::_jobject
            )
        };
        self.with_env_mut(|a| f(a, context))
    }

    /// Retrieve a clone of the androidapp object
    pub fn get_app(&self) -> AndroidApp {
        self.borrow_app().clone()
    }

    /// Make a new java object using the androidapp object
    pub fn make(app: AndroidApp) -> Self {
        let vm = unsafe {
            jni::JavaVM::from_raw(app.vm_as_ptr() as *mut *const jni::sys::JNIInvokeInterface_)
        }
        .unwrap();
        JavaBuilder {
            app,
            java: vm,
            env_builder: |java: &jni::JavaVM| java.attach_current_thread().unwrap(),
        }
        .build()
    }
}

/// The main struct for holding data for the gui of the application
pub struct DemoApp {
    local_storage: Option<std::path::PathBuf>,
    settings: Result<AppConfig, AppConfigError>,
    bluetooth: bluetooth::Bluetooth,
    _java: Arc<Mutex<Java>>,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bluetooth.enable();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Config: {:?}", self.settings));
            ui.label(format!(
                "bluetooth enabled: {}",
                self.bluetooth.is_enabled()
            ));
            for mut d in self.bluetooth.get_bonded_devices().unwrap() {
                ui.label(format!(
                    "bluetooth device: {:?} {:?}",
                    d.get_name(),
                    d.get_bond_state()
                ));
                d.get_uuids_with_sdp();
                if ui.button("Connect").clicked() {
                    let socket = d.get_rfcomm_socket(bluetooth::SPP_UUID, true);
                    if let Some(socket) = socket {
                        self.bluetooth.cancel_discovery();
                        log::warn!("About to connect");
                        let mut times = 0;
                        let a = loop {
                            times += 1;
                            let s = socket.connect();
                            if s.is_ok() {
                                break s.ok();
                            }
                            if times == 10 {
                                break None;
                            }
                        };
                        if socket.connect().is_ok() {
                            ui.label("Connection is ok");
                        }
                    }
                }
            }
        });
    }
}

impl DemoApp {
    fn load_config(&mut self) {
        if let Some(p) = &self.local_storage {
            let mut config = p.clone();
            config.push("config.bin");
            let settings = if let Ok(false) = std::fs::exists(&config) {
                let settings = AppConfig::default();
                let encoded: Vec<u8> =
                    bincode::serde::encode_to_vec(&settings, bincode::config::standard()).unwrap();
                let f = std::fs::File::create(&config);
                if let Ok(mut f) = f {
                    use std::io::Write;
                    match f.write(&encoded) {
                        Ok(_l) => Ok(settings),
                        Err(e) => {
                            log::error!("Unable to create config file: {:?}", e);
                            Err(AppConfigError::UnableToCreate)
                        }
                    }
                } else {
                    log::error!("Unable to create config file2: {:?}", f);
                    Err(AppConfigError::UnableToCreate)
                }
            } else {
                let f = std::fs::read(&config);
                if let Ok(a) = f {
                    let s = bincode::serde::decode_from_slice(&a, bincode::config::standard());
                    if let Ok((s, _len)) = s {
                        Ok(s)
                    } else {
                        Err(AppConfigError::Corrupt)
                    }
                } else {
                    Err(AppConfigError::Corrupt)
                }
            };
            self.settings = settings;
        }
    }

    fn new(_cc: &eframe::CreationContext<'_>, options: NativeOptions, app: AndroidApp) -> Self {
        let java = Java::make(app);
        let java = Arc::new(Mutex::new(java));
        let mut s = Self {
            local_storage: options.android_app.unwrap().internal_data_path(),
            settings: Err(AppConfigError::NotLoaded),
            bluetooth: bluetooth::Bluetooth::new(java.clone()),
            _java: java,
        };
        s.load_config();
        s
    }
}

fn _main(mut options: NativeOptions, app: AndroidApp) {
    options.renderer = Renderer::Wgpu;
    let o = options.clone();
    let _run = eframe::run_native(
        "UobRadio",
        options,
        Box::new(move |cc| Ok(Box::new(DemoApp::new(cc, o, app)))),
    )
    .unwrap();
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug) // limit log level
            .with_tag("uob_radio"), // logs will show under mytag tag
    );
    log::info!("UobRadio startup");
    let mut options = NativeOptions::default();
    options.viewport.fullscreen = Some(true);
    let app2 = app.clone();
    options.android_app = Some(app);
    _main(options, app2);
}
