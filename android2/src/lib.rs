//! This is the code for the android app that pairs with the custom electronics and software in an automotive radio.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use eframe::egui;
use eframe::{NativeOptions, Renderer};

mod bluetooth;
mod comms;

/// Represents a color pixel with rgb and alpha components
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct RgbPixel {
    colors: [u8; 4],
}

impl RgbPixel {
    /// Build from r g and b, making it fully non-transparent
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            colors: [r, g, b, 255],
        }
    }
    /// Build from a solid gray channel
    pub const fn from_gray(g: u8) -> Self {
        Self {
            colors: [g, g, g, 255],
        }
    }
}

/// A generic pixel based image
#[derive(Debug, Clone)]
pub struct PixelImage<T> {
    /// The actual pixels of the image
    pixels: Vec<T>,
    /// The width of the image in pixels.
    pub width: u16,
    /// The height of the image in pixels.
    pub height: u16,
}

impl PixelImage<RgbPixel> {
    /// Construct from raw image data of the specified dimensions
    pub fn from_raw(width: u16, height: u16, data: &[u8]) -> Self {
        let pixels: Vec<RgbPixel> = data.iter().map(|p| RgbPixel::from_gray(*p)).collect();
        Self {
            pixels,
            width,
            height,
        }
    }
    /// Build from gray zune jpeg data
    pub fn from_zune_jpeg_gray(data: Vec<u8>, ii: &zune_jpeg::ImageInfo) -> Self {
        let w = ii.width;
        let h = ii.height;
        let pixels: Vec<RgbPixel> = data.iter().map(|p| RgbPixel::from_gray(*p)).collect();
        Self {
            pixels,
            width: w,
            height: h,
        }
    }
    /// Build from color zune jpeg data
    pub fn from_zune_jpeg(data: Vec<u8>, ii: &zune_jpeg::ImageInfo) -> Self {
        let w = ii.width;
        let h = ii.height;
        let pixels: Vec<RgbPixel> = data
            .chunks_exact(3)
            .map(|p| RgbPixel::from_rgb(p[0], p[1], p[2]))
            .collect();
        Self {
            pixels,
            width: w,
            height: h,
        }
    }
    /// Build a new image of the specified dimensions
    pub fn new(w: u16, h: u16) -> Self {
        let cap = w as usize * h as usize;
        let m = vec![RgbPixel { colors: [0; 4] }; cap];
        Self {
            pixels: m,
            width: w,
            height: h,
        }
    }
}

impl From<PixelImage<RgbPixel>> for egui::ColorImage {
    fn from(value: PixelImage<RgbPixel>) -> Self {
        let pixels = value
            .pixels
            .iter()
            .map(|p| egui::Color32::from_rgb(p.colors[0], p.colors[1], p.colors[2]))
            .collect();
        Self {
            size: [value.width as usize, value.height as usize],
            pixels,
        }
    }
}

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
        Self { connect_nap: false }
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
    uob_radio_pipe: (
        std::sync::mpsc::Sender<comms::MessageToApp>,
        std::sync::mpsc::Receiver<comms::MessageToApp>,
    ),
    texture: Option<egui::TextureHandle>,
}

impl DemoApp {
    fn update_shown_image(
        &mut self,
        image: crate::PixelImage<crate::RgbPixel>,
        ctx: &egui::Context,
    ) {
        if self.texture.is_none() {
            self.texture = Some(ctx.load_texture(
                "Camera Image1",
                egui::ColorImage::from(image.clone()),
                egui::TextureOptions::NEAREST,
            ));
        } else if let Some(t) = &mut self.texture {
            if t.size()[0] != image.width as usize || t.size()[1] != image.height as usize {
                self.texture = Some(ctx.load_texture(
                    "Camera Image1",
                    egui::ColorImage::from(image.clone()),
                    egui::TextureOptions::NEAREST,
                ));
            } else {
                t.set_partial(
                    [0, 0],
                    egui::ColorImage::from(image.clone()),
                    egui::TextureOptions::NEAREST,
                );
            }
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bluetooth.enable();
        while let Ok(m) = self.uob_radio_pipe.1.try_recv() {
            match m {
                comms::MessageToApp::PingReply => {
                    log::error!("got ping packet in update method");
                }
                comms::MessageToApp::CameraDataJpeg(index, jpeg) => {
                    log::error!("Recieved data for camera {} length {}", index, jpeg.len());
                    let mut decoder = zune_jpeg::JpegDecoder::new(&jpeg);
                    if let Ok(img) = decoder.decode() {
                        let info = decoder.info().unwrap();
                        if info.components == 3 && info.pixel_density == 8 {
                            let picture =
                                crate::PixelImage::<crate::RgbPixel>::from_zune_jpeg(img, &info);
                            self.update_shown_image(picture, ctx);
                            log::error!("Got a color jpeg");
                        } else if info.components == 1 && info.pixel_density == 8 {
                            let picture = crate::PixelImage::<crate::RgbPixel>::from_zune_jpeg_gray(
                                img, &info,
                            );
                            self.update_shown_image(picture, ctx);
                            log::error!("Got a gray jpeg");
                        } else {
                            log::error!(
                                "Unexpected image properties {}x{} {} {}",
                                info.width,
                                info.height,
                                info.components,
                                info.pixel_density
                            );
                        }
                    } else {
                        log::error!("Invalid jpeg received");
                    }
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(10));
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if ui.button("Find radios").clicked() {
                    let rs = comms::UobRadio::detect_radios(self.uob_radio_pipe.0.clone());
                    if let Ok(radios) = rs {
                        log::error!("Got some radios {}", radios.len());
                        self.radios = radios;
                    } else {
                        log::error!("Failed to get any radios at all {:?}", rs);
                    }
                }
                for (address, radio) in self.radios.iter_mut() {
                    ui.label(format!("Radio at {:?}: {:?}", address, radio));
                    if ui.button("Camera enable").clicked() {
                        radio.send_camera_request(true, 0);
                    }
                }
                if let Some(t) = &self.texture {
                    let size = ui.available_size();
                    let zoom = (size.x / t.size()[0] as f32).min(size.y / t.size()[1] as f32);
                    let r = ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                        id: t.id(),
                        size: egui::Vec2 {
                            x: t.size()[0] as f32 * zoom,
                            y: t.size()[1] as f32 * zoom,
                        },
                    }));
                }
                for mut d in self.bluetooth.get_bonded_devices().unwrap() {
                    d.get_uuids_with_sdp();
                    let uuids = d.get_uuids();
                    if let Ok(uuids) = uuids {
                        if uuids.contains(&bluetooth::Uuid::NetworkingNap) {
                            let address = d.get_address().unwrap();
                            if !self.bluetooth_devs.contains_key(&address) {
                                self.bluetooth_devs
                                    .insert(address.clone(), BluetoothConfig::new());
                            }
                            if let Some(config) = self.bluetooth_devs.get_mut(&address) {
                                ui.label(format!("Config is {:?}", config));
                                if ui.button("Connect").clicked() {
                                    config.connect_nap = true;
                                }
                                if config.connect_nap {
                                    self.bluetooth.cancel_discovery();
                                    log::warn!("About to connect");
                                    let socket =
                                        d.get_rfcomm_socket(bluetooth::Uuid::NetworkingNap, true);
                                    if let Some(socket) = socket {
                                        if socket.connect().is_ok() {
                                            ui.label("Connection is ok");
                                            config.connect_nap = false;
                                        } else {
                                            ctx.request_repaint_after(
                                                std::time::Duration::from_millis(100),
                                            );
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
            uob_radio_pipe: std::sync::mpsc::channel(),
            texture: None,
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
