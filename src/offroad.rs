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
        for or in common.offroad_lights.iter_mut().enumerate() {
            or.1.poll_action(|| {
                common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::GpioQuery(
                        uobradio_comms::GpioQuery::LightControl(or.0 as u8),
                    ));
            });
        }
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
                ui.horizontal(|ui| {
                    let r = ui.big_momentary_button(theme, "Winch IN");
                    if r.drag_started() {
                        common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GpioControl(
                                uobradio_comms::Gpio::WinchControl(true, false),
                            ));
                    } else if r.drag_stopped() {
                        common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GpioControl(
                                uobradio_comms::Gpio::WinchControl(false, false),
                            ));
                    }
                    let r = ui.big_momentary_button(theme, "Winch OUT");
                    if r.drag_started() {
                        common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GpioControl(
                                uobradio_comms::Gpio::WinchControl(false, true),
                            ));
                    } else if r.drag_stopped() {
                        common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GpioControl(
                                uobradio_comms::Gpio::WinchControl(false, false),
                            ));
                    }
                });
                ui.horizontal(|ui| {
                    for or in common.offroad_lights.iter_mut().enumerate() {
                        if ui.big_toggle(theme, or.1, &format!("L{}", or.0)).clicked() {
                            if let Some(v) = or.1.proposed_value() {
                                let _ = common
                                    .radio
                                    .send_gpio(uobradio_comms::Gpio::LightControl(or.0 as u8, v));
                            }
                        }
                    }
                });
            }
        });
        None
    }
}
