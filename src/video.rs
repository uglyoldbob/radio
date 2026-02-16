use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

#[derive(Clone, Copy)]
pub struct Video {}

impl Video {
    pub fn new() -> Self {
        Self {}
    }
}

impl SubwindowTrait for Video {
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
            egui::RichText::new(format!("{}\n{}", "🚗", "Video"))
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
        let h = ctx.screen_rect().height();
        egui::SidePanel::right("Camera view").show(ctx, |ui| {
            let size = ui.available_size();
            if let Some(t) = &common.vsettings.video_texture {
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
            if common
                .radio
                .send_camera_request(common.vsettings.which_video)
                .is_err()
            {
                common.radio.disconnect();
            }
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .max_height(h)
                .show(ui, |ui| {
                    let mut packets_to_send = Vec::new();
                    if let Some(cameras) = common.radio.cameras_mut() {
                        if let Some(vsrc) = cameras.get_mut(&common.vsettings.which_video) {
                            for (i, c) in &mut vsrc.controls.iter_mut().enumerate() {
                                if c.egui_show(ui) {
                                    let packet =
                                        uobradio_comms::MessageFromApp::CameraSettingControl(
                                            common.vsettings.which_video,
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
                    }
                    for packet in packets_to_send {
                        if common.radio.send_packet(packet).is_err() {
                            break;
                        }
                    }
                });
        });
        None
    }
}
