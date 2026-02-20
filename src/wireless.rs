use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

#[cfg(feature = "wifi")]
use uobradio_comms::WifiConnectStage;

#[derive(Clone, Copy)]
pub struct Config {}

impl Config {
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(feature = "wifi")]
    fn make_wifi_qr(&self, wifi_name: &String, wifi_password: &String) -> Vec<u8> {
        let a = format!("WIFI:S:{};T:WPA;P:{};H:false;;", wifi_name, wifi_password);
        a.as_bytes().to_vec()
    }

    #[cfg(feature = "wifi")]
    fn update_qr_code(&mut self, ctx: &egui::Context, common: &mut CommonWindowProperties) {
        if let uobradio_comms::WifiConfig::Disabled = common.settings.wifi_config {
            common.vsettings.wifi_texture.take();
        } else if let Some((wn, wp)) = &common.wifi_details {
            if let Some(wp) = wp {
                let contents = self.make_wifi_qr(wn, wp);
                let code = qrcode::QrCode::new(contents).unwrap();
                let image = code.render::<image::Rgb<u8>>().build();
                let img: uobradio_comms::video::PixelImage<uobradio_comms::video::RgbPixel> =
                    image.into();
                let cimg: egui::ColorImage = img.into();
                common.vsettings.wifi_texture =
                    Some(ctx.load_texture("qrcode", cimg, egui::TextureOptions::LINEAR));
            }
        }
    }
}

impl SubwindowTrait for Config {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        vsettings: &mut uobradio_comms::VolatileSettings,
        packet: &uobradio_comms::MessageToApp,
    ) {
        match packet {
            uobradio_comms::MessageToApp::ConnectedToWifiNetwork { ssid, password: _ } => {}
            uobradio_comms::MessageToApp::FailedToConnectToWifiNetwork { ssid } => {}
            _ => {}
        }
    }

    fn card(&self, active: bool, ui: &mut egui::Ui) -> bool {
        let button_color = if active {
            super::ACCENT_PRIMARY
        } else {
            super::BG_SECONDARY
        };
        let text_color = if active {
            egui::Color32::WHITE
        } else {
            super::TEXT_SECONDARY
        };

        let button = egui::Button::new(
            egui::RichText::new(format!("{}\n{}", "📱", "Wireless"))
                .size(16.0)
                .color(text_color),
        )
        .fill(button_color)
        .min_size(egui::vec2(70.0, 70.0))
        .corner_radius(12.0);

        ui.add(button).clicked()
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
        let _ = common
            .radio
            .send_packet(uobradio_comms::MessageFromApp::GetWifiDetails);
        self.update_qr_code(ctx, common);
        if let Some(t) = &common.vsettings.wifi_texture {
            egui::SidePanel::right("Hotspot qr code view").show(ctx, |ui| {
                let size = ui.available_size();
                let isize = t.size()[1];
                let zoom = isize as f32 / size.y;
                let dsize = t.size_vec2() / zoom;
                ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                    id: t.id(),
                    size: dsize,
                }));
            });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Future expansion here for bluetooth settings");
            if ui.button("Enable discovery").clicked() {
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::SetBluetoothDiscovery(true));
            }
            if ui.button("Disable discovery").clicked() {
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::SetBluetoothDiscovery(false));
            }
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
                if let uobradio_comms::WifiConfig::RegularNetwork = &common.settings.wifi_config {
                    if let Some((wn, _wp)) = &common.wifi_details {
                        ui.label(format!("Connected to wifi network {}", wn));
                    } else {
                        ui.label("ConnectPasswordPrompted to a wifi network");
                    }
                }
            }
            ui.label("Saved wifi networks");
            for (i, w) in common.settings.wifi_network.iter().enumerate() {
                ui.label(format!(" * {}: {}", i, w.0));
            }
            let mut scan = || {
                if ui.button("Scan for wifi networks").clicked() {
                    let _ = common
                        .radio
                        .send_packet(uobradio_comms::MessageFromApp::ScanForWifiNetworks);
                }
                for (i, w) in common.wifi_list.iter().enumerate() {
                    ui.label(format!("Wifi network {}", w.ssid));
                }
            };
            match &common.settings.wifi_config {
                uobradio_comms::WifiConfig::RegularNetwork => {
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
                    .send_packet(uobradio_comms::MessageFromApp::NewSettings {
                        settings: common.settings.clone(),
                        wifi_reconnect: reconnect,
                    });
            }
        });
        None
    }
}
