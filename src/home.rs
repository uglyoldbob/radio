//! The home page for the radio

use crate::gauge;
use crate::swipable;
use crate::{
    CommonWindowProperties, GaugeValue, GraphicsTheme, InclinometerOrientation, Pollable, Sensors,
    Subwindow, SubwindowTrait,
};

use crate::ConvenienceGui;

/// The main page for the gui
#[derive(Clone)]
pub struct MainPage {
    pages: swipable::SwipablePages,
    historical: Pollable<Vec<Sensors>>,
    history_popup: Option<GaugeValue>,
}

impl MainPage {
    pub fn new() -> Self {
        Self {
            pages: swipable::SwipablePages::new(3),
            historical: Default::default(),
            history_popup: None,
        }
    }
}

impl SubwindowTrait for MainPage {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
        theme: &mut GraphicsTheme,
    ) -> Option<Subwindow> {
        let r = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            #[cfg(feature = "androidauto")]
            {
                if common.radio.android_auto_frontend() {
                    let size = ui.available_size();
                    if let Some(t) = &common.android_auto_texture {
                        let isize = t.size();
                        let zoom = isize[1] as f32 / size.y;
                        let zoom2 = isize[0] as f32 / size.x;
                        let zoom = zoom.max(zoom2);
                        let dsize = t.size_vec2() / zoom;
                        let p = ui.cursor();
                        let r = ui.add(
                            egui::Image::from_texture(egui::load::SizedTexture {
                                id: t.id(),
                                size: dsize,
                            })
                            .sense(egui::Sense::drag()),
                        );
                        let o = if let Some(mut o) = r.interact_pointer_pos() {
                            o.x -= p.left();
                            o.y -= p.top();
                            o.x *= zoom;
                            o.y *= zoom;
                            Some(o)
                        } else if let Some(mut o) = r.hover_pos() {
                            o.x -= p.left();
                            o.y -= p.top();
                            o.x *= zoom;
                            o.y *= zoom;
                            Some(o)
                        } else {
                            None
                        };
                        if let Some(o) = o {
                            let mut i_event = android_auto::Wifi::InputEventIndication::new();
                            let timestamp: u64 = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_micros()
                                as u64;
                            i_event.set_timestamp(timestamp);
                            let mut te = android_auto::Wifi::TouchEvent::new();
                            let mut tl = android_auto::Wifi::TouchLocation::new();
                            tl.set_x(o.x as u32);
                            tl.set_y(o.y as u32);
                            tl.set_pointer_id(0);
                            te.touch_location = vec![tl];
                            let mut do_touch = true;
                            if r.drag_started() {
                                te.set_touch_action(android_auto::Wifi::touch_action::Enum::PRESS);
                            } else if r.drag_stopped() {
                                te.set_touch_action(
                                    android_auto::Wifi::touch_action::Enum::RELEASE,
                                );
                            } else if r.dragged() {
                                te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                            } else if r.hovered() {
                                te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
                            } else {
                                do_touch = false;
                            }
                            if do_touch {
                                i_event.touch_event =
                                    android_auto::protobuf::MessageField::some(te);
                                let e = android_auto::AndroidAutoMessage::Input(i_event);
                                let m2 = uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(
                                    e.sendable(),
                                );
                                let _ = common.radio.send_packet(
                                    uobradio_comms::MessageFromApp::AndroidAutoMessage(m2),
                                );
                            }
                        }
                    }
                }
            }
            if let Some(index) = self.history_popup {
                let id: egui::ViewportId = egui::ViewportId::from_hash_of("gauge_history");
                let builder = egui::ViewportBuilder::default()
                    .with_title("Gauge history")
                    .with_always_on_top()
                    .with_position((ctx.content_rect().size() / 4.0).to_pos2())
                    .with_max_inner_size(ctx.content_rect().size() / 2.0);
                ctx.show_viewport_immediate(id, builder, |ctx, _class| {
                    self.historical.poll_action(|| {
                        common
                            .radio
                            .send_packet(uobradio_comms::MessageFromApp::GetHistoricalData);
                    });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if ui.big_button(theme, "Close").clicked() {
                            self.history_popup = None;
                        }
                        ui.vertical_centered(|ui| {
                            if let Some(hp) = self.historical.value() {
                                let points: Vec<[f64; 2]> = hp
                                    .iter()
                                    .enumerate()
                                    .map(|v| {
                                        let a = match index {
                                            GaugeValue::OrientationX => {
                                                v.1.orientation
                                                    .clone()
                                                    .unwrap_or(InclinometerOrientation {
                                                        x: 0.0,
                                                        y: 0.0,
                                                    })
                                                    .x
                                                    as f64
                                            }
                                            GaugeValue::OrientationY => {
                                                v.1.orientation
                                                    .clone()
                                                    .unwrap_or(InclinometerOrientation {
                                                        x: 0.0,
                                                        y: 0.0,
                                                    })
                                                    .y
                                                    as f64
                                            }
                                            GaugeValue::CoolantTemp => {
                                                v.1.engine_coolant_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::OilTemp => {
                                                v.1.engine_oil_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::IntakeTemp => {
                                                v.1.intake_air_temperature.unwrap_or_default()
                                                    as f64
                                            }
                                            GaugeValue::ExhaustTemp => {
                                                v.1.engine_exhaust_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::FrontAxleTemp => {
                                                v.1.front_diff_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::RearAxleTemp => {
                                                v.1.rear_diff_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::TranmissionTemp => {
                                                v.1.trans_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::TCaseTemp => {
                                                v.1.transfer_temp.unwrap_or_default() as f64
                                            }
                                            GaugeValue::EngineRpm => {
                                                v.1.engine_rpm.unwrap_or_default() as f64
                                            }
                                            GaugeValue::MainVoltage => {
                                                v.1.main_voltage.unwrap_or_default() as f64
                                            }
                                            GaugeValue::OilPressure => {
                                                v.1.engine_oil_pressure.unwrap_or_default() as f64
                                            }
                                            GaugeValue::CoolantPressure => {
                                                v.1.coolant_pressure.unwrap_or_default() as f64
                                            }
                                            GaugeValue::VehicleSpeed => 42.42,
                                            GaugeValue::FuelLevel => 0.25,
                                        };
                                        [v.0 as f64, a]
                                    })
                                    .collect();
                                let line = egui_plot::Line::new("Gauge", points);
                                egui_plot::Plot::new("gauge_plot")
                                    .show(ui, |plot_ui| plot_ui.line(line));
                            }
                        })
                    });
                });
            }
            self.pages.show(ui, |ui, page| match page {
                0 => {
                    if let Some(sensors) = common.radio.sensors.value() {
                        let sz = egui::Vec2::splat(200.0);
                        ui.horizontal(|ui| {
                            if let Some(rpm) = sensors.engine_rpm {
                                if (gauge::Gauge {
                                    label: "TACHOMETER",
                                    unit: "x100 RPM",
                                    min: 6.0,
                                    max: 36.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: Some(30.0),
                                    major_interval: 6.0,
                                    minor_per_major: 5,
                                })
                                .draw(ui, sz, rpm as f32 / 100.0, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::EngineRpm);
                                }
                            }

                            if let Some(temp) = sensors.engine_coolant_temp {
                                if (gauge::Gauge {
                                    label: "ENGINE",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| {
                                    if v < 130.0 {
                                        "C".to_string()
                                    } else if v < 200.0 {
                                        "N".to_string()
                                    } else {
                                        "H".to_string()
                                    }
                                })
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::CoolantTemp);
                                }
                            }

                            if let Some(temp) = sensors.engine_exhaust_temp {
                                if (gauge::Gauge {
                                    label: "EGR",
                                    unit: "F",
                                    min: 100.0,
                                    max: 1300.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(1100.0),
                                    major_interval: 150.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::ExhaustTemp);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if let Some(temp) = sensors.engine_oil_temp {
                                if (gauge::Gauge {
                                    label: "OIL",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::OilTemp);
                                }
                            }

                            if let Some(speed) = sensors.engine_oil_pressure {
                                if (gauge::Gauge {
                                    label: "OIL PRESSURE",
                                    unit: "PSI",
                                    min: 0.0,
                                    max: 120.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: None,
                                    major_interval: 20.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, speed, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::OilPressure);
                                }
                            }

                            if let Some(p) = sensors.coolant_pressure {
                                if (gauge::Gauge {
                                    label: "COOLANT PRESSURE",
                                    unit: "PSI",
                                    min: 0.0,
                                    max: 30.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: None,
                                    major_interval: 5.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, p, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::CoolantPressure);
                                }
                            }
                        });
                    }
                }
                1 => {
                    if let Some(sensors) = common.radio.sensors.value() {
                        let sz = egui::Vec2::splat(200.0);
                        ui.horizontal(|ui| {
                            if let Some(temp) = sensors.front_diff_temp {
                                if (gauge::Gauge {
                                    label: "F AXLE",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::FrontAxleTemp);
                                }
                            }

                            if let Some(temp) = sensors.trans_temp {
                                if (gauge::Gauge {
                                    label: "TRANSMISSION",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::TranmissionTemp);
                                }
                            }

                            if let Some(v) = sensors.main_voltage {
                                if (gauge::Gauge {
                                    label: "VOLTAGE",
                                    unit: "V",
                                    min: 9.0,
                                    max: 19.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(15.0),
                                    major_interval: 5.0,
                                    minor_per_major: 5,
                                })
                                .draw(ui, sz, v, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::MainVoltage);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if let Some(temp) = sensors.rear_diff_temp {
                                if (gauge::Gauge {
                                    label: "R AXLE",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::RearAxleTemp);
                                }
                            }

                            if let Some(temp) = sensors.transfer_temp {
                                if (gauge::Gauge {
                                    label: "XFER CASE",
                                    unit: "F",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::TCaseTemp);
                                }
                            }

                            if let Some(speed) = Some(0.5) {
                                if (gauge::Gauge {
                                    label: "SPEEDOMETER",
                                    unit: "MPH",
                                    min: 0.0,
                                    max: 120.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: None,
                                    major_interval: 20.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, speed, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::VehicleSpeed);
                                }
                            }
                        });
                    }
                }
                2 => {
                    if let Some(sensors) = common.radio.sensors.value() {
                        let sz = egui::Vec2::splat(200.0);
                        ui.horizontal(|ui| {
                            if let Some(level) = Some(0.42) {
                                if (gauge::Gauge {
                                    label: "FUEL",
                                    unit: "",
                                    min: 0.0,
                                    max: 1.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(0.12),
                                    major_interval: 0.25,
                                    minor_per_major: 2,
                                })
                                .draw(ui, sz, level, |v| match (v * 4.0).round() as i32 {
                                    0 => "E".to_string(),
                                    1 => "1/4".to_string(),
                                    2 => "1/2".to_string(),
                                    3 => "3/4".to_string(),
                                    _ => "F".to_string(),
                                })
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::FuelLevel);
                                }
                            }

                            if let Some(temp) = sensors.engine_coolant_temp {
                                if (gauge::Gauge {
                                    label: "TEMP",
                                    unit: "",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| {
                                    if v < 130.0 {
                                        "C".to_string()
                                    } else if v < 200.0 {
                                        "N".to_string()
                                    } else {
                                        "H".to_string()
                                    }
                                })
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::CoolantTemp);
                                }
                            }

                            if let Some(level) = Some(0.42) {
                                if (gauge::Gauge {
                                    label: "FUEL",
                                    unit: "",
                                    min: 0.0,
                                    max: 1.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(0.12),
                                    major_interval: 0.25,
                                    minor_per_major: 2,
                                })
                                .draw(ui, sz, level, |v| match (v * 4.0).round() as i32 {
                                    0 => "E".to_string(),
                                    1 => "1/4".to_string(),
                                    2 => "1/2".to_string(),
                                    3 => "3/4".to_string(),
                                    _ => "F".to_string(),
                                })
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::FuelLevel);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if let Some(rpm) = sensors.engine_rpm {
                                if (gauge::Gauge {
                                    label: "TACHOMETER",
                                    unit: "x100 RPM",
                                    min: 0.0,
                                    max: 36.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: Some(30.0),
                                    major_interval: 6.0,
                                    minor_per_major: 5,
                                })
                                .draw(ui, sz, rpm as f32 / 100.0, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::EngineRpm);
                                }
                            }

                            if let Some(temp) = sensors.engine_coolant_temp {
                                if (gauge::Gauge {
                                    label: "TEMP",
                                    unit: "",
                                    min: 70.0,
                                    max: 260.0,
                                    start_deg: 215.0,
                                    end_deg: -35.0,
                                    red_start: Some(230.0),
                                    major_interval: 40.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, temp, |v| {
                                    if v < 130.0 {
                                        "C".to_string()
                                    } else if v < 200.0 {
                                        "N".to_string()
                                    } else {
                                        "H".to_string()
                                    }
                                })
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::CoolantTemp);
                                }
                            }

                            if let Some(speed) = Some(0.5) {
                                if (gauge::Gauge {
                                    label: "SPEEDOMETER",
                                    unit: "MPH",
                                    min: 0.0,
                                    max: 120.0,
                                    start_deg: 220.0,
                                    end_deg: -40.0,
                                    red_start: None,
                                    major_interval: 20.0,
                                    minor_per_major: 4,
                                })
                                .draw(ui, sz, speed, |v| format!("{:.0}", v))
                                .clicked()
                                {
                                    self.history_popup = Some(GaugeValue::VehicleSpeed);
                                }
                            }
                        });
                    }
                }
                _ => {
                    log::error!("Unknown page {page}");
                }
            });
        });
        r
    }

    fn card(&self, active: bool, theme: &mut GraphicsTheme, ui: &mut egui::Ui) -> bool {
        ui.selectable_button(&theme, active, &format!("{}\n{}", "H", "Home"))
            .clicked()
    }

    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _vsettings: &mut uobradio_comms::VolatileSettings,
        packet: &uobradio_comms::MessageToApp,
    ) {
        match packet {
            uobradio_comms::MessageToApp::HistoricalSensorData(h) => {
                self.historical.new_value_optional(Some(h.clone()));
            }
            _ => {}
        }
    }
}
