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
            let mut hotspot = common.settings.hotspot_enabled.is_some();
            if ui.checkbox(&mut hotspot, "Enable hotspot").changed() {
                if hotspot {
                    common.settings.hotspot_enabled =
                        Some(("UobRadio Hotspot".to_string(), "qwertyuiop".to_string()));
                } else {
                    common.settings.hotspot_enabled = None;
                }
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
