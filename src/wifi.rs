//! Code for the wifi configuration on the radio

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

/// The stage of connecting to a wifi network
enum WifiConnectStage {
    /// Prompt the user for the password
    PasswordPrompt,
    /// Indicate connecting to the network
    Connecting,
    /// The wifi is connected
    Connected,
}

pub struct Screen {
    /// For the qr code
    texture: Option<egui::TextureHandle>,
    /// For connecting to a wifi with new credentials
    wifi_new_connect: Option<usize>,
    /// The password storage for wifi connection
    wifi_password: String,
    /// Connection state for the indicated wifi network (wifi_new_connect)
    wifi_state: WifiConnectStage,
}

impl Screen {
    pub fn new() -> Self {
        Self {
            texture: None,
            wifi_new_connect: None,
            wifi_password: String::new(),
            wifi_state: WifiConnectStage::PasswordPrompt,
        }
    }

    fn make_wifi_qr(&self, wifi_name: &String, wifi_password: &String) -> Vec<u8> {
        let a = format!("WIFI:S:{};T:WPA;P:{};H:false;;", wifi_name, wifi_password);
        a.as_bytes().to_vec()
    }

    fn update_qr_code(&mut self, ctx: &egui::Context, common: &CommonWindowProperties) {
        if let uobradio_comms::WifiConfig::Disabled = common.settings.wifi_config {
            self.texture.take();
        } else if let Some((wn, wp)) = &common.wifi_details {
            let contents = self.make_wifi_qr(wn, wp);
            let code = qrcode::QrCode::new(contents).unwrap();
            let image = code.render::<image::Rgb<u8>>().build();
            let img: uobradio_comms::video::PixelImage<uobradio_comms::video::RgbPixel> =
                image.into();
            let cimg: egui::ColorImage = img.into();
            self.texture = Some(ctx.load_texture("qrcode", cimg, egui::TextureOptions::LINEAR));
        }
    }
}

impl SubwindowTrait for Screen {

    fn process_packet(&mut self, settings: &mut uobradio_comms::NonvolatileSettings, packet: &uobradio_comms::MessageToApp) {
        if let uobradio_comms::MessageToApp::ConnectedToWifiNetwork { ssid: _, password: _, } = packet {
            log::info!("Processing that the wifi is connected now");
            self.wifi_state = WifiConnectStage::Connected;
        }
    }

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        let _ = common
            .radio
            .send_packet(uobradio_comms::MessageFromApp::RequestSettings);
        self.update_qr_code(ctx, common);
        egui::SidePanel::right("Hotspot qr code view").show(ctx, |ui| {
            let size = ui.available_size();
            if let Some(t) = &self.texture {
                let isize = t.size()[1];
                let zoom = isize as f32 / size.y;
                let dsize = t.size_vec2() / zoom;
                ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                    id: t.id(),
                    size: dsize,
                }));
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("This is the wifi page".to_string());
            let mut save = false;
            let mut reconnect = false;
            ui.label("Wifi mode");
            {
                if ui
                    .add(egui::SelectableLabel::new(
                        common.settings.wifi_config == uobradio_comms::WifiConfig::Disabled,
                        "Disabled",
                    ))
                    .clicked()
                {
                    save = true;
                    reconnect = true;
                    common.settings.wifi_config = uobradio_comms::WifiConfig::Disabled;
                }
                if ui
                    .add(egui::SelectableLabel::new(
                        common.settings.wifi_config == uobradio_comms::WifiConfig::Hotspot,
                        "Hotspot",
                    ))
                    .clicked()
                {
                    save = true;
                    reconnect = true;
                    common.settings.wifi_config = uobradio_comms::WifiConfig::Hotspot;
                }
                if ui
                    .add(egui::SelectableLabel::new(
                        common.settings.wifi_config == uobradio_comms::WifiConfig::Ready,
                        "Regular network",
                    ))
                    .clicked()
                {
                    save = true;
                    reconnect = true;
                    common.settings.wifi_config = uobradio_comms::WifiConfig::Ready;
                }
                if let uobradio_comms::WifiConfig::RegularNetwork(ssid, _) = &common.settings.wifi_config {
                    ui.label(format!("Connected to {}", ssid));
                }
            }
            let mut scan = || {
                if ui.button("Scan for wifi networks").clicked() {
                    let _ = common
                        .radio
                        .send_packet(uobradio_comms::MessageFromApp::ScanForWifiNetworks);
                }
                for (i, w) in common.wifi_list.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.wifi_new_connect,
                        Some(i),
                        format!("{} - {}", w.name.clone(), w.get_speed()),
                    );
                    if Some(i) == self.wifi_new_connect {
                        match self.wifi_state {
                            WifiConnectStage::PasswordPrompt => {
                                ui.label("Password");
                                let te = egui::widgets::TextEdit::singleline(&mut self.wifi_password).password(true);
                                ui.add(te);
                                if ui.button("Connect").clicked() {
                                    let asdf = common.radio.send_packet(
                                        uobradio_comms::MessageFromApp::ConnectToNetwork(
                                            w.name.clone(),
                                            Some(self.wifi_password.clone()),
                                        ),
                                    );
                                    log::info!("State of connect message: {:?}", asdf);
                                    self.wifi_state = WifiConnectStage::Connecting;
                                }
                            }
                            WifiConnectStage::Connecting => {
                                ui.label("Connecting to network...");
                            }
                            WifiConnectStage::Connected => {
                                ui.label("Connected");
                            }
                        }
                    }
                }
            };
            match &common.settings.wifi_config {
                uobradio_comms::WifiConfig::RegularNetwork(_ssid, _pw) => {
                    scan();
                }
                uobradio_comms::WifiConfig::Ready => {
                    scan();
                }
                _ => {}
            }
            let mut hotspot = common.settings.hotspot_enabled.is_some();
            if ui.checkbox(&mut hotspot, "Configure hotspot").changed() {
                if hotspot {
                    common.settings.hotspot_enabled =
                        Some(("UobRadio Hotspot".to_string(), "qwertyuiop".to_string()));
                } else {
                    common.settings.hotspot_enabled = None;
                }
                reconnect = true;
                save = true;
            }
            if let Some(hs) = &mut common.settings.hotspot_enabled {
                ui.label("Hotspot name");
                if ui.text_edit_singleline(&mut hs.0).changed() {
                    reconnect = true;
                    save = true;
                }
                ui.label("Hotspot password");
                if ui.text_edit_singleline(&mut hs.1).changed() {
                    reconnect = true;
                    save = true;
                }
            }
            if save {
                log::info!("Sending new settings: {:?}", common.settings);
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::NewSettings { settings: common.settings.clone(), wifi_reconnect: reconnect, });
            }
        });
        None
    }
}
