//! HVAC specific code

mod pid;
use pid::*;

/// The settings for the hvac
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Settings {
    /// The target temperature in fahrenheit
    pub ac_target: f32,
    /// The target temperature in fahrenheit
    pub heat_target: f32,
    /// The target temperature in fahrenheit
    pub auto_target: f32,
    /// The current hvac mode
    pub current_mode: HvacMode,
}

/// The volatile settings for the hvac
#[derive(Clone, Debug)]
pub struct VolatileSettings {
    /// The current temperature in fahrenheit
    pub current_temperature: Option<f32>,
}

impl Default for VolatileSettings {
    fn default() -> Self {
        Self {
            current_temperature: Some(71.8),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ac_target: 72.0,
            heat_target: 75.0,
            auto_target: 73.0,
            current_mode: HvacMode::Off,
        }
    }
}

/// The modes that the hvac system can be in
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HvacMode {
    /// All controls inactive
    #[default]
    Off,
    /// The ac is active in temperature control
    AcAuto,
    /// The heat is active in temperature control
    HeatAuto,
    /// The air is set to fully auto
    AutoAuto,
}

/// The structure used for controlling the hvac controls of the vehicle
pub struct HvacController {
    /// The hvac mode
    mode: HvacMode,
    /// The heat setpoint
    heat_sp: f32,
    /// The ac setpoint
    ac_sp: f32,
    /// The auto auto setpoint
    auto_sp: f32,
    /// The humidity setpoint
    humidity_sp: f32,
    /// The actual humidity
    humidity: Option<f32>,
    /// The temperature of the cabin sensor
    cabin_temperature: Option<f32>,
    // The desired fan speed
    fan_speed_out: u8,
    // Ac enabled output
    ac_enabled_out: bool,
    /// The hvac air output temperature
    hvac_vent_temperature: f32,
    /// The pid for ac control
    ac_pid: Pid,
    /// The pid for heat control
    heat_pid: Pid,
    /// Set when the fan speed is automatically calculated
    fan_auto_calc: bool,
    /// The pid for the fan speed
    fan_speed_pid: Pid,
    /// The minimum fan speed
    min_fan_speed: f32,
}

impl HvacController {
    /// Construct a new self
    pub fn new() -> Self {
        Self {
            mode: HvacMode::Off,
            heat_sp: 75.0,
            ac_sp: 70.0,
            auto_sp: 72.0,
            humidity_sp: 45.0,
            humidity: None,
            cabin_temperature: None,
            fan_speed_out: 0,
            hvac_vent_temperature: 55.0,
            ac_enabled_out: false,
            ac_pid: Pid::new(PidMode::Decreasing, 0.333, 0.2, 5.0),
            heat_pid: Pid::new(PidMode::Increasing, 0.333, 0.2, 5.0),
            fan_auto_calc: true,
            fan_speed_pid: Pid::new(PidMode::Increasing, 0.333, 0.2, 5.0),
            min_fan_speed: 0.25,
        }
    }

    /// Set the mode of the controller
    pub fn set_mode(&mut self, m: HvacMode) {
        self.mode = m;
        self.fan_speed_pid.reset();
        let m2 = match m {
            HvacMode::Off => PidMode::Increasing,
            HvacMode::AcAuto => PidMode::Decreasing,
            HvacMode::HeatAuto => PidMode::Increasing,
            HvacMode::AutoAuto => PidMode::Increasing,
        };
        self.fan_speed_pid.change_mode(m2);
    }

    /// Declare the humidity of the cabin for control purposes
    pub fn set_cabin_humidity(&mut self, h: f32) {
        self.humidity = Some(h);
    }

    /// Declare the temperature of the cabin for control purposes
    pub fn set_cabin_temperature(&mut self, t: f32) {
        self.cabin_temperature = Some(t);
    }

    /// Declare the temperature of the ac vent temperature
    pub fn set_hvac_vent_temperature(&mut self, t: f32) {
        self.hvac_vent_temperature = t;
    }

    /// Get the duty cycle for the ac compressor
    pub fn get_ac_compressor_duty_cycle(&self) -> f32 {
        self.ac_pid.duty_cycle()
    }

    /// Get the duty cycle for the heater control output
    pub fn get_heat_control_duty_cycle(&self) -> f32 {
        self.heat_pid.duty_cycle()
    }

    /// Returns the hvac vent temperature
    pub fn get_hvac_temperature(&self) -> f32 {
        self.hvac_vent_temperature
    }

    /// Returns the cabin temperature
    pub fn get_cabin_temperature(&self) -> Option<f32> {
        self.cabin_temperature
    }

    /// Set the setpoint for ac control
    pub fn set_ac_setpoint(&mut self, t: f32) {
        self.ac_sp = t;
    }

    /// Set the setpoint for heat control
    pub fn set_heat_setpoint(&mut self, t: f32) {
        self.heat_sp = t;
    }

    /// Set the setpoint for auto control
    pub fn set_auto_setpoint(&mut self, t: f32) {
        self.auto_sp = t;
    }

    /// Set the fan speed
    pub fn set_fan_speed(&mut self, s: u8) {
        self.fan_speed_out = s;
    }

    /// Get the fan speed
    pub fn get_fan_speed(&self) -> u8 {
        self.fan_speed_out
    }

    /// Operate the controls and perform any necessary calculations
    pub fn run_controls(&mut self) {
        const HYSTERESIS: f32 = 0.5;
        const MIN_AC_TEMP: f32 = 40.0;
        match self.mode {
            HvacMode::Off => {
                self.fan_speed_out = 0;
                self.ac_enabled_out = false;
                self.ac_pid.reset();
                self.heat_pid.reset();
                self.fan_speed_pid.reset();
                self.fan_speed_out = 0;
            }
            HvacMode::AcAuto => {
                self.ac_pid.set_setpoint(self.ac_sp);
                self.ac_pid.run_calc(self.hvac_vent_temperature);
                self.fan_speed_pid.set_setpoint(self.ac_sp);
                self.fan_speed_pid.run_calc(self.hvac_vent_temperature);
                self.heat_pid.reset();
                let fs = self.fan_speed_pid.duty_cycle();
                let fs = if fs < self.min_fan_speed { 0.0 } else { fs };
                self.fan_speed_out = (fs * 255.0).round() as u8;
            }
            HvacMode::HeatAuto => {
                self.ac_pid.reset();
                self.heat_pid.set_setpoint(self.heat_sp);
                self.heat_pid.run_calc(self.hvac_vent_temperature);
                self.fan_speed_pid.set_setpoint(self.heat_sp);
                self.fan_speed_pid.run_calc(self.hvac_vent_temperature);
                let fs = self.fan_speed_pid.duty_cycle();
                let fs = if fs < self.min_fan_speed { 0.0 } else { fs };
                self.fan_speed_out = (fs * 255.0).round() as u8;
            }
            HvacMode::AutoAuto => {
                if let Some(cabin) = self.cabin_temperature {
                    if (cabin + HYSTERESIS) < self.auto_sp {
                        self.fan_speed_pid.change_mode(PidMode::Increasing);
                        self.ac_pid.reset();
                        self.heat_pid.set_setpoint(self.auto_sp);
                        self.heat_pid.run_calc(cabin);
                        self.fan_speed_pid.set_setpoint(self.auto_sp);
                        self.fan_speed_pid.run_calc(cabin);
                    } else if (cabin - HYSTERESIS) > self.auto_sp {
                        self.fan_speed_pid.change_mode(PidMode::Decreasing);
                        self.ac_pid.run_calc(self.hvac_vent_temperature);
                        self.ac_pid.run_calc(cabin);
                        self.heat_pid.reset();
                        self.fan_speed_pid.set_setpoint(self.auto_sp);
                        self.fan_speed_pid.run_calc(cabin);
                    }
                } else {
                    if (self.hvac_vent_temperature + HYSTERESIS) < self.auto_sp {
                        self.fan_speed_pid.change_mode(PidMode::Increasing);
                        self.ac_pid.reset();
                        self.heat_pid.set_setpoint(self.auto_sp);
                        self.heat_pid.run_calc(self.hvac_vent_temperature);
                        self.fan_speed_pid.set_setpoint(self.auto_sp);
                        self.fan_speed_pid.run_calc(self.hvac_vent_temperature);
                    } else if (self.hvac_vent_temperature - HYSTERESIS) > self.auto_sp {
                        self.fan_speed_pid.change_mode(PidMode::Decreasing);
                        self.ac_pid.set_setpoint(self.auto_sp);
                        self.ac_pid.run_calc(self.hvac_vent_temperature);
                        self.heat_pid.reset();
                        self.fan_speed_pid.set_setpoint(self.auto_sp);
                        self.fan_speed_pid.run_calc(self.hvac_vent_temperature);
                    }
                }
                let fs = self.fan_speed_pid.duty_cycle();
                let fs = if fs < self.min_fan_speed { 0.0 } else { fs };
                self.fan_speed_out = (fs * 255.0).round() as u8;
            }
        }
    }
}

/// An ac control message
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum HvacControl {
    /// Get the temperature (fahrenheit) of the hvac vent
    GetCurrentVentTemperature,
    /// Get the temperature (fahrenheit) of the cabin
    GetCurrentCabinTemperature,
    /// Set the target temperature (fahrenheit) for temperature control of the ac
    SetAcTargetTemperature(f32),
    /// Set the target temperature (fahrenheit) for temperature control of the heat
    SetHeatTargetTemperature(f32),
    /// Set the target temperature (fahrenheit) for temperature control of the auto mode
    SetAutoTargetTemperature(f32),
    /// Set the fan speed
    SetFanSpeed(u8),
    /// Set the hvac mode
    SetMode(HvacMode),
}

/// An ac control message
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum AcResponse {
    /// The current hvac vent temperature if it is known (fahrenheit)
    CurrentHvacTemperature(Option<f32>),
    /// The current cabin temperature if it is known (fahrenheit)
    CurrentCabinTemperature(Option<f32>),
    /// Acknowledgement of set target temperature, indicates success or failure of setting hvac temperature setpoint
    TemperatureSetStatus(bool),
    /// Acknowledge set fan speed
    FanSpeedAcknowledge,
}
