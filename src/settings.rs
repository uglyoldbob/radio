//! Code for the settings page

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

use crate::ConvenienceGui;

/// The settings page for the application, with sub-menus
#[derive(Clone, Copy)]
pub struct Settings {}

impl Settings {
    /// construct a new Self
    pub fn new() -> Self {
        Self {}
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

    fn card(&self, active: bool, theme: &mut super::GraphicsTheme, ui: &mut egui::Ui) -> bool {
        ui.selectable_button(&theme, active, &format!("{}\n{}", "🚗", "Settings"))
            .clicked()
    }

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut super::GraphicsTheme,
    ) -> Option<Subwindow> {
        egui::SidePanel::left("Settings tabs")
            .resizable(false)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(theme.bg_primary)
                    .inner_margin(10.0)
                    .outer_margin(0.0),
            )
            .show(ctx, |ui| {
                {
                    let active = common.vsettings.settings.tab
                        == uobradio_comms::settings::Subsetting::General;
                    let button_color = if active {
                        theme.accent_primary
                    } else {
                        theme.bg_secondary
                    };
                    let text_color = if active {
                        egui::Color32::WHITE
                    } else {
                        theme.text_secondary
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
                        common.vsettings.settings.tab =
                            uobradio_comms::settings::Subsetting::General;
                    }
                }
                if let Some(cameras) = common.radio.cameras_mut() {
                    if !cameras.is_empty() {
                        let active = common.vsettings.settings.tab
                            == uobradio_comms::settings::Subsetting::Video;
                        let button_color = if active {
                            theme.accent_primary
                        } else {
                            theme.bg_secondary
                        };
                        let text_color = if active {
                            egui::Color32::WHITE
                        } else {
                            theme.text_secondary
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
                            common.vsettings.settings.tab =
                                uobradio_comms::settings::Subsetting::Video;
                        }
                    }
                }
                {
                    let active = common.vsettings.settings.tab
                        == uobradio_comms::settings::Subsetting::Update;
                    let button_color = if active {
                        theme.accent_primary
                    } else {
                        theme.bg_secondary
                    };
                    let text_color = if active {
                        egui::Color32::WHITE
                    } else {
                        theme.text_secondary
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
                        common.vsettings.settings.tab =
                            uobradio_comms::settings::Subsetting::Update;
                    }
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            match common.vsettings.settings.tab {
                uobradio_comms::settings::Subsetting::General => {
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        ui.label("Logging interval (seconds): ");
                        for v in [1,2,5,10] {
                            if ui.selectable_button(theme, common.settings.logging_interval_seconds == v, &v.to_string()).clicked() {
                                changed = true;
                                common.settings.logging_interval_seconds = v;
                            }
                        }
                        if changed {
                            let _ = common.radio.send_packet(
                                uobradio_comms::MessageFromApp::NewSettings {
                                    settings: common.settings.clone(),
                                    #[cfg(feature = "wifi")]
                                    wifi_reconnect: false,
                                },
                            );
                        }
                    });
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
                                        .selected_text(format!(
                                            "Camera {}",
                                            common.vsettings.settings.selected_video
                                        ))
                                        .show_ui(ui, |ui| {
                                            for i in 0..cameras.len() {
                                                if ui
                                                    .selectable_label(
                                                        false,
                                                        format!("Camera {}", i),
                                                    )
                                                    .clicked()
                                                {
                                                    common.vsettings.settings.selected_video =
                                                        i as u8;
                                                }
                                            }
                                        });
                                    if let Some(vsrc) =
                                        cameras.get_mut(&common.vsettings.settings.selected_video)
                                    {
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
                                if let Some(vsrc) =
                                    cameras.get_mut(&common.vsettings.settings.selected_video)
                                {
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
                                                common.vsettings.video_texture =
                                                    Some(ctx.load_texture(
                                                        "camera0",
                                                        image,
                                                        egui::TextureOptions::LINEAR,
                                                    ));
                                            } else if let Some(t) =
                                                &mut common.vsettings.video_texture
                                            {
                                                t.set_partial(
                                                    [0, 0],
                                                    image,
                                                    egui::TextureOptions::LINEAR,
                                                );
                                            }
                                        }
                                    }
                                }
                                ui.with_layout(egui::Layout::top_down(egui::Align::TOP), |ui| {
                                    if let Some(t) = &common.vsettings.video_texture {
                                        ui.add(egui::Image::from_texture(
                                            egui::load::SizedTexture { id: t.id(), size },
                                        ));
                                    }
                                });
                            });
                        }
                    }
                }
                uobradio_comms::settings::Subsetting::Update => {
                    if let Some(version) = std::option_env!("SOFTWARE_VERSION") {
                        let t = egui::RichText::new(format!("Version {version}"))
                            .size(16.0)
                            .color(theme.text_secondary);
                        ui.label(t);
                    }
                    if !common.vsettings.settings.update_status_pending {
                        common.vsettings.settings.update_status_pending = common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GetUpdateProgress)
                            .is_ok();
                    }
                    match common.vsettings.settings.download_status {
                        uobradio_comms::settings::UpdateStatus::Idle => {
                            #[cfg(feature = "swupdate")]
                            if let Ok(true) = std::fs::exists("/data/update.swu") {
                                let button = egui::Button::new(
                                    egui::RichText::new("Install downloaded version")
                                        .size(16.0)
                                        .color(theme.text_secondary),
                                )
                                .fill(theme.bg_secondary)
                                .min_size(egui::vec2(70.0, 70.0))
                                .corner_radius(12.0);

                                if ui.add(button).clicked() {
                                    common.vsettings.settings.download_status =
                                        uobradio_comms::settings::UpdateStatus::UpdateStarted;
                                    let _ = common
                                        .radio
                                        .send_packet(uobradio_comms::MessageFromApp::StartUpdate);
                                }
                            }
                            if let Some(update_url) = std::option_env!("UPDATE_SERVER") {
                                if ui.big_button(&theme, "Check for updates").clicked() {
                                    let _ = common.radio.send_packet(
                                        uobradio_comms::MessageFromApp::DownloadServerFileList(
                                            update_url.to_string(),
                                        ),
                                    );
                                }
                                match &common.vsettings.settings.list {
                                    Ok(list) => {
                                        for f in  list {
                                            if ui.big_button(&theme, f).clicked() {
                                                common.vsettings.settings.download_status =
                                                    uobradio_comms::settings::UpdateStatus::DownloadStarted;
                                                let url = format!("{update_url}/{f}");
                                                let _ = common.radio.send_packet(
                                                    uobradio_comms::MessageFromApp::DownloadServerFile(url),
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        ui.label(&format!("Failed to get update list: {}", e));
                                    }
                                }
                            }
                        }
                        uobradio_comms::settings::UpdateStatus::DownloadStarted => {
                            let t = egui::RichText::new("Download Started")
                                .size(16.0)
                                .color(theme.text_secondary);
                            ui.label(t);
                        }
                        uobradio_comms::settings::UpdateStatus::Downloading(p) => {
                            let t = egui::RichText::new("Download Progress")
                                .size(16.0)
                                .color(theme.text_secondary);
                            ui.label(t);
                            let pb = egui::ProgressBar::new(p).corner_radius(5).show_percentage();
                            ui.add(pb);
                        }
                        uobradio_comms::settings::UpdateStatus::Completed(p) => {
                            let t = if p {
                                common.vsettings.settings.download_status =
                                    uobradio_comms::settings::UpdateStatus::UpdateStarted;
                                let _ = common
                                    .radio
                                    .send_packet(uobradio_comms::MessageFromApp::StartUpdate);
                                egui::RichText::new("Download complete")
                                    .size(16.0)
                                    .color(theme.text_secondary)
                            } else {
                                egui::RichText::new("Download failed")
                                    .size(16.0)
                                    .color(theme.text_secondary)
                            };
                            ui.label(t);
                        }
                        uobradio_comms::settings::UpdateStatus::UpdateStarted => {
                            let t = egui::RichText::new("Update Started")
                                .size(16.0)
                                .color(theme.text_secondary);
                            ui.label(t);
                        }
                        uobradio_comms::settings::UpdateStatus::UpdateProgress(step, percent) => {
                            let t = egui::RichText::new("Update Progress")
                                .size(16.0)
                                .color(theme.text_secondary);
                            ui.label(t);
                            let p = percent as f32 / 100.0;
                            let t = egui::RichText::new(format!("Update Step {step}"))
                                .size(16.0)
                                .color(theme.text_secondary);
                            ui.label(t);
                            let pb = egui::ProgressBar::new(p).corner_radius(5).show_percentage();
                            ui.add(pb);
                        }
                    }
                }
            }
        });
        None
    }
}
