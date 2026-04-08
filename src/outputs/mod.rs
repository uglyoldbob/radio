//! Code for handling the various outputs of the radio system

/// The boolean output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolOutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: bool) -> Result<(), String>;
    /// Get the last written output
    fn last_output(&self) -> bool;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(BoolOutputTrait)]
pub enum BoolOutput {
    Dummy(DummyOutput),
    #[cfg(feature = "gpio")]
    Gpio(GpioOutput),
}

impl Default for BoolOutput {
    fn default() -> Self {
        Self::Dummy(DummyOutput { val: false })
    }
}

/// An output that goes nowhere
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DummyOutput {
    val: bool,
}

impl BoolOutputTrait for DummyOutput {
    fn output(&mut self, val: bool) -> Result<(), String> {
        self.val = val;
        Ok(())
    }

    fn last_output(&self) -> bool {
        self.val
    }
}

/// An output that writes to a gpio line
#[cfg(feature = "gpio")]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GpioOutput {
    chip: String,
    line: u32,
    output: bool,
}

#[cfg(feature = "gpio")]
impl BoolOutputTrait for GpioOutput {
    fn output(&mut self, val: bool) -> Result<(), String> {
        let mut a = gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_output(if val {
                gpiocdev::line::Value::Active
            } else {
                gpiocdev::line::Value::Inactive
            })
            .request();
        self.output = val;
        Ok(())
    }

    fn last_output(&self) -> bool {
        self.output
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
    #[cfg(feature = "iio")]
    Iio(IioOutput),
}

impl Default for F32Output {
    fn default() -> Self {
        Self::Dummy(DummyOutput { val: false })
    }
}

impl F32OutputTrait for DummyOutput {
    fn output(&mut self, val: f32) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(feature = "iio")]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IioOutput {
    device: usize,
    attribute: String,
}

#[cfg(feature = "iio")]
impl F32OutputTrait for IioOutput {
    fn output(&mut self, val: f32) -> Result<(), String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        device
            .attr_write_float(&self.attribute, val as f64)
            .map_err(|e| e.to_string())
    }
}

/// The boolean vector output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolVecOutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: &[bool]) -> Result<(), String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(BoolVecOutputTrait)]
pub enum BoolVecOutput {
    Dummy(DummyOutput),
    #[cfg(feature = "gpio")]
    Gpio(GpioVecOutput),
}

impl Default for BoolVecOutput {
    fn default() -> Self {
        Self::Dummy(DummyOutput { val: false })
    }
}

impl BoolVecOutputTrait for DummyOutput {
    fn output(&mut self, val: &[bool]) -> Result<(), String> {
        Ok(())
    }
}

/// An output that writes to a gpio line
#[cfg(feature = "gpio")]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GpioVecOutput {
    outputs: Vec<GpioOutput>,
}

#[cfg(feature = "gpio")]
impl Default for GpioVecOutput {
    fn default() -> Self {
        Self {
            outputs: Vec::new(),
        }
    }
}

#[cfg(feature = "gpio")]
impl BoolVecOutputTrait for GpioVecOutput {
    fn output(&mut self, val: &[bool]) -> Result<(), String> {
        if self.outputs.len() != val.len() {
            return Err("Wrong number of outputs".to_string());
        }
        for o in &mut self.outputs.iter_mut().zip(val) {
            o.0.output(*o.1)?;
        }
        Ok(())
    }
}
