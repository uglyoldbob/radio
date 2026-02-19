use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

#[derive(Clone, Copy)]
pub struct Settings {
}

impl Settings {
    pub fn new() -> Self {
        Self { }
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
        egui::SidePanel::left("Settings tabs")
            .resizable(false)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(super::BG_PRIMARY)
                    .inner_margin(10.0)
                    .outer_margin(0.0)
            )
            .show(ctx, |ui| {
                {
                    let active = common.vsettings.settings.tab == uobradio_comms::settings::Subsetting::General;
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
                        egui::RichText::new(format!("{}\n{}", "G", "General"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.settings.tab = uobradio_comms::settings::Subsetting::General;
                    }
                }
                if let Some(cameras) = common.radio.cameras_mut() {
                    if !cameras.is_empty() {
                        let active = common.vsettings.settings.tab == uobradio_comms::settings::Subsetting::Video;
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
                            egui::RichText::new(format!("{}\n{}", "V", "Video"))
                                .size(16.0)
                                .color(text_color),
                        )
                        .fill(button_color)
                        .min_size(egui::vec2(70.0, 70.0))
                        .corner_radius(12.0);

                        if ui.add(button).clicked() {
                            common.vsettings.settings.tab = uobradio_comms::settings::Subsetting::Video;
                        }
                    }
                }
                {
                    let active = common.vsettings.settings.tab == uobradio_comms::settings::Subsetting::Update;
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
                        egui::RichText::new(format!("{}\n{}", "U", "Update"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.settings.tab = uobradio_comms::settings::Subsetting::Update;
                    }
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            match common.vsettings.settings.tab {
                uobradio_comms::settings::Subsetting::General => {
                    
                }
                uobradio_comms::settings::Subsetting::Video => {
                    let mut size = ui.available_size();
                    size.x *= 0.95;
                    size.y *= 0.95;
                    if let Some(cameras) = common.radio.cameras_mut() {
                        if !cameras.is_empty() {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    egui::ComboBox::from_label("Select a camera")
                                        .selected_text(format!("Camera {}", common.vsettings.settings.selected_video))
                                        .show_ui(ui, |ui| {
                                            for i in 0..cameras.len() {
                                                if ui
                                                    .selectable_label(false, format!("Camera {}", i))
                                                    .clicked()
                                                {
                                                    common.vsettings.settings.selected_video = i as u8;
                                                }
                                            }
                                        });
                                    if let Some(vsrc) = cameras.get_mut(&common.vsettings.settings.selected_video) {
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
                                if let Some(vsrc) = cameras.get_mut(&common.vsettings.settings.selected_video) {
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
                }
                uobradio_comms::settings::Subsetting::Update => {
                    if let Ok(true) = std::fs::exists("/data/update.swu") {
                        let button = egui::Button::new(
                            egui::RichText::new("Install update")
                                .size(16.0)
                                .color(super::TEXT_SECONDARY),
                        )
                        .fill(super::BG_SECONDARY)
                        .min_size(egui::vec2(70.0, 70.0))
                        .corner_radius(12.0);

                        if ui.add(button).clicked() {
                            common.vsettings.settings.download_status = uobradio_comms::settings::UpdateStatus::UpdateStarted;
                            let _ = common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::StartUpdate);
                        }
                    }
                    match common.vsettings.settings.download_status {
                        uobradio_comms::settings::UpdateStatus::Idle => {
                            if let Some(update_url) = std::option_env!("UPDATE_SERVER") {
                                ui.label(update_url);
                                let button = egui::Button::new(
                                    egui::RichText::new("Check for updates")
                                        .size(16.0)
                                        .color(super::TEXT_SECONDARY),
                                )
                                .fill(super::BG_SECONDARY)
                                .min_size(egui::vec2(70.0, 70.0))
                                .corner_radius(12.0);

                                if ui.add(button).clicked() {
                                    let _ = common
                                        .radio
                                        .send_packet(uobradio_comms::MessageFromApp::DownloadServerFileList(update_url.to_string()));
                                }
                                for f in &common.vsettings.settings.list {
                                    let button = egui::Button::new(
                                        egui::RichText::new(f)
                                            .size(16.0)
                                            .color(super::TEXT_SECONDARY),
                                    )
                                    .fill(super::BG_SECONDARY)
                                    .min_size(egui::vec2(70.0, 70.0))
                                    .corner_radius(12.0);

                                    if ui.add(button).clicked() {
                                        common.vsettings.settings.download_status = uobradio_comms::settings::UpdateStatus::DownloadStarted;
                                        let url = format!("{update_url}/{f}");
                                        let _ = common
                                        .radio
                                        .send_packet(uobradio_comms::MessageFromApp::DownloadServerFile(url));
                                    }
                                }
                            }
                        }
                        uobradio_comms::settings::UpdateStatus::DownloadStarted => {
                            ui.label("Download started");
                        }
                        uobradio_comms::settings::UpdateStatus::Downloading(p) => {
                            let pb = egui::ProgressBar::new(p).corner_radius(5).show_percentage();
                            ui.add(pb);
                        }
                        uobradio_comms::settings::UpdateStatus::Completed(p) => {
                            if p {
                                ui.label("Download complete");
                            } else {
                                ui.label("Download failed");
                            }
                        }
                        uobradio_comms::settings::UpdateStatus::UpdateStarted => {
                            ui.label("Update started");
                        }
                    }
                }
            }
        });
        None
    }
}
