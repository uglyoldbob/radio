use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

#[derive(Clone, Copy)]
pub struct BluetoothConfig {}

impl BluetoothConfig {
    pub fn new() -> Self {
        Self {}
    }
}

impl SubwindowTrait for BluetoothConfig {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        _vsettings: &mut uobradio_comms::VolatileSettings,
        _packet: &uobradio_comms::MessageToApp,
    ) {
    }

    fn card(&self, active: bool, ui: &mut egui::Ui) -> bool {
        let button_color = if active {
            super::ACCENT_PRIMARY
        } else {
            super::BG_SECONDARY
        };
        let text_color = if active {
            egui::Color32::WHITE
        } else {
            super::TEXT_SECONDARY
        };

        let button = egui::Button::new(
            egui::RichText::new(format!("{}\n{}", "📱", "Bluetooth"))
                .size(16.0)
                .color(text_color),
        )
        .fill(button_color)
        .min_size(egui::vec2(140.0, 70.0))
        .corner_radius(12.0);

        ui.add(button).clicked()
    }

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
