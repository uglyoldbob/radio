//! Sensor code for the radio

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

    /// Get the temperature in celsius
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
    fn poll(&mut self) -> Result<Temperature, String>;
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
    Iio(IioTemperatureSensor),
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
pub struct IioTemperatureSensor {
    device: usize,
    attribute: String,
    fahrenheit: bool,
}

impl TemperatureSensorTrait for IioTemperatureSensor {
    fn poll(&mut self) -> Result<Temperature, String> {
        let context: industrial_io::Context = industrial_io::context::Context::new().map_err(|e|e.to_string())?;
        let device = context.get_device(self.device).map_err(|e|e.to_string())?;
        let a = device.attr_read_float(&self.attribute).map_err(|e|e.to_string())? as f32;
        if self.fahrenheit {
            Ok(Temperature::Fahrenheit(a))
        } else {
            Ok(Temperature::Celsius(a))
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioPressureSensor {
    device: usize,
    attribute: String,
}

impl PressureSensorTrait for IioPressureSensor {
    fn poll(&mut self) -> Result<f32, String> {
        let context: industrial_io::Context = industrial_io::context::Context::new().map_err(|e|e.to_string())?;
        let device = context.get_device(self.device).map_err(|e|e.to_string())?;
        let a = device.attr_read_float(&self.attribute).map_err(|e|e.to_string())? as f32;
        Ok(a)
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
    fn poll(&mut self) -> Result<Temperature, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.temp = (self.temp + 2.0 * rng.random::<f32>() - 1.0).clamp(-5.0, 110.0);
        Ok(Temperature::Fahrenheit(self.temp))
    }
}

#[enum_dispatch::enum_dispatch]
pub trait PressureSensorTrait {
    /// Poll and return the pressure in psi
    fn poll(&mut self) -> Result<f32, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(PressureSensorTrait)]
pub enum PressureSensor {
    Simulator(PressureSensorSimulator),
    Iio(IioPressureSensor),
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
    fn poll(&mut self) -> Result<f32, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.pressure = (self.pressure + 2.0 * rng.random::<f32>() - 1.0).clamp(0.0, 100.0);
        Ok(self.pressure)
    }
}

#[enum_dispatch::enum_dispatch]
pub trait BoolSensorTrait {
    /// Poll and return the bool value of the gpio
    fn poll(&mut self) -> Result<bool, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(BoolSensorTrait)]
pub enum BoolSensor {
    Simulator(BoolSensorSimulator),
    GpioSensor(GpioSensor),
}

impl Default for BoolSensor {
    fn default() -> Self {
        Self::Simulator(BoolSensorSimulator::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GpioSensor {
    chip: String,
    line: u32,
}

impl BoolSensorTrait for GpioSensor {
    fn poll(&mut self) -> Result<bool, String> {
        Ok(gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_input()
            .request()
            .map_err(|e| e.to_string())?
            .value(self.line)
            .map_err(|e| e.to_string())?
            == gpiocdev::line::Value::Active)
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BoolSensorSimulator {}

impl BoolSensorTrait for BoolSensorSimulator {
    fn poll(&mut self) -> Result<bool, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        Ok(rng.random())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait VoltageSensorTrait {
    /// Poll and return the voltage in volts
    fn poll(&mut self) -> Result<f32, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(VoltageSensorTrait)]
pub enum VoltageSensor {
    Simulator(VoltageSensorSimulator),
    Iio(IioVoltageSensor),
}

impl Default for VoltageSensor {
    fn default() -> Self {
        Self::Simulator(VoltageSensorSimulator::default())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioVoltageSensor {
    device: usize,
    attribute: String,
}

impl VoltageSensorTrait for IioVoltageSensor {
    fn poll(&mut self) -> Result<f32, String> {
        let context: industrial_io::Context = industrial_io::context::Context::new().map_err(|e|e.to_string())?;
        let device = context.get_device(self.device).map_err(|e|e.to_string())?;
        let a = device.attr_read_float(&self.attribute).map_err(|e|e.to_string())? as f32;
        Ok(a)
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct VoltageSensorSimulator {
    volts: f32,
}

impl VoltageSensorTrait for VoltageSensorSimulator {
    fn poll(&mut self) -> Result<f32, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.volts = (self.volts + 2.0 * rng.random::<f32>() - 1.0).clamp(10.0, 15.0);
        Ok(self.volts)
    }
}

#[enum_dispatch::enum_dispatch]
pub trait RpmSensorTrait {
    /// Poll and return the rpm
    fn poll(&mut self) -> Result<u16, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(RpmSensorTrait)]
pub enum RpmSensor {
    Simulator(RpmSensorSimulator),
    Iio(IioRpmSensor),
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
    fn poll(&mut self) -> Result<u16, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.rpm = (self.rpm + 2.0 * rng.random::<f32>() - 1.0).clamp(600.0, 3600.0);
        Ok(self.rpm as u16)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioRpmSensor {
    device: usize,
    attribute: String,
}

impl RpmSensorTrait for IioRpmSensor {
    fn poll(&mut self) -> Result<u16, String> {
        let context: industrial_io::Context = industrial_io::context::Context::new().map_err(|e|e.to_string())?;
        let device = context.get_device(self.device).map_err(|e|e.to_string())?;
        let a = device.attr_read_int(&self.attribute).map_err(|e|e.to_string())? as u16;
        Ok(a)
    }
}