use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Window {
    ac_target: f32,
    current_temperature: Option<f32>,
    current_mode: uobradio_comms::HvacMode,
}

impl Window {
    pub fn new() -> Self {
        Self {
            current_temperature: Some(71.8),
            ac_target: 72.0,
            current_mode: uobradio_comms::HvacMode::Off,
        }
    }
}

impl SubwindowTrait for Window {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _packet: &uobradio_comms::MessageToApp,
    ) {
    }

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(egui::RichText::new("Current mode").size(32.0));
            ui.horizontal(|ui| {
                if ui
                    .selectable_value(&mut self.current_mode, uobradio_comms::HvacMode::Off, "OFF")
                    .changed()
                    || ui
                        .selectable_value(
                            &mut self.current_mode,
                            uobradio_comms::HvacMode::AcAuto,
                            "Auto AC",
                        )
                        .changed()
                    || ui
                        .selectable_value(
                            &mut self.current_mode,
                            uobradio_comms::HvacMode::HeatAuto,
                            "Auto Heat",
                        )
                        .changed()
                {
                    let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Ac(
                        uobradio_comms::AcControl::SetMode(self.current_mode),
                    ));
                }
            });
            if let Some(t) = self.current_temperature {
                ui.label(egui::RichText::new("Current temperature").size(32.0));
                ui.label(egui::RichText::new(format!("{:01}", t)).size(32.0));
            }
            ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
            let response = ui.add(
                egui::DragValue::new(&mut self.ac_target)
                    .range(32.0..=95.0)
                    .clamp_existing_to_range(true)
                    .speed(0.1)
                    .fixed_decimals(1),
            );
            if response.dragged() {
                let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Ac(
                    uobradio_comms::AcControl::SetAcTargetTemperature(self.ac_target),
                ));
            }
        });
        None
    }
}
