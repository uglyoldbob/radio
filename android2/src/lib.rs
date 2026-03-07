//! This is the code for the android app that pairs with the custom electronics and software in an automotive radio.

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use eframe::egui;
use eframe::{NativeOptions, Renderer};

use bluetooth_rust::Java;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

use bluetooth_rust::SyncBluetoothAdapterTrait;

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
pub struct UobRadioMainWindow {
    local_storage: Option<std::path::PathBuf>,
    settings: Result<AppConfig, AppConfigError>,
    nvsettings: uobradio_comms::NonvolatileSettings,
    _java: Arc<Mutex<Java>>,
    bluetooth: bluetooth_rust::BluetoothAdapter,
    known_uuids: BTreeMap<String, Vec<bluetooth_rust::BluetoothUuid>>,
    bluetooth_devs: BTreeMap<String, BluetoothConfig>,
    radios: uobradio_comms::UobRadios,
    texture: Option<egui::TextureHandle>,
}

impl UobRadioMainWindow {
    fn update_shown_image(
        texture: &mut Option<egui::TextureHandle>,
        image: uobradio_comms::video::PixelImage<uobradio_comms::video::RgbPixel>,
        ctx: &egui::Context,
    ) {
        if texture.is_none() {
            let eimg: egui::ColorImage = image.into();
            *texture = Some(ctx.load_texture("Camera Image1", eimg, egui::TextureOptions::NEAREST));
        } else if let Some(t) = texture {
            if t.size()[0] != image.width as usize || t.size()[1] != image.height as usize {
                *texture = Some(ctx.load_texture(
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

    /// Get the minimum size for ui elements
    pub fn min_size(ui: &egui::Ui) -> egui::Vec2 {
        let m = ui.pixels_per_point();
        egui::vec2(10.0 * m, 10.0 * m)
    }

    /// Get the font size
    pub fn font_size() -> f32 {
        24.0
    }
}

impl eframe::App for UobRadioMainWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        //self.bluetooth.enable();
        for (address, radio) in self.radios.iter_mut() {
            if radio
                .process_received(|packet| match packet {
                    uobradio_comms::MessageToApp::Ac(_) => {}
                    uobradio_comms::MessageToApp::ListOfServerUpdateFiles { files } => {}
                    uobradio_comms::MessageToApp::ServerFileDownloadComplete(_) => {}
                    uobradio_comms::MessageToApp::ServerFileDownloadProgress(_) => {}
                    uobradio_comms::MessageToApp::UpdateProgress(_, _) => {}
                    uobradio_comms::MessageToApp::NoUpdateInProgress => {}
                    uobradio_comms::MessageToApp::NewSettings(s) => {
                        self.nvsettings = s.clone();
                    }
                    uobradio_comms::MessageToApp::CamerasBtreeMap(map) => {
                        log::error!("Got camera map with {} items", map.len());
                    }
                    uobradio_comms::MessageToApp::PingReply(port) => {
                        log::error!("got ping packet port {}", port);
                    }
                    uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg) => {
                        log::error!("Recieved data for camera {} length {}", index, jpeg.len());
                        if let Some(img) = uobradio_comms::video::PixelImage::<
                            uobradio_comms::video::RgbPixel,
                        >::from_jpeg_image(&jpeg)
                        {
                            Self::update_shown_image(&mut self.texture, img, ctx);
                        } else {
                            log::error!("Invalid jpeg received");
                        }
                    }
                })
                .is_err()
            {
                log::error!("Reconnecting to radio due to error");
                radio.disconnect();
                radio.connect();
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(10));
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Size 1: {}", ui.pixels_per_point()))
                    .size(Self::font_size()),
            );
            let min_size = Self::min_size(ui);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let find = ui.add(
                    egui::Button::new(egui::RichText::new("Find radios").size(Self::font_size()))
                        .min_size(min_size),
                );
                if find.clicked() {
                    let rs = uobradio_comms::UobRadio::detect_radios(5);
                    if let Ok(radios) = rs {
                        log::error!("Got some radios {}", radios.len());
                        self.radios = radios;
                    } else {
                        log::error!("Failed to get any radios at all");
                    }
                }
                for (address, radio) in self.radios.iter_mut() {
                    radio.connect();
                    radio.send_camera_request(0);
                    ui.label(format!("Radio at {:?}", address.ip()));
                    ui.horizontal(|ui| {
                        let winch_response = ui.add(
                            egui::Button::new("Winch IN")
                                .min_size(min_size)
                                .sense(egui::Sense::drag()),
                        );
                        if winch_response.drag_started() {
                            radio.send_gpio(uobradio_comms::Gpio::WinchControl(true, false));
                        } else if winch_response.drag_released() {
                            radio.send_gpio(uobradio_comms::Gpio::WinchControl(false, false));
                        }
                        let winch_response = ui.add(
                            egui::Button::new("Winch OUT")
                                .min_size(min_size)
                                .sense(egui::Sense::drag()),
                        );
                        if winch_response.drag_started() {
                            radio.send_gpio(uobradio_comms::Gpio::WinchControl(false, true));
                        } else if winch_response.drag_released() {
                            radio.send_gpio(uobradio_comms::Gpio::WinchControl(false, false));
                        }
                    });
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
                use bluetooth_rust::BluetoothAdapterTrait;
                if let Some(b) = self.bluetooth.supports_sync() {
                    use bluetooth_rust::BluetoothDeviceTrait;
                    for mut d in b.get_paired_devices().unwrap() {
                        d.run_sdp();
                        let uuids = d.get_uuids();
                        if let Ok(uuids) = uuids {
                            if true {
                                let address = d.get_address().unwrap();
                                if !self.bluetooth_devs.contains_key(&address) {
                                    self.bluetooth_devs
                                        .insert(address.clone(), BluetoothConfig::new());
                                }
                                if let Some(config) = self.bluetooth_devs.get_mut(&address) {
                                    ui.label(format!("Config is {:?}", config));
                                    if ui
                                        .add(egui::Button::new("Connect").min_size(min_size))
                                        .clicked()
                                    {
                                        config.connect_nap = true;
                                    }
                                    if config.connect_nap {
                                        log::warn!("About to connect");
                                        let socket = d.get_rfcomm_socket(
                                            bluetooth_rust::BluetoothUuid::NetworkingNap,
                                            true,
                                        );
                                        if let Ok(mut socket) = socket {
                                            use bluetooth_rust::BluetoothSocketTrait;
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
                                ui.label(format!("bluetooth device: {:?}", d.get_name()));
                                if let Some(uuids) = self.known_uuids.get(&address) {
                                    for uuid in uuids {
                                        ui.label(format!("UUID: {:?}", uuid));
                                    }
                                }
                                if ui
                                    .add(egui::Button::new("UUIDS").min_size(min_size))
                                    .clicked()
                                {
                                    let uuids = d.get_uuids();
                                    if let Ok(uuids) = uuids {
                                        self.known_uuids.insert(address.clone(), uuids);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        });
    }
}

impl UobRadioMainWindow {
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
        let java = Java::make(app.clone());
        let mut bab = bluetooth_rust::BluetoothAdapterBuilder::new();
        bab.with_android_app(app);
        let mut s = Self {
            local_storage: options.android_app.unwrap().internal_data_path(),
            settings: Err(AppConfigError::NotLoaded),
            nvsettings: uobradio_comms::NonvolatileSettings::default(),
            _java: Arc::new(Mutex::new(java)),
            bluetooth: bab.build().expect("Failed to get bluetooth adapter"),
            known_uuids: BTreeMap::new(),
            bluetooth_devs: BTreeMap::new(),
            radios: uobradio_comms::UobRadios::new(),
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
        Box::new(move |cc| Ok(Box::new(UobRadioMainWindow::new(cc, o, app)))),
    )
    .unwrap();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("uob_radio"),
    );
    log::info!("UobRadio startup");
    let mut options = NativeOptions::default();
    options.viewport.fullscreen = Some(true);
    let app2 = app.clone();
    options.android_app = Some(app);
    _main(options, app2);
}
