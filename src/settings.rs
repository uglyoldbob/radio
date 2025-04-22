use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Settings {
    selected_video: usize,
    texture: Option<egui::TextureHandle>,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            selected_video: 0,
            texture: None,
        }
    }
}

impl SubwindowTrait for Settings {
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
                                            self.selected_video = i;
                                        }
                                    }
                                });
                            let vsrc = &mut cameras[self.selected_video];
                            for c in &mut vsrc.controls {
                                if c.egui_show(ui) {
                                    todo!();
                                    //c.send_update(&mut vsrc.vsend);
                                }
                            }
                            ui.checkbox(&mut vsrc.image.hmirror, "H Mirror");
                            ui.checkbox(&mut vsrc.image.vmirror, "V Mirror");
                        });
                        let vsrc = &mut cameras[self.selected_video];
                        if let Some(pd) = &vsrc.image.pixel_data {
                            let zoom = (size.x / (vsrc.image.width as f32)).min(size.y / (vsrc.image.height as f32));
                            size = egui::Vec2 {
                                x: vsrc.image.width as f32 * zoom,
                                y: vsrc.image.height as f32 * zoom,
                            };
                            let image = egui::ColorImage {
                                size: [vsrc.image.width as usize, vsrc.image.height as usize],
                                pixels: pd.get_egui(),
                            };
                            if let None = self.texture {
                                self.texture = Some(ctx.load_texture(
                                    "camera0",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            } else if let Some(t) = &mut self.texture {
                                t.set_partial([0, 0], image, egui::TextureOptions::LINEAR);
                            }
                        }
                        ui.with_layout(egui::Layout::top_down(egui::Align::TOP), |ui| {
                            if let Some(t) = &self.texture {
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
