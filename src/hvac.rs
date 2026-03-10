//! Code for the hvac control page

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

/// The hvac control page
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
            egui::RichText::new(format!("{}\n{}", "🚗", "HVAC"))
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
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.bg_primary))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Current mode").size(32.0));
                ui.horizontal(|ui| {
                    if ui
                        .selectable_value(
                            &mut common.settings.hvac.current_mode,
                            uobradio_comms::HvacMode::Off,
                            "OFF",
                        )
                        .changed()
                        || ui
                            .selectable_value(
                                &mut common.settings.hvac.current_mode,
                                uobradio_comms::HvacMode::AcAuto,
                                "Auto AC",
                            )
                            .changed()
                        || ui
                            .selectable_value(
                                &mut common.settings.hvac.current_mode,
                                uobradio_comms::HvacMode::HeatAuto,
                                "Auto Heat",
                            )
                            .changed()
                        || ui
                            .selectable_value(
                                &mut common.settings.hvac.current_mode,
                                uobradio_comms::HvacMode::AutoAuto,
                                "Auto Auto",
                            )
                            .changed()
                    {
                        let _ = common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::Hvac(
                                uobradio_comms::HvacControl::SetMode(
                                    common.settings.hvac.current_mode,
                                ),
                            ));
                    }
                });
                if let Some(t) = common.vsettings.hvac.current_temperature {
                    ui.label(egui::RichText::new("Current temperature").size(32.0));
                    ui.label(egui::RichText::new(format!("{:01}", t)).size(32.0));
                }
                match common.settings.hvac.current_mode {
                    uobradio_comms::HvacMode::Off => {}
                    uobradio_comms::HvacMode::AcAuto => {
                        ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                        let response = ui.add(
                            egui::DragValue::new(&mut common.settings.hvac.ac_target)
                                .range(32.0..=95.0)
                                .clamp_existing_to_range(true)
                                .speed(0.1)
                                .fixed_decimals(1),
                        );
                        if response.dragged() {
                            let _ = common
                                .radio
                                .send_packet(uobradio_comms::MessageFromApp::Hvac(
                                    uobradio_comms::HvacControl::SetAcTargetTemperature(
                                        common.settings.hvac.ac_target,
                                    ),
                                ));
                        }
                    }
                    uobradio_comms::HvacMode::HeatAuto => {
                        ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                        let response = ui.add(
                            egui::DragValue::new(&mut common.settings.hvac.heat_target)
                                .range(32.0..=95.0)
                                .clamp_existing_to_range(true)
                                .speed(0.1)
                                .fixed_decimals(1),
                        );
                        if response.dragged() {
                            let _ = common
                                .radio
                                .send_packet(uobradio_comms::MessageFromApp::Hvac(
                                    uobradio_comms::HvacControl::SetHeatTargetTemperature(
                                        common.settings.hvac.heat_target,
                                    ),
                                ));
                        }
                    }
                    uobradio_comms::HvacMode::AutoAuto => {
                        ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                        let response = ui.add(
                            egui::DragValue::new(&mut common.settings.hvac.auto_target)
                                .range(32.0..=95.0)
                                .clamp_existing_to_range(true)
                                .speed(0.1)
                                .fixed_decimals(1),
                        );
                        if response.dragged() {
                            let _ = common
                                .radio
                                .send_packet(uobradio_comms::MessageFromApp::Hvac(
                                    uobradio_comms::HvacControl::SetAutoTargetTemperature(
                                        common.settings.hvac.auto_target,
                                    ),
                                ));
                        }
                    }
                }
            });
        None
    }
}
