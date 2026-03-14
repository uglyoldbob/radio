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

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut super::GraphicsTheme,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(egui::RichText::new("Current mode").size(32.0));
            if let Some((lr, fb)) = common.radio.sensors.orientation {}
        });
        None
    }
}
