//! Code for the wifi configuration on the radio

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Screen {
    /// For the qr code
    texture: Option<egui::TextureHandle>,
}

impl Screen {
    pub fn new() -> Self {
        Self { texture: None }
    }

    pub fn make_wifi_qr(&self, wifi_name: &String, wifi_password: &String) -> Vec<u8> {
        log::info!("Making qr code with {}/{}", wifi_name, wifi_password);
        let a = format!("WIFI:S:{};T:WPA;P:{};H:false;;", wifi_name, wifi_password);
        log::info!("Qr code contents ->{}", a);
        a.as_bytes().to_vec()
    }
}

impl SubwindowTrait for Screen {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        let _ = common
            .radio
            .send_packet(uobradio_comms::MessageFromApp::RequestSettings);
        if self.texture.is_none() {
            if let Some((wn, wp)) = &common.settings.hotspot_enabled {
                let contents = self.make_wifi_qr(wn, wp);
                let code = qrcode::QrCode::new(contents).unwrap();
                let image = code.render::<image::Rgb<u8>>().build();
                let img: uobradio_comms::video::PixelImage<uobradio_comms::video::RgbPixel> =
                    image.into();
                let cimg: egui::ColorImage = img.into();
                self.texture = Some(ctx.load_texture("qrcode", cimg, egui::TextureOptions::LINEAR));
            }
        }
        if common.settings.hotspot_enabled.is_none() && self.texture.is_some() {
            self.texture.take();
        }
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
                    common.settings.wifi_config = uobradio_comms::WifiConfig::Hotspot;
                }
                if ui
                    .add(egui::SelectableLabel::new(
                        common.settings.wifi_config == uobradio_comms::WifiConfig::RegularNetwork,
                        "Regular network",
                    ))
                    .clicked()
                {
                    save = true;
                    common.settings.wifi_config = uobradio_comms::WifiConfig::RegularNetwork;
                }
                if ui
                    .add(egui::SelectableLabel::new(
                        common.settings.wifi_config == uobradio_comms::WifiConfig::Scanning,
                        "Scan for networks",
                    ))
                    .clicked()
                {
                    save = true;
                    common.settings.wifi_config = uobradio_comms::WifiConfig::Scanning;
                }
            }
            let mut hotspot = common.settings.hotspot_enabled.is_some();
            if ui.checkbox(&mut hotspot, "Configure hotspot").changed() {
                if hotspot {
                    common.settings.hotspot_enabled =
                        Some(("UobRadio Hotspot".to_string(), "qwertyuiop".to_string()));
                } else {
                    common.settings.hotspot_enabled = None;
                }
                save = true;
            }
            if let Some(hs) = &mut common.settings.hotspot_enabled {
                let mut change = false;
                ui.label("Hotspot name");
                if ui.text_edit_singleline(&mut hs.0).changed() {
                    save = true;
                }
                ui.label("Hotspot password");
                if ui.text_edit_singleline(&mut hs.1).changed() {
                    save = true;
                }
            }
            if save {
                log::info!("Sending new settings: {:?}", common.settings);
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::NewSettings(
                        common.settings.clone(),
                    ));
            }
        });
        None
    }
}
