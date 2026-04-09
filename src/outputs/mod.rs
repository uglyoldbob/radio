//! Code for handling the various outputs of the radio system

/// The boolean output configuration trait
#[enum_dispatch::enum_dispatch]
pub trait BoolOutputConfigTrait {
    /// Build the output
    fn build(&self) -> Result<BoolOutput, String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(BoolOutputConfigTrait)]
pub enum BoolOutputConfig {
    Dummy(DummyConfigOutput),
    #[cfg(feature = "gpio")]
    Gpio(GpioOutputConfig),
}

impl Default for BoolOutputConfig {
    fn default() -> Self {
        Self::Dummy(DummyConfigOutput {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DummyConfigOutput {}

impl BoolOutputConfigTrait for DummyConfigOutput {
    fn build(&self) -> Result<BoolOutput, String> {
        Ok(BoolOutput::Dummy(DummyOutput { val: false }))
    }
}

/// The boolean output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolOutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: bool) -> Result<(), String>;
    /// Get the last written output
    fn last_output(&self) -> bool;
}

#[derive(Debug)]
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
#[derive(Debug)]
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
pub struct GpioOutputConfig {
    chip: String,
    line: u32,
    output: bool,
}

#[cfg(feature = "gpio")]
impl BoolOutputConfigTrait for GpioOutputConfig {
    fn build(&self) -> Result<BoolOutput, String> {
        let mut a = gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_output(if self.output {
                gpiocdev::line::Value::Active
            } else {
                gpiocdev::line::Value::Inactive
            })
            .request()
            .map_err(|e| e.to_string())?;
        let g = GpioOutput {
            req: a,
            line: self.line,
            output: self.output,
        };
        Ok(BoolOutput::Gpio(g))
    }
}

/// An output that writes to a gpio line
#[cfg(feature = "gpio")]
#[derive(Debug)]
pub struct GpioOutput {
    req: gpiocdev::request::Request,
    line: u32,
    output: bool,
}

#[cfg(feature = "gpio")]
impl BoolOutputTrait for GpioOutput {
    fn output(&mut self, val: bool) -> Result<(), String> {
        self.req
            .set_value(
                self.line,
                if val {
                    gpiocdev::line::Value::Active
                } else {
                    gpiocdev::line::Value::Inactive
                },
            )
            .map_err(|e| e.to_string())?;
        self.output = val;
        Ok(())
    }

    fn last_output(&self) -> bool {
        self.output
    }
}

/// The f32 output trait
#[enum_dispatch::enum_dispatch]
pub trait F32OutputConfigTrait {
    /// Build the output
    fn build(&self) -> Result<F32Output, String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(F32OutputConfigTrait)]
pub enum F32OutputConfig {
    Dummy(DummyConfigOutput),
    #[cfg(feature = "iio")]
    Iio(IioOutputConfig),
}

impl Default for F32OutputConfig {
    fn default() -> Self {
        Self::Dummy(DummyConfigOutput {})
    }
}

impl F32OutputConfigTrait for DummyConfigOutput {
    fn build(&self) -> Result<F32Output, String> {
        Ok(F32Output::Dummy(DummyOutput { val: false }))
    }
}

#[cfg(feature = "iio")]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IioOutputConfig {
    device: usize,
    attribute: String,
}

#[cfg(feature = "iio")]
impl F32OutputConfigTrait for IioOutputConfig {
    fn build(&self) -> Result<F32Output, String> {
        let context: industrial_io::Context =
            industrial_io::context::Context::new().map_err(|e| e.to_string())?;
        let device = context.get_device(self.device).map_err(|e| e.to_string())?;
        Ok(F32Output::Iio(IioOutput {
            device,
            attribute: self.attribute.clone(),
        }))
    }
}

/// The f32 output trait
#[enum_dispatch::enum_dispatch]
pub trait F32OutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: f32) -> Result<(), String>;
}

#[derive(Debug)]
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
#[derive(Debug)]
pub struct IioOutput {
    device: industrial_io::device::Device,
    attribute: String,
}

#[cfg(feature = "iio")]
impl F32OutputTrait for IioOutput {
    fn output(&mut self, val: f32) -> Result<(), String> {
        self.device
            .attr_write_float(&self.attribute, val as f64)
            .map_err(|e| e.to_string())
    }
}

/// The boolean vector output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolVecOutputConfigTrait {
    /// Build the output object
    fn build(&self) -> Result<BoolVecOutput, String>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[enum_dispatch::enum_dispatch(BoolVecOutputConfigTrait)]
pub enum BoolVecOutputConfig {
    Dummy(DummyConfigOutput),
    #[cfg(feature = "gpio")]
    Gpio(GpioVecOutputConfig),
}

impl Default for BoolVecOutputConfig {
    fn default() -> Self {
        Self::Dummy(DummyConfigOutput {})
    }
}

impl BoolVecOutputConfigTrait for DummyConfigOutput {
    fn build(&self) -> Result<BoolVecOutput, String> {
        Ok(BoolVecOutput::Dummy(DummyOutput { val: false }))
    }
}

/// An output that writes to a gpio line
#[cfg(feature = "gpio")]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GpioVecOutputConfig {
    outputs: Vec<GpioOutputConfig>,
}

#[cfg(feature = "gpio")]
impl BoolVecOutputConfigTrait for GpioVecOutputConfig {
    fn build(&self) -> Result<BoolVecOutput, String> {
        let mut outputs = Vec::new();
        for o in &self.outputs {
            let b = o.build()?;
            outputs.push(b);
        }
        Ok(BoolVecOutput::Gpio(GpioVecOutput { outputs }))
    }
}

/// The boolean vector output trait
#[enum_dispatch::enum_dispatch]
pub trait BoolVecOutputTrait {
    /// Write the output to the destination
    fn output(&mut self, val: &[bool]) -> Result<(), String>;
}

#[derive(Debug)]
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
#[derive(Debug)]
pub struct GpioVecOutput {
    outputs: Vec<BoolOutput>,
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
