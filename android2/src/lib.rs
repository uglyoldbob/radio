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
    pub fn use_env<T, F: FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> T>(&mut self, f: F) -> T {
        let context = unsafe {
            jni::objects::JObject::from_raw(
                self.borrow_app().activity_as_ptr() as *mut jni::sys::_jobject
            )
        };
        self.with_env_mut(|a| {
            f(a, context)
        })
    }
}

pub struct DemoApp {
    local_storage: Option<std::path::PathBuf>,
    settings: Result<AppConfig, AppConfigError>,
    bluetooth: bluetooth::Bluetooth,
    java: Java,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bluetooth.do_test(&mut self.java);
        self.bluetooth.enable(&mut self.java);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Config: {:?}", self.settings));
            ui.label(format!("bluetooth enabled: {}", self.bluetooth.isEnabled(&mut self.java)));
            for d in self.bluetooth.getBondedDevices(&mut self.java).unwrap() {
                ui.label(format!("bluetooth device: {:?}", d.getName(&mut self.java)));
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
                    if let Ok((s, len)) = s {
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

    fn new(
        cc: &eframe::CreationContext<'_>,
        options: NativeOptions,
        java: jni::JavaVM,
        app: AndroidApp,
    ) -> Self {
        let java = JavaBuilder {
            app,
            java,
            env_builder: |java| java.attach_current_thread().unwrap(),
        }
        .build();
        let mut s = Self {
            local_storage: options.android_app.unwrap().internal_data_path(),
            settings: Err(AppConfigError::NotLoaded),
            bluetooth: bluetooth::Bluetooth::new(),
            java,
        };
        s.load_config();
        s
    }
}

fn _main(mut options: NativeOptions, java: jni::JavaVM, app: AndroidApp) {
    options.renderer = Renderer::Wgpu;
    let o = options.clone();
    let run = eframe::run_native(
        "UobRadio",
        options,
        Box::new(move |cc| Ok(Box::new(DemoApp::new(cc, o, java, app)))),
    )
    .unwrap();
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    log::error!("UobRadio startup");
    let mut vm = unsafe {
        jni::JavaVM::from_raw(app.vm_as_ptr() as *mut *const jni::sys::JNIInvokeInterface_)
    }
    .unwrap();
    let context = unsafe {
        jni::objects::JObject::from_raw(app.activity_as_ptr() as *mut jni::sys::_jobject)
    };
    let mut options = NativeOptions::default();
    options.viewport.fullscreen = Some(true);
    let app2 = app.clone();
    options.android_app = Some(app);
    _main(options, vm, app2);
}
