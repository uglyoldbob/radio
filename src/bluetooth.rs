use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

pub struct BluetoothConfig {}

impl BluetoothConfig {
    pub fn new() -> Self {
        Self {}
    }
}

impl SubwindowTrait for BluetoothConfig {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Future expansion here for bluetooth settings");
            if ui.button("Enable discovery").clicked() {
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::SetBluetoothDiscovery(true));
            }
            if ui.button("Disable discovery").clicked() {
                let _ = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::SetBluetoothDiscovery(false));
            }
        });
        None
    }
}
