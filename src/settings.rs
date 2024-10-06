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
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut size = ui.available_size();
            size.x *= 0.95;
            size.y *= 0.95;
            ui.label("Settings");
            if !common.video_sources.is_empty() {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        egui::ComboBox::from_label("Select a camera")
                            .selected_text(format!("Camera {}", self.selected_video))
                            .show_ui(ui, |ui| {
                                for i in 0..common.video_sources.len() {
                                    if ui
                                        .selectable_label(false, format!("Camera {}", i))
                                        .clicked()
                                    {
                                        self.selected_video = i;
                                    }
                                }
                            });
                        let vsrc = &mut common.video_sources[self.selected_video];
                        for c in &mut vsrc.controls {
                            if c.egui_show(ui) {
                                c.send_update(&mut vsrc.vsend);
                            }
                        }
                        if let Ok(mut i) = vsrc.image.lock() {
                            ui.checkbox(&mut i.hmirror, "H Mirror");
                            ui.checkbox(&mut i.vmirror, "V Mirror");
                        }
                    });
                    let vsrc = &mut common.video_sources[self.selected_video];
                    ui.with_layout(egui::Layout::top_down(egui::Align::TOP), |ui| {
                        if let Ok(i) = vsrc.image.lock() {
                            if let Some(pd) = &i.pixel_data {
                                let zoom =
                                    (size.x / (i.width as f32)).min(size.y / (i.height as f32));
                                size = egui::Vec2 {
                                    x: i.width as f32 * zoom,
                                    y: i.height as f32 * zoom,
                                };
                                let image = egui::ColorImage {
                                    size: [i.width as usize, i.height as usize],
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
                        }
                        if let Some(t) = &self.texture {
                            ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                                id: t.id(),
                                size,
                            }));
                        }
                    });
                });
            }
        });
        None
    }
}
