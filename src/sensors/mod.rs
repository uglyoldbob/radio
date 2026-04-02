//! Sensor code for the radio

use uobradio_comms::Sensors;

/// The trait for gps sensors
#[enum_dispatch::enum_dispatch]
pub trait GpsSensorTrait {}

/// The trait for inclinometer sensors
#[enum_dispatch::enum_dispatch]
pub trait InclinometerSensorTrait {
    /// Poll the sensor for the current orientation
    fn poll(&mut self) -> uobradio_comms::InclinometerOrientation;
}

/// A temperature reading
pub enum Temperature {
    /// The temperature units are fahrenheit
    Fahrenheit(f32),
    /// The temperature units are celsius
    Celsius(f32),
}

impl Temperature {
    /// Get the temperature in fahrenheit
    pub fn fahrenheit(&self) -> f32 {
        match self {
            Self::Fahrenheit(f) => *f,
            Self::Celsius(c) => c * 9.0 / 5.0 + 32.0,
        }
    }

    /// Get the temperature in fahrenheit
    pub fn celsius(&self) -> f32 {
        match self {
            Self::Fahrenheit(f) => (f - 32.0) * 5.0 / 9.0,
            Self::Celsius(c) => *c,
        }
    }
}

/// The trait for temperature sensors
#[enum_dispatch::enum_dispatch]
pub trait TemperatureSensorTrait {
    /// Poll the sensor for the current temperature in fahrenheit
    fn poll(&mut self) -> Temperature;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(InclinometerSensorTrait)]
pub enum InclinometerSensor {
    Simulated(InclinometerSimulator),
}

impl Default for InclinometerSensor {
    fn default() -> Self {
        Self::Simulated(InclinometerSimulator::default())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(TemperatureSensorTrait)]
pub enum TemperatureSensor {
    Simulated(TemperatureSimulator),
}

impl Default for TemperatureSensor {
    fn default() -> Self {
        Self::Simulated(TemperatureSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct InclinometerSimulator {
    x: f32,
    y: f32,
}

impl InclinometerSensorTrait for InclinometerSimulator {
    fn poll(&mut self) -> uobradio_comms::InclinometerOrientation {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.x += 2.0 * rng.random::<f32>() - 1.0;
        self.y += 2.0 * rng.random::<f32>() - 1.0;
        self.x = self.x.clamp(-45.0, 45.0);
        self.y = self.y.clamp(-45.0, 45.0);
        uobradio_comms::InclinometerOrientation {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TemperatureSimulator {
    temp: f32,
}

impl Default for TemperatureSimulator {
    fn default() -> Self {
        Self { temp: 72.3 }
    }
}

impl TemperatureSensorTrait for TemperatureSimulator {
    fn poll(&mut self) -> Temperature {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.temp = (self.temp + 2.0 * rng.random::<f32>() - 1.0).clamp(-5.0, 110.0);
        Temperature::Fahrenheit(self.temp)
    }
}

#[enum_dispatch::enum_dispatch]
pub trait PressureSensorTrait {
    /// Poll and return the pressure in psi
    fn poll(&mut self) -> f32;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(PressureSensorTrait)]
pub enum PressureSensor {
    Simulator(PressureSensorSimulator),
}

impl Default for PressureSensor {
    fn default() -> Self {
        Self::Simulator(PressureSensorSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PressureSensorSimulator {
    pressure: f32,
}

impl PressureSensorTrait for PressureSensorSimulator {
    fn poll(&mut self) -> f32 {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.pressure = (self.pressure + 2.0 * rng.random::<f32>() - 1.0).clamp(0.0, 100.0);
        self.pressure
    }
}

#[enum_dispatch::enum_dispatch]
pub trait BoolSensorTrait {
    /// Poll and return the bool value of the gpio
    fn poll(&mut self) -> bool;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(BoolSensorTrait)]
pub enum BoolSensor {
    Simulator(BoolSensorSimulator),
}

impl Default for BoolSensor {
    fn default() -> Self {
        Self::Simulator(BoolSensorSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BoolSensorSimulator {
}

impl BoolSensorTrait for BoolSensorSimulator {
    fn poll(&mut self) -> bool {
        use rand::RngExt;
        let mut rng = rand::rng();
        rng.random()
    }
}

#[enum_dispatch::enum_dispatch]
pub trait VoltageSensorTrait {
    /// Poll and return the voltage in volts
    fn poll(&mut self) -> f32;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(VoltageSensorTrait)]
pub enum VoltageSensor {
    Simulator(VoltageSensorSimulator),
}

impl Default for VoltageSensor {
    fn default() -> Self {
        Self::Simulator(VoltageSensorSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct VoltageSensorSimulator {
    volts: f32,
}

impl VoltageSensorTrait for VoltageSensorSimulator {
    fn poll(&mut self) -> f32 {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.volts = (self.volts + 2.0 * rng.random::<f32>() - 1.0).clamp(10.0, 15.0);
        self.volts
    }
}


#[enum_dispatch::enum_dispatch]
pub trait RpmSensorTrait {
    /// Poll and return the rpm
    fn poll(&mut self) -> u16;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(RpmSensorTrait)]
pub enum RpmSensor {
    Simulator(RpmSensorSimulator),
}

impl Default for RpmSensor {
    fn default() -> Self {
        Self::Simulator(RpmSensorSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RpmSensorSimulator {
    rpm: f32,
}

impl RpmSensorTrait for RpmSensorSimulator {
    fn poll(&mut self) -> u16 {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.rpm = (self.rpm + 2.0 * rng.random::<f32>() - 1.0).clamp(600.0, 3600.0);
        self.rpm as u16
    }
}
