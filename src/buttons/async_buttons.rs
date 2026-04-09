//! Code for async buttons

#[enum_dispatch::enum_dispatch]
pub trait AsyncButtonInputConfigTrait {
    /// Wait for the next input event
    fn build(&self) -> Result<ButtonInput, String>;
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[enum_dispatch::enum_dispatch(AsyncButtonInputConfigTrait)]
pub enum ButtonInputConfig {
    Simulator(DummyButtonConfig),
    #[cfg(feature = "gpio")]
    Gpio(GpioButtonConfig),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DummyButtonConfig {}

impl AsyncButtonInputConfigTrait for DummyButtonConfig {
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
impl AsyncButtonInputConfigTrait for GpioButtonConfig {
    fn build(&self) -> Result<ButtonInput, String> {
        let req = gpiocdev::Request::builder()
            .on_chip(&self.chip)
            .with_line(self.line)
            .as_input()
            .with_edge_detection(gpiocdev::line::EdgeDetection::BothEdges)
            .request()
            .map_err(|e| e.to_string())?;
        let areq = gpiocdev::tokio::AsyncRequest::new(req);
        Ok(ButtonInput::Gpio(GpioButton { req: areq }))
    }
}

/// Used to implement a future that never returns
pub struct Never<T>(std::marker::PhantomData<T>);

impl<T> Never<T> {
    /// Construct a new Self
    pub fn new() -> Self {
        Never(std::marker::PhantomData)
    }
}

impl<T> std::future::Future for Never<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Pending
    }
}

#[enum_dispatch::enum_dispatch]
pub trait AsyncButtonInputTrait {
    /// Wait for the next input event
    async fn next(&mut self) -> Result<ButtonEvent, String>;
}

#[derive(Debug)]
#[enum_dispatch::enum_dispatch(AsyncButtonInputTrait)]
pub enum ButtonInput {
    Simulator(DummyButton),
    #[cfg(feature = "gpio")]
    Gpio(GpioButton),
}

pub enum ButtonEvent {
    Rising,
    Falling,
}

#[derive(Debug)]
#[cfg(feature = "gpio")]
pub struct GpioButton {
    req: gpiocdev::tokio::AsyncRequest,
}

#[cfg(feature = "gpio")]
impl AsyncButtonInputTrait for GpioButton {
    async fn next(&mut self) -> Result<ButtonEvent, String> {
        let a = self
            .req
            .read_edge_event()
            .await
            .map_err(|e| e.to_string())?;
        if a.kind == gpiocdev::line::EdgeKind::Rising {
            Ok(ButtonEvent::Rising)
        } else {
            Ok(ButtonEvent::Falling)
        }
    }
}

#[derive(Debug)]
pub struct DummyButton {}

impl AsyncButtonInputTrait for DummyButton {
    async fn next(&mut self) -> Result<ButtonEvent, String> {
        let n = Never::new();
        n.await
    }
}
