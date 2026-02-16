use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

#[derive(Clone, Copy)]
pub struct Settings {
    selected_video: u8,
}

impl Settings {
    pub fn new() -> Self {
        Self { selected_video: 0 }
    }
}

impl SubwindowTrait for Settings {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _vsettings: &mut uobradio_comms::VolatileSettings,
        _packet: &uobradio_comms::MessageToApp,
    ) {
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
            egui::RichText::new(format!("{}\n{}", "🚗", "Settings"))
                .size(16.0)
                .color(text_color),
        )
        .fill(button_color)
        .min_size(egui::vec2(140.0, 70.0))
        .corner_radius(12.0);

        ui.add(button).clicked()
    }

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut size = ui.available_size();
            size.x *= 0.95;
            size.y *= 0.95;
            ui.label("Settings");
            if let Some(cameras) = common.radio.cameras_mut() {
                if !cameras.is_empty() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            egui::ComboBox::from_label("Select a camera")
                                .selected_text(format!("Camera {}", self.selected_video))
                                .show_ui(ui, |ui| {
                                    for i in 0..cameras.len() {
                                        if ui
                                            .selectable_label(false, format!("Camera {}", i))
                                            .clicked()
                                        {
                                            self.selected_video = i as u8;
                                        }
                                    }
                                });
                            if let Some(vsrc) = cameras.get_mut(&self.selected_video) {
                                for c in &mut vsrc.controls {
                                    if c.egui_show(ui) {
                                        todo!();
                                        //c.send_update(&mut vsrc.vsend);
                                    }
                                }
                                if let Some(image) = &mut vsrc.image {
                                    ui.checkbox(&mut image.hmirror, "H Mirror");
                                    ui.checkbox(&mut image.vmirror, "V Mirror");
                                }
                            }
                        });
                        if let Some(vsrc) = cameras.get_mut(&self.selected_video) {
                            if let Some(image) = &vsrc.image {
                                if let Some(pd) = &image.pixel_data {
                                    let zoom = (size.x / (image.width as f32))
                                        .min(size.y / (image.height as f32));
                                    size = egui::Vec2 {
                                        x: image.width as f32 * zoom,
                                        y: image.height as f32 * zoom,
                                    };
                                    let image = egui::ColorImage {
                                        size: [image.width as usize, image.height as usize],
                                        pixels: pd.get_egui(),
                                    };
                                    if common.vsettings.video_texture.is_none() {
                                        common.vsettings.video_texture = Some(ctx.load_texture(
                                            "camera0",
                                            image,
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    } else if let Some(t) = &mut common.vsettings.video_texture {
                                        t.set_partial([0, 0], image, egui::TextureOptions::LINEAR);
                                    }
                                }
                            }
                        }
                        ui.with_layout(egui::Layout::top_down(egui::Align::TOP), |ui| {
                            if let Some(t) = &common.vsettings.video_texture {
                                ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                                    id: t.id(),
                                    size,
                                }));
                            }
                        });
                    });
                }
            }
        });
        None
    }
}
