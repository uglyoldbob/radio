use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Video {
    which_video: usize,
    texture: Option<egui::TextureHandle>,
}

impl Video {
    pub fn new() -> Self {
        Self {
            which_video: 0,
            texture: None,
        }
    }
}

impl SubwindowTrait for Video {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label("This is the video page");
                let mut size = ui.available_size();
                if let Some(cameras) = common.radio.cameras_mut() {
                    let vsrc = &mut cameras[self.which_video as usize];
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
                }
                if let Some(t) = &self.texture {
                    ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                        id: t.id(),
                        size,
                    }));
                }
            });
        });
        None
    }
}
