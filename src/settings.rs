use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Settings {
    selected_video: usize,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            selected_video: 0,
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
            ui.label("Settings");
            if !common.video_sources.is_empty() {
                egui::ComboBox::from_label("Select a camera")
                    .selected_text(format!("Camera {}", self.selected_video))
                    .show_ui(ui, |ui| {
                        for i in 0..common.video_sources.len() {
                            if ui.selectable_label(false, format!("Camera {}", i)).clicked() {
                                self.selected_video = i;
                            }
                        }
                    });
                let vsrc = &mut common.video_sources[self.selected_video];
                for c in &mut vsrc.controls {
                    if c.egui_show(ui) {
                        println!("Need to update {:?} control", c);
                    }
                }
            }
        });
        None
    }
}
