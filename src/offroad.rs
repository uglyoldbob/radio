//! Code for the offroad page

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

use crate::ConvenienceGui;

/// The offroad page for the application
#[derive(Clone, Copy)]
pub struct Window {}

impl Window {
    /// Construct a new Self
    pub fn new() -> Self {
        Self {}
    }
}

impl SubwindowTrait for Window {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _vsettings: &mut uobradio_comms::VolatileSettings,
        _packet: &uobradio_comms::MessageToApp,
    ) {
    }

    fn card(&self, active: bool, theme: &mut super::GraphicsTheme, ui: &mut egui::Ui) -> bool {
        ui.selectable_button(&theme, active, &format!("{}\n{}", "🚗", "4X4"))
            .clicked()
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut super::GraphicsTheme,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(sensors) = common.radio.sensors.value() {
                if let Some(oriented) = &sensors.orientation {
                    ui.label(
                        egui::RichText::new(format!(
                            "LR: {:.1} degrees, FB: {:.1} degrees",
                            oriented.x, oriented.y
                        ))
                        .size(32.0),
                    );
                }
            }
        });
        None
    }
}
