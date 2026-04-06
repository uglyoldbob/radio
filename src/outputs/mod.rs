//! Code for handling the various outputs of the radio system

/// The boolean output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolOutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: bool) -> Result<(), String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(BoolOutputTrait)]
pub enum BoolOutput {
    Dummy(DummyOutput),
    Gpio(GpioOutput),
}

impl Default for BoolOutput {
    fn default() -> Self {
        Self::Dummy(DummyOutput {  })
    }
}

/// An output that goes nowhere
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DummyOutput {}

impl BoolOutputTrait for DummyOutput {
    fn output(&mut self, val: bool) -> Result<(), String> {
        Ok(())
    }
}

/// An output that writes to a gpio line
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GpioOutput {
    chip: String,
    line: u32,
}

impl BoolOutputTrait for GpioOutput {
    fn output(&mut self, val: bool) -> Result<(), String> {
        let mut a = gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_output(if val { gpiocdev::line::Value::Active } else { gpiocdev::line::Value::Inactive })
            .request();
        Ok(())
    }
}

/// The f32 output trait
#[enum_dispatch::enum_dispatch]
pub trait F32OutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: f32) -> Result<(), String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(F32OutputTrait)]
pub enum F32Output {
    Dummy(DummyOutput),
    Iio(IioOutput),
}

impl Default for F32Output {
    fn default() -> Self {
        Self::Dummy(DummyOutput {  })
    }
}


impl F32OutputTrait for DummyOutput {
    fn output(&mut self, val: f32) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IioOutput {
    device: usize,
    attribute: String,
}

impl F32OutputTrait for IioOutput {
    fn output(&mut self, val: f32) -> Result<(), String> {
        let context: industrial_io::Context = industrial_io::context::Context::new().map_err(|e|e.to_string())?;
        let device = context.get_device(self.device).map_err(|e|e.to_string())?;
        device.attr_write_float(&self.attribute, val as f64).map_err(|e| e.to_string())
    }
}