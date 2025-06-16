use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct Window {
    ac_target: f32,
    heat_target: f32,
    auto_target: f32,
    current_temperature: Option<f32>,
    current_mode: uobradio_comms::HvacMode,
}

impl Window {
    pub fn new() -> Self {
        Self {
            current_temperature: Some(71.8),
            ac_target: 72.0,
            heat_target: 75.0,
            auto_target: 73.0,
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
                    || ui
                    .selectable_value(
                        &mut self.current_mode,
                        uobradio_comms::HvacMode::AutoAuto,
                        "Auto Auto",
                    )
                    .changed()
                {
                    let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Hvac(
                        uobradio_comms::HvacControl::SetMode(self.current_mode),
                    ));
                }
            });
            if let Some(t) = self.current_temperature {
                ui.label(egui::RichText::new("Current temperature").size(32.0));
                ui.label(egui::RichText::new(format!("{:01}", t)).size(32.0));
            }
            match self.current_mode {
                uobradio_comms::HvacMode::Off => {}
                uobradio_comms::HvacMode::AcAuto => {
                    ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                    let response = ui.add(
                        egui::DragValue::new(&mut self.ac_target)
                            .range(32.0..=95.0)
                            .clamp_existing_to_range(true)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                    if response.dragged() {
                        let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Hvac(
                            uobradio_comms::HvacControl::SetAcTargetTemperature(self.ac_target),
                        ));
                    }
                }
                uobradio_comms::HvacMode::HeatAuto => {
                    ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                    let response = ui.add(
                        egui::DragValue::new(&mut self.heat_target)
                            .range(32.0..=95.0)
                            .clamp_existing_to_range(true)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                    if response.dragged() {
                        let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Hvac(
                            uobradio_comms::HvacControl::SetHeatTargetTemperature(self.heat_target),
                        ));
                    }
                }
                uobradio_comms::HvacMode::AutoAuto => {
                    ui.style_mut().drag_value_text_style = egui::TextStyle::Heading;
                    let response = ui.add(
                        egui::DragValue::new(&mut self.auto_target)
                            .range(32.0..=95.0)
                            .clamp_existing_to_range(true)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                    if response.dragged() {
                        let _ = common.radio.send_packet(uobradio_comms::MessageFromApp::Hvac(
                            uobradio_comms::HvacControl::SetAutoTargetTemperature(self.auto_target),
                        ));
                    }
                }
            }
            
        });
        None
    }
}
