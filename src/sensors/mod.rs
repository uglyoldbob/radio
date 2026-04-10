//! Sensor code for the radio

/// The trait for gps sensors
#[enum_dispatch::enum_dispatch]
pub trait GpsSensorTrait {}

/// The trait for inclinometer sensors
#[enum_dispatch::enum_dispatch]
pub trait InclinometerSensorConfigTrait {
    /// Build the sensor
    fn build(&self) -> Result<InclinometerSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(InclinometerSensorConfigTrait)]
pub enum InclinometerSensorConfig {
    Simulated(InclinometerSimulatorConfig),
}

impl Default for InclinometerSensorConfig {
    fn default() -> Self {
        Self::Simulated(InclinometerSimulatorConfig::default())
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct InclinometerSimulatorConfig {}

impl InclinometerSensorConfigTrait for InclinometerSimulatorConfig {
    fn build(&self) -> Result<InclinometerSensor, String> {
        Ok(InclinometerSensor::Simulated(InclinometerSimulator {
            x: 0.0,
            y: 0.0,
        }))
    }
}

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

/// The trait for temperature sensors
#[enum_dispatch::enum_dispatch]
pub trait TemperatureSensorConfigTrait {
    /// build the sensor
    fn build(&self) -> Result<TemperatureSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(TemperatureSensorConfigTrait)]
pub enum TemperatureSensorConfig {
    Simulated(TemperatureSimulatorConfig),
    #[cfg(feature = "iio")]
    Iio(IioTemperatureSensorConfig),
}

impl Default for TemperatureSensorConfig {
    fn default() -> Self {
        TemperatureSensorConfig::Simulated(TemperatureSimulatorConfig::default())
    }
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(InclinometerSensorTrait)]
pub enum InclinometerSensor {
    Simulated(InclinometerSimulator),
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(TemperatureSensorTrait)]
pub enum TemperatureSensor {
    Simulated(TemperatureSimulator),
    #[cfg(feature = "iio")]
    Iio(IioTemperatureSensor),
}

#[derive(Debug)]
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

#[cfg(feature = "iio")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioTemperatureSensorConfig {
    device: usize,
    attribute: String,
    fahrenheit: bool,
}

#[cfg(feature = "iio")]
impl TemperatureSensorConfigTrait for IioTemperatureSensorConfig {
    fn build(&self) -> Result<TemperatureSensor, String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        let a = IioTemperatureSensor {
            device,
            attribute: self.attribute.clone(),
            fahrenheit: self.fahrenheit,
        };
        Ok(TemperatureSensor::Iio(a))
    }
}

#[cfg(feature = "iio")]
#[derive(Debug)]
pub struct IioTemperatureSensor {
    device: industrial_io::Device,
    attribute: String,
    fahrenheit: bool,
}

#[cfg(feature = "iio")]
impl TemperatureSensorTrait for IioTemperatureSensor {
    fn poll(&mut self) -> Result<Temperature, String> {
        let a = self
            .device
            .attr_read_float(&self.attribute)
            .map_err(|e| e.to_string())? as f32;
        if self.fahrenheit {
            Ok(Temperature::Fahrenheit(a))
        } else {
            Ok(Temperature::Celsius(a))
        }
    }
}

#[cfg(feature = "iio")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioPressureSensorConfig {
    device: usize,
    attribute: String,
}

#[cfg(feature = "iio")]
impl PressureSensorConfigTrait for IioPressureSensorConfig {
    fn build(&self) -> Result<PressureSensor, String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        let a = IioPressureSensor {
            device,
            attribute: self.attribute.clone(),
        };
        Ok(PressureSensor::Iio(a))
    }
}

#[cfg(feature = "iio")]
#[derive(Debug)]
pub struct IioPressureSensor {
    device: industrial_io::Device,
    attribute: String,
}

#[cfg(feature = "iio")]
impl PressureSensorTrait for IioPressureSensor {
    fn poll(&mut self) -> Result<f32, String> {
        let a = self
            .device
            .attr_read_float(&self.attribute)
            .map_err(|e| e.to_string())? as f32;
        Ok(a)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TemperatureSimulatorConfig {
    temp: f32,
}

impl Default for TemperatureSimulatorConfig {
    fn default() -> Self {
        Self { temp: 72.3 }
    }
}

impl TemperatureSensorConfigTrait for TemperatureSimulatorConfig {
    fn build(&self) -> Result<TemperatureSensor, String> {
        Ok(TemperatureSensor::Simulated(TemperatureSimulator {
            temp: self.temp,
        }))
    }
}

#[derive(Debug)]
pub struct TemperatureSimulator {
    temp: f32,
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
pub trait PressureSensorConfigTrait {
    /// Build the sensor
    fn build(&self) -> Result<PressureSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(PressureSensorConfigTrait)]
pub enum PressureSensorConfig {
    Simulator(PressureSensorSimulatorConfig),
    #[cfg(feature = "iio")]
    Iio(IioPressureSensorConfig),
}

impl Default for PressureSensorConfig {
    fn default() -> Self {
        Self::Simulator(PressureSensorSimulatorConfig::default())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait PressureSensorTrait {
    /// Poll and return the pressure in psi
    fn poll(&mut self) -> Result<f32, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(PressureSensorTrait)]
pub enum PressureSensor {
    Simulator(PressureSensorSimulator),
    #[cfg(feature = "iio")]
    Iio(IioPressureSensor),
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PressureSensorSimulatorConfig {
    pressure: f32,
}

impl PressureSensorConfigTrait for PressureSensorSimulatorConfig {
    fn build(&self) -> Result<PressureSensor, String> {
        Ok(PressureSensor::Simulator(PressureSensorSimulator {
            pressure: self.pressure,
        }))
    }
}

#[derive(Debug)]
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
pub trait BoolSensorConfigTrait {
    /// Build the sensor
    fn build(&self) -> Result<BoolSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(BoolSensorConfigTrait)]
pub enum BoolSensorConfig {
    Simulator(BoolSensorSimulatorConfig),
    #[cfg(feature = "gpio")]
    GpioSensor(GpioSensorConfig),
}

impl Default for BoolSensorConfig {
    fn default() -> Self {
        Self::Simulator(BoolSensorSimulatorConfig::default())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait BoolSensorTrait {
    /// Poll and return the bool value of the gpio
    fn poll(&mut self) -> Result<bool, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(BoolSensorTrait)]
pub enum BoolSensor {
    Simulator(BoolSensorSimulator),
    #[cfg(feature = "gpio")]
    GpioSensor(GpioSensor),
}

#[cfg(feature = "gpio")]
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GpioSensorConfig {
    chip: String,
    line: u32,
}

#[cfg(feature = "gpio")]
impl BoolSensorConfigTrait for GpioSensorConfig {
    fn build(&self) -> Result<BoolSensor, String> {
        Ok(BoolSensor::GpioSensor(GpioSensor {
            req: gpiocdev::Request::builder()
                .on_chip(&self.chip)
                .with_line(self.line)
                .as_input()
                .request()
                .map_err(|e| e.to_string())?,
            line: self.line,
        }))
    }
}

#[cfg(feature = "gpio")]
#[derive(Debug)]
pub struct GpioSensor {
    req: gpiocdev::Request,
    line: u32,
}

#[cfg(feature = "gpio")]
impl BoolSensorTrait for GpioSensor {
    fn poll(&mut self) -> Result<bool, String> {
        Ok(self.req.value(self.line).map_err(|e| e.to_string())? == gpiocdev::line::Value::Active)
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BoolSensorSimulatorConfig {}

impl BoolSensorConfigTrait for BoolSensorSimulatorConfig {
    fn build(&self) -> Result<BoolSensor, String> {
        Ok(BoolSensor::Simulator(BoolSensorSimulator {}))
    }
}

#[derive(Debug)]
pub struct BoolSensorSimulator {}

impl BoolSensorTrait for BoolSensorSimulator {
    fn poll(&mut self) -> Result<bool, String> {
        use rand::RngExt;
        let mut rng = rand::rng();
        Ok(rng.random())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait VoltageSensorConfigTrait {
    /// Build the sensor
    fn build(&self) -> Result<VoltageSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(VoltageSensorConfigTrait)]
pub enum VoltageSensorConfig {
    Simulator(VoltageSensorSimulatorConfig),
    #[cfg(feature = "iio")]
    Iio(IioVoltageSensorConfig),
}

impl Default for VoltageSensorConfig {
    fn default() -> Self {
        Self::Simulator(VoltageSensorSimulatorConfig::default())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait VoltageSensorTrait {
    /// Poll and return the voltage in volts
    fn poll(&mut self) -> Result<f32, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(VoltageSensorTrait)]
pub enum VoltageSensor {
    Simulator(VoltageSensorSimulator),
    #[cfg(feature = "iio")]
    Iio(IioVoltageSensor),
}

#[cfg(feature = "iio")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioVoltageSensorConfig {
    device: usize,
    attribute: String,
}

#[cfg(feature = "iio")]
impl VoltageSensorConfigTrait for IioVoltageSensorConfig {
    fn build(&self) -> Result<VoltageSensor, String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        let a = IioVoltageSensor {
            device,
            attribute: self.attribute.clone(),
        };
        Ok(VoltageSensor::Iio(a))
    }
}

#[cfg(feature = "iio")]
#[derive(Debug)]
pub struct IioVoltageSensor {
    device: industrial_io::Device,
    attribute: String,
}

#[cfg(feature = "iio")]
impl VoltageSensorTrait for IioVoltageSensor {
    fn poll(&mut self) -> Result<f32, String> {
        let a = self
            .device
            .attr_read_float(&self.attribute)
            .map_err(|e| e.to_string())? as f32;
        Ok(a)
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct VoltageSensorSimulatorConfig {
    volts: f32,
}

impl VoltageSensorConfigTrait for VoltageSensorSimulatorConfig {
    fn build(&self) -> Result<VoltageSensor, String> {
        Ok(VoltageSensor::Simulator(VoltageSensorSimulator {
            volts: self.volts,
        }))
    }
}

#[derive(Debug)]
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
pub trait RpmSensorConfigTrait {
    /// Build the sensor
    fn build(&self) -> Result<RpmSensor, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(RpmSensorConfigTrait)]
pub enum RpmSensorConfig {
    Simulator(RpmSensorSimulatorConfig),
    #[cfg(feature = "iio")]
    Iio(IioRpmSensorConfig),
}

impl Default for RpmSensorConfig {
    fn default() -> Self {
        Self::Simulator(RpmSensorSimulatorConfig::default())
    }
}

#[enum_dispatch::enum_dispatch]
pub trait RpmSensorTrait {
    /// Poll and return the rpm
    fn poll(&mut self) -> Result<u16, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(RpmSensorTrait)]
pub enum RpmSensor {
    Simulator(RpmSensorSimulator),
    #[cfg(feature = "iio")]
    Iio(IioRpmSensor),
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RpmSensorSimulatorConfig {
    rpm: f32,
}

impl RpmSensorConfigTrait for RpmSensorSimulatorConfig {
    fn build(&self) -> Result<RpmSensor, String> {
        Ok(RpmSensor::Simulator(RpmSensorSimulator { rpm: self.rpm }))
    }
}

#[derive(Debug)]
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

#[cfg(feature = "iio")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IioRpmSensorConfig {
    device: usize,
    attribute: String,
}

#[cfg(feature = "iio")]
impl RpmSensorConfigTrait for IioRpmSensorConfig {
    fn build(&self) -> Result<RpmSensor, String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        let a = IioRpmSensor {
            device,
            attribute: self.attribute.clone(),
        };
        Ok(RpmSensor::Iio(a))
    }
}

#[cfg(feature = "iio")]
#[derive(Debug)]
pub struct IioRpmSensor {
    device: industrial_io::Device,
    attribute: String,
}

#[cfg(feature = "iio")]
impl RpmSensorTrait for IioRpmSensor {
    fn poll(&mut self) -> Result<u16, String> {
        let a = self
            .device
            .attr_read_int(&self.attribute)
            .map_err(|e| e.to_string())? as u16;
        Ok(a)
    }
}
