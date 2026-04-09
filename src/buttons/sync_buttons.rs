//! Button code

#[enum_dispatch::enum_dispatch]
pub trait ButtonInputConfigTrait {
    /// Wait for the next input event
    fn build(&self) -> Result<ButtonInput, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(ButtonInputConfigTrait)]
pub enum ButtonInputConfig {
    Simulator(DummyButtonConfig),
    #[cfg(feature = "gpio")]
    Gpio(GpioButtonConfig),
    #[cfg(feature = "evdev")]
    Evdev(EvdevButtonConfig),
}

impl Default for ButtonInputConfig {
    fn default() -> Self {
        Self::Simulator(DummyButtonConfig {})
    }
}

#[cfg(feature = "evdev")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EvdevButtonConfig {
    dev: String,
}

#[cfg(feature = "evdev")]
impl ButtonInputConfigTrait for EvdevButtonConfig {
    fn build(&self) -> Result<ButtonInput, String> {
        let e = EvdevButton {
            dev: evdev::Device::open(&self.dev).map_err(|e| e.to_string())?,
        };
        e.dev.set_nonblocking(true);
        Ok(ButtonInput::Evdev(e))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DummyButtonConfig {}

impl ButtonInputConfigTrait for DummyButtonConfig {
    fn build(&self) -> Result<ButtonInput, String> {
        Ok(ButtonInput::Simulator(DummyButton {}))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg(feature = "gpio")]
pub struct GpioButtonConfig {
    chip: String,
    line: u32,
}

#[cfg(feature = "gpio")]
impl ButtonInputConfigTrait for GpioButtonConfig {
    fn build(&self) -> Result<ButtonInput, String> {
        let req = gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_input()
            .with_edge_detection(gpiocdev::line::EdgeDetection::BothEdges)
            .request()
            .map_err(|e| e.to_string())?;
        Ok(ButtonInput::Gpio(GpioButton { req }))
    }
}

pub struct ButtonEvents(Vec<ButtonEvent>);

impl ButtonEvents {
    /// true when the specified keycode was detected as pressed
    pub fn pressed(&self, code: u16) -> bool {
        let mut action = false;
        for e in &self.0 {
            match e {
                ButtonEvent::Pressed(c) => {
                    if *c == code {
                        action = true;
                        break;
                    }
                }
                ButtonEvent::Released(c) => {}
            }
        }
        action
    }

    /// true when the specified keycode was detected as released
    pub fn released(&self, code: u16) -> bool {
        let mut action = false;
        for e in &self.0 {
            match e {
                ButtonEvent::Pressed(c) => {}
                ButtonEvent::Released(c) => {
                    if *c == code {
                        action = true;
                        break;
                    }
                }
            }
        }
        action
    }

    /// true when any keycode was detected as pressed
    pub fn any_pressed(&self) -> bool {
        let mut action = false;
        for e in &self.0 {
            match e {
                ButtonEvent::Pressed(c) => {
                    action = true;
                    break;
                }
                ButtonEvent::Released(c) => {}
            }
        }
        action
    }

    /// true when any keycode was detected as released
    pub fn any_released(&self) -> bool {
        let mut action = false;
        for e in &self.0 {
            match e {
                ButtonEvent::Pressed(c) => {}
                ButtonEvent::Released(c) => {
                    action = true;
                    break;
                }
            }
        }
        action
    }
}

#[enum_dispatch::enum_dispatch]
pub trait ButtonInputTrait {
    /// Poll to see if the button was acted on
    fn poll(&mut self) -> Result<ButtonEvents, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(ButtonInputTrait)]
pub enum ButtonInput {
    Simulator(DummyButton),
    #[cfg(feature = "gpio")]
    Gpio(GpioButton),
    #[cfg(feature = "evdev")]
    Evdev(EvdevButton),
}

pub enum ButtonEvent {
    Released(u16),
    Pressed(u16),
}

#[derive(Debug)]
#[cfg(feature = "gpio")]
pub struct GpioButton {
    req: gpiocdev::Request,
}

#[cfg(feature = "gpio")]
impl ButtonInputTrait for GpioButton {
    fn poll(&mut self) -> Result<ButtonEvents, String> {
        if let Ok(true) = self.req.has_edge_event() {
            if let Ok(ev) = self.req.read_edge_event() {
                if ev.kind == gpiocdev::line::EdgeKind::Falling {
                    return Ok(ButtonEvents(vec![ButtonEvent::Released(0)]));
                } else {
                    return Ok(ButtonEvents(vec![ButtonEvent::Pressed(0)]));
                }
            }
        }
        Ok(ButtonEvents(Vec::new()))
    }
}

#[derive(Debug)]
pub struct DummyButton {}

impl ButtonInputTrait for DummyButton {
    fn poll(&mut self) -> Result<ButtonEvents, String> {
        Ok(ButtonEvents(Vec::new()))
    }
}

#[derive(Debug)]
#[cfg(feature = "evdev")]
pub struct EvdevButton {
    dev: evdev::Device,
}

#[cfg(feature = "evdev")]
impl ButtonInputTrait for EvdevButton {
    fn poll(&mut self) -> Result<ButtonEvents, String> {
        let mut r = Vec::new();
        for event in self.dev.fetch_events().map_err(|e| e.to_string())? {
            match event.destructure() {
                evdev::EventSummary::Key(_, keycode, 0) => {
                    r.push(ButtonEvent::Released(keycode.0));
                }
                evdev::EventSummary::Key(_, keycode, 1) => {
                    r.push(ButtonEvent::Pressed(keycode.0));
                }
                _ => {}
            }
        }
        Ok(ButtonEvents(r))
    }
}
