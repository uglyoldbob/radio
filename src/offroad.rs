//! Code for the offroad page

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

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
            egui::RichText::new(format!("{}\n{}", "🚗", "4X4"))
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
        theme: &mut super::GraphicsTheme,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(egui::RichText::new("Current mode").size(32.0));
            if let Some((lr, fb)) = common.radio.sensors.orientation {}
        });
        None
    }
}
