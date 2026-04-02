//! Sensor code for the radio

/// The trait for gps sensors
#[enum_dispatch::enum_dispatch]
pub trait GpsSensorTrait {

}

/// The 3d orientation for an inclinometer
pub struct InclinometerOrientation {
    /// x axis - left right for the vehicle
    x: f32,
    /// y axis - forwards backwards for the vehicle
    y: f32,
    /// z axis - vertical for the vehicle
    z: f32,
}

/// The trait for inclinometer sensors
#[enum_dispatch::enum_dispatch]
pub trait InclinometerSensorTrait {
    /// Poll the sensor for the current orientation
    fn poll(&mut self) -> InclinometerOrientation;
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
            Self::Celsius(c) => {
                c * 9.0/5.0 + 32.0
            }
        }
    }

    /// Get the temperature in fahrenheit
    pub fn celsius(&self) -> f32 {
        match self {
            Self::Fahrenheit(f) => {
                (f - 32.0) * 5.0/9.0
            }
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
    z: f32,
}

impl InclinometerSensorTrait for InclinometerSimulator {
    fn poll(&mut self) -> InclinometerOrientation {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.x += rng.random::<f32>();
        self.y += rng.random::<f32>();
        self.z += rng.random::<f32>();
        self.x = self.x.clamp(-45.0, 45.0);
        self.y = self.y.clamp(-45.0, 45.0);
        self.z = self.z.clamp(-45.0, 45.0);
        InclinometerOrientation { x: self.x, y: self.y, z: self.z }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TemperatureSimulator {
    temp: f32,
}

impl Default for TemperatureSimulator {
    fn default() -> Self {
        Self {
            temp: 72.3,
        }
    }
}

impl TemperatureSensorTrait for TemperatureSimulator {
    fn poll(&mut self) -> Temperature {
        use rand::RngExt;
        let mut rng = rand::rng();
        self.temp = (self.temp + rng.random::<f32>()).clamp(-5.0, 110.0);
        Temperature::Fahrenheit(self.temp)
    }
}