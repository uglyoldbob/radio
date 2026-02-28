//! Code for the wireless settings page

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

/// The wireless config page
#[derive(Clone, Copy)]
pub struct Config {}

impl Config {
    /// construct a new Self
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(feature = "wifi")]
    /// Creates a qr code for a wifi network
    fn make_wifi_qr(&self, wifi_name: &String, wifi_password: &String) -> Vec<u8> {
        let a = format!("WIFI:S:{};T:WPA;P:{};H:false;;", wifi_name, wifi_password);
        a.as_bytes().to_vec()
    }

    #[cfg(feature = "wifi")]
    /// update the displayed qr code for the user to be able to scan
    fn update_qr_code(&mut self, ctx: &egui::Context, common: &mut CommonWindowProperties) {
        if let uobradio_comms::wireless::WifiConfig::Disabled = common.settings.wifi_config.config {
            common.vsettings.wireless.wifi_texture.take();
        } else {
            let mut wifi = |wifi: &(String, Option<String>)| {
                if let Some(wp) = &wifi.1 {
                    let contents = self.make_wifi_qr(&wifi.0, wp);
                    let code = qrcode::QrCode::new(contents).unwrap();
                    let image = code.render::<image::Rgb<u8>>().build();
                    let img: uobradio_comms::video::PixelImage<uobradio_comms::video::RgbPixel> =
                        image.into();
                    let cimg: egui::ColorImage = img.into();
                    common.vsettings.wireless.wifi_texture =
                        Some(ctx.load_texture("qrcode", cimg, egui::TextureOptions::LINEAR));
                }
            };
            match &common.wifi_details {
                uobradio_comms::Pollable::Idle { last_known } => {
                    if let Some(known) = last_known {
                        wifi(known)
                    }
                }
                uobradio_comms::Pollable::Waiting {
                    last_known,
                    waiting_since: _,
                } => {
                    if let Some(known) = last_known {
                        wifi(known)
                    }
                }
                uobradio_comms::Pollable::Value { v } => wifi(v),
            }
        }
    }
}

impl SubwindowTrait for Config {
    fn process_packet(
        &mut self,
        _settings: &mut uobradio_comms::NonvolatileSettings,
        vsettings: &mut uobradio_comms::VolatileSettings,
        packet: &uobradio_comms::MessageToApp,
    ) {
        match packet {
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageToApp::ConnectedToWifiNetwork {
                ssid: _,
                password: _,
            } => {
                vsettings.wireless.wifi_state =
                    uobradio_comms::wireless::WifiConnectStage::Connected;
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageToApp::FailedToConnectToWifiNetwork { ssid: _ } => {
                vsettings.wireless.wifi_state =
                    uobradio_comms::wireless::WifiConnectStage::FailedConnection;
            }
            _ => {}
        }
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
            egui::RichText::new(format!("{}\n{}", "📱", "Wireless"))
                .size(16.0)
                .color(text_color),
        )
        .fill(button_color)
        .min_size(egui::vec2(70.0, 70.0))
        .corner_radius(12.0);

        ui.add(button).clicked()
    }

    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        let _ = common
            .radio
            .send_packet(uobradio_comms::MessageFromApp::RequestSettings);
        #[cfg(feature = "wifi")]
        {
            common.wifi_details.poll_action(|| {
                service::log::info!("Polling wifi details");
                let a = common
                    .radio
                    .send_packet(uobradio_comms::MessageFromApp::GetWifiDetails);
                service::log::info!("Send packet is {:?}", a);
            });
            log::info!("WIFI IS {:?}", common.wifi_details);
        }
        #[cfg(feature = "wifi")]
        self.update_qr_code(ctx, common);

        egui::SidePanel::left("Settings tabs")
            .resizable(false)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(super::BG_PRIMARY)
                    .inner_margin(10.0)
                    .outer_margin(0.0),
            )
            .show(ctx, |ui| {
                {
                    let active = common.vsettings.wireless.submenu
                        == uobradio_comms::wireless::Submenu::Normal;
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
                        egui::RichText::new(format!("{}\n{}", "G", "General"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.wireless.submenu =
                            uobradio_comms::wireless::Submenu::Normal;
                    }
                }
                #[cfg(feature = "bluetooth")]
                {
                    let active = ccommon.vsettings.wifi.submenu
                        == uobradio_comms::wireless::Submenu::Bluetooth;
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
                        egui::RichText::new(format!("{}\n{}", "B", "Bluetooth"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.wireless.submenu =
                            uobradio_comms::wireless::Submenu::Bluetooth;
                    }
                }
                #[cfg(feature = "wifi")]
                {
                    let active = common.vsettings.wireless.submenu
                        == uobradio_comms::wireless::Submenu::Wifi;
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
                        egui::RichText::new(format!("{}\n{}", "W", "Wifi"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.wireless.submenu = uobradio_comms::wireless::Submenu::Wifi;
                    }
                }
                #[cfg(feature = "wifi")]
                {
                    let active = common.vsettings.wireless.submenu
                        == uobradio_comms::wireless::Submenu::ShareWifi;
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
                        egui::RichText::new(format!("{}\n{}", "S", "Share Wifi"))
                            .size(16.0)
                            .color(text_color),
                    )
                    .fill(button_color)
                    .min_size(egui::vec2(70.0, 70.0))
                    .corner_radius(12.0);

                    if ui.add(button).clicked() {
                        common.vsettings.wireless.submenu =
                            uobradio_comms::wireless::Submenu::ShareWifi;
                    }
                }
            });

        match &common.vsettings.wireless.submenu {
            uobradio_comms::wireless::Submenu::Normal => {
                egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                #[cfg(feature = "bluetooth")]
                {
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
                }
                #[cfg(feature = "wifi")]
                {
                    ui.label("This is the wifi page".to_string());
                    let mut save = false;
                    let mut reconnect = false;
                    common.known_networks.poll_action(|| {
                        let _ = common.radio.send_packet(
                            uobradio_comms::MessageFromApp::ListAllKnownWifiNetworks,
                        );
                    });
                    ui.label("Wifi mode");
                    {
                        if ui
                            .add(egui::SelectableLabel::new(
                                common.settings.wifi_config.config
                                    == uobradio_comms::wireless::WifiConfig::Disabled,
                                "Disabled",
                            ))
                            .clicked()
                        {
                            save = true;
                            reconnect = true;
                            common.settings.wifi_config.config =
                                uobradio_comms::wireless::WifiConfig::Disabled;
                        }
                        if ui
                            .add(egui::SelectableLabel::new(
                                common.settings.wifi_config.config
                                    == uobradio_comms::wireless::WifiConfig::Hotspot,
                                "Hotspot",
                            ))
                            .clicked()
                        {
                            save = true;
                            reconnect = true;
                            common.settings.wifi_config.config =
                                uobradio_comms::wireless::WifiConfig::Hotspot;
                        }
                        if ui
                            .add(egui::SelectableLabel::new(
                                common.settings.wifi_config.config
                                    == uobradio_comms::wireless::WifiConfig::Ready,
                                "Regular network",
                            ))
                            .clicked()
                        {
                            save = true;
                            reconnect = true;
                            common.settings.wifi_config.config =
                                uobradio_comms::wireless::WifiConfig::Ready;
                        }
                        if let Some((wn, _wp)) = &common.wifi_details.value() {
                            ui.label(format!("Connected to wifi network {}", wn));
                        }
                    }
                    common.vsettings.wireless.show_keyboard = false;
                    let mut new_wifi_state = None;
                    match &mut common.vsettings.wireless.wifi_state {
                        uobradio_comms::wireless::WifiConnectStage::PasswordPrompt(w, pw) => {
                            common.vsettings.wireless.show_keyboard = true;
                            let t = egui::RichText::new("Wifi password...")
                                .size(16.0)
                                .color(super::TEXT_SECONDARY);
                            ui.label(t);
                            let edit = egui::text_edit::TextEdit::singleline(pw).password(true);
                            ui.add(edit).request_focus();
                            let button = egui::Button::new(
                                egui::RichText::new("Connect")
                                    .size(16.0)
                                    .color(super::TEXT_SECONDARY),
                            )
                            .fill(super::BG_SECONDARY)
                            .min_size(egui::vec2(70.0, 70.0))
                            .corner_radius(12.0);

                            if ui.add(button).clicked() {
                                new_wifi_state = Some(uobradio_comms::wireless::WifiConnectStage::Connecting);
                                let _ = common.radio.send_packet(
                                    uobradio_comms::MessageFromApp::ConnectToNetwork {
                                        network: w.clone(),
                                        password: Some(pw.to_string()),
                                    },
                                );
                            }
                        }
                        uobradio_comms::wireless::WifiConnectStage::Connecting => {
                            let t = egui::RichText::new("Connecting to wifi network...")
                                .size(16.0)
                                .color(super::TEXT_SECONDARY);
                            ui.label(t);
                        }
                        uobradio_comms::wireless::WifiConnectStage::Connected => {
                            let t = egui::RichText::new("Connected to wifi network...")
                                .size(16.0)
                                .color(super::TEXT_SECONDARY);
                            ui.label(t);
                            let button = egui::Button::new(
                                egui::RichText::new("OK")
                                    .size(16.0)
                                    .color(super::TEXT_SECONDARY),
                            )
                            .fill(super::BG_SECONDARY)
                            .min_size(egui::vec2(70.0, 70.0))
                            .corner_radius(12.0);

                            if ui.add(button).clicked() {
                                common.vsettings.wireless.wifi_state =
                                    uobradio_comms::wireless::WifiConnectStage::Idle;
                            }
                        }
                        uobradio_comms::wireless::WifiConnectStage::FailedConnection => {
                            let t = egui::RichText::new("Failed to connect to wifi network...")
                                .size(16.0)
                                .color(super::TEXT_SECONDARY);
                            ui.label(t);
                            let button = egui::Button::new(
                                egui::RichText::new("OK")
                                    .size(16.0)
                                    .color(super::TEXT_SECONDARY),
                            )
                            .fill(super::BG_SECONDARY)
                            .min_size(egui::vec2(70.0, 70.0))
                            .corner_radius(12.0);

                            if ui.add(button).clicked() {
                                common.vsettings.wireless.wifi_state =
                                    uobradio_comms::wireless::WifiConnectStage::Idle;
                            }
                        }
                        uobradio_comms::wireless::WifiConnectStage::Idle => {
                            ui.label("Saved wifi networks");
                            if let Some(wifi_nets) = common.known_networks.value() {
                                for (i, w) in wifi_nets.iter().enumerate() {
                                    ui.label(format!(" * {}: {}", i, w));
                                    if ui.button("Forget").clicked() {
                                        let _ = common.radio.send_packet(
                                            uobradio_comms::MessageFromApp::ForgetWifiNetwork(w.clone()),
                                        );
                                    }
                                    if ui.button("Connect").clicked() {
                                        let _ = common.radio.send_packet(
                                            uobradio_comms::MessageFromApp::ConnectToSavedWifiNetwork(w.clone()),
                                        );
                                    }
                                }
                            }
                            let mut scan = || {
                                let button = egui::Button::new(
                                    egui::RichText::new("Scan for networks")
                                        .size(16.0)
                                        .color(super::TEXT_SECONDARY),
                                )
                                .fill(super::BG_SECONDARY)
                                .min_size(egui::vec2(70.0, 70.0))
                                .corner_radius(12.0);

                                if ui.add(button).clicked() {
                                    common.available_networks.poll_action(|| {
                                        let _ = common.radio.send_packet(
                                            uobradio_comms::MessageFromApp::ScanForWifiNetworks,
                                        );
                                    });
                                }
                                if let Some(asdf) = common.available_networks.value() {
                                    for (i, w) in asdf.iter().enumerate() {
                                        if ui.button(format!("Wifi network {i} {}", w.ssid)).clicked() {
                                            common.vsettings.wireless.wifi_state = uobradio_comms::wireless::WifiConnectStage::PasswordPrompt(w.clone(), String::new());
                                        }
                                    }
                                }
                            };
                            match &common.settings.wifi_config.config {
                                uobradio_comms::wireless::WifiConfig::RegularNetwork => {
                                    scan();
                                }
                                uobradio_comms::wireless::WifiConfig::Ready => {
                                    scan();
                                }
                                _ => {}
                            }
                            let mut hotspot = common.settings.hotspot_enabled.is_some();
                            if ui.checkbox(&mut hotspot, "Configure hotspot").changed() {
                                if hotspot {
                                    common.settings.hotspot_enabled = Some((
                                        "UobRadio Hotspot".to_string(),
                                        "qwertyuiop".to_string(),
                                    ));
                                } else {
                                    common.settings.hotspot_enabled = None;
                                }
                                reconnect = true;
                                save = true;
                            }
                            if let Some(hs) = &mut common.settings.hotspot_enabled {
                                ui.label("Hotspot name");
                                if ui.text_edit_singleline(&mut hs.0).changed() {
                                    reconnect = true;
                                    save = true;
                                }
                                ui.label("Hotspot password");
                                if ui.text_edit_singleline(&mut hs.1).changed() {
                                    reconnect = true;
                                    save = true;
                                }
                            }
                            if save {
                                log::info!("Sending new settings: {:?}", common.settings);
                                let _ = common.radio.send_packet(
                                    uobradio_comms::MessageFromApp::NewSettings {
                                        settings: common.settings.clone(),
                                        wifi_reconnect: reconnect,
                                    },
                                );
                            }
                        }
                    }
                    if let Some(state) = new_wifi_state {
                        common.vsettings.wireless.wifi_state = state;
                    }
                }
                if common.vsettings.wireless.show_keyboard {
                    egui::TopBottomPanel::bottom("KBD").show(ctx, |ui| {
                        ui.set_min_width(ui.available_width());
                        common.keyboard.show(ui);
                    });
                }
            });
        });
            }
            #[cfg(feature = "bluetooth")]
            uobradio_comms::wireless::Submenu::Bluetooth => {}
            #[cfg(feature = "wifi")]
            uobradio_comms::wireless::Submenu::Wifi => {}
            #[cfg(feature = "wifi")]
            uobradio_comms::wireless::Submenu::ShareWifi => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(t) = &common.vsettings.wireless.wifi_texture {
                        let size = ui.available_size();
                        let isize = t.size()[1];
                        let zoom = isize as f32 / size.y;
                        let dsize = t.size_vec2() / zoom;
                        ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                            id: t.id(),
                            size: dsize,
                        }));
                    }
                });
            }
        }
        None
    }
}
