use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Video {
    which_video: u8,
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
        let h = ctx.screen_rect().height();
        egui::SidePanel::right("Camera view").show(ctx, |ui| {
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
            ui.label(format!("This is the video page {}", h));
            if common.radio.send_camera_request(self.which_video).is_err() {
                common.radio.disconnect();
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .max_height(h)
                .show(ui, |ui| {
                    let mut packets_to_send = Vec::new();
                    if let Some(cameras) = common.radio.cameras_mut() {
                        if let Some(vsrc) = cameras.get_mut(&self.which_video) {
                            for (i, c) in &mut vsrc.controls.iter_mut().enumerate() {
                                if c.egui_show(ui) {
                                    let packet =
                                        uobradio_comms::MessageFromApp::CameraSettingControl(
                                            self.which_video,
                                            i as u8,
                                            c.value.clone(),
                                        );
                                    packets_to_send.push(packet);
                                }
                            }
                            if let Some(image) = &vsrc.image {
                                if let Some(pd) = &image.pixel_data {
                                    let image = egui::ColorImage {
                                        size: [image.width as usize, image.height as usize],
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
                        }
                    }
                    for packet in packets_to_send {
                        common.radio.send_packet(packet);
                    }
                });
        });
        None
    }
}
