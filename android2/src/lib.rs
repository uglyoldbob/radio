//! This is the code for the android app that pairs with the custom electronics and software in an automotive radio.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use eframe::egui;
use eframe::{NativeOptions, Renderer};

mod bluetooth;
mod comms;

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

#[derive(Debug)]
struct BluetoothConfig {
    connect_nap: bool,
}

impl BluetoothConfig {
    fn new() -> Self {
        Self {
            connect_nap: false,
        }
    }
}

/// The main struct for holding data for the gui of the application
pub struct DemoApp {
    local_storage: Option<std::path::PathBuf>,
    settings: Result<AppConfig, AppConfigError>,
    bluetooth: bluetooth::Bluetooth,
    _java: Arc<Mutex<Java>>,
    known_uuids: BTreeMap<String, Vec<bluetooth::Uuid>>,
    bluetooth_devs: BTreeMap<String, BluetoothConfig>,
    radios: comms::UobRadios,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bluetooth.enable();
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if ui.button("Find radios").clicked() {
                    let rs = comms::UobRadio::detect_radios();
                    if let Ok(radios) = rs {
                        self.radios = radios;
                    }
                }
                for (address, radio) in self.radios.iter_mut() {
                    ui.label(format!("Radio at {:?}: {:?}", address, radio));
                }
                for mut d in self.bluetooth.get_bonded_devices().unwrap() {
                    d.get_uuids_with_sdp();
                    let uuids = d.get_uuids();
                    if let Ok(uuids) = uuids {
                        if uuids.contains(&bluetooth::Uuid::NetworkingNap) {
                            let address = d.get_address().unwrap();
                            if !self.bluetooth_devs.contains_key(&address) {
                                self.bluetooth_devs.insert(address.clone(), BluetoothConfig::new());
                            }
                            if let Some(config) = self.bluetooth_devs.get_mut(&address) {
                                ui.label(format!("Config is {:?}", config));
                                if ui.button("Connect").clicked() {
                                    config.connect_nap = true;
                                }
                                if config.connect_nap {
                                    self.bluetooth.cancel_discovery();
                                    log::warn!("About to connect");
                                    let socket = d.get_rfcomm_socket(bluetooth::Uuid::NetworkingNap, true);
                                    if let Some(socket) = socket {
                                        if socket.connect().is_ok() {
                                            ui.label("Connection is ok");
                                            config.connect_nap = false;
                                        }
                                        else {
                                            ctx.request_repaint_after(std::time::Duration::from_millis(100));
                                        }
                                    }
                                }
                            }
                            ui.label(format!(
                                "bluetooth device: {:?} {:?}",
                                d.get_name(),
                                d.get_bond_state()
                            ));
                            if let Some(uuids) = self.known_uuids.get(&address) {
                                for uuid in uuids {
                                    ui.label(format!("UUID: {:?}", uuid));
                                }
                            }
                            if ui.button("UUIDS").clicked() {
                                let uuids = d.get_uuids();
                                if let Ok(uuids) = uuids {
                                    self.known_uuids.insert(address.clone(), uuids);
                                }
                            }
                        }
                    }
                }
            });
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
            known_uuids: BTreeMap::new(),
            bluetooth_devs: BTreeMap::new(),
            radios: comms::UobRadios::new(),
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
