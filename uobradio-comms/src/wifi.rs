//! Wifi page code and structs

/// The mode of operation for wifi
#[derive(Debug)]
pub enum WifiMode {
    /// The local wifi devices creates a hotspot
    Hotspot {
        /// The ssid of the network
        ssid: String,
        /// The password of the network if applicable
        password: Option<String>,
    },
    /// The local wifi adapter connects to an existing wifi network
    RegularNetwork {
        /// The ssid of the network
        ssid: String,
        /// The password of the network if applicable
        password: Option<String>,
    },
}

/// Specifies what mode the wifi card should be in
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub enum WifiConfig {
    /// Hotspot
    Hotspot,
    /// The local wifi adapter connects to an existing wifi network
    RegularNetwork,
    /// The wifi card should be ready
    Ready,
    /// The wifi card should be disabled
    #[default]
    Disabled,
}

#[derive(Clone, Copy, Default)]
/// The stage of connecting to a wifi network
pub enum WifiConnectStage {
    /// Prompt the user for the password
    PasswordPrompt,
    /// Indicate connecting to the network
    Connecting,
    /// The wifi is connected
    Connected,
    /// The wifi failed to connect
    FailedConnection,
    /// Doing nothing
    #[default]
    Idle,
}

/// The volatile wifi settings
#[derive(Default)]
pub struct Settings {
    /// For the qr code
    pub wifi_texture: Option<egui::TextureHandle>,
    /// The password storage for wifi connection
    pub wifi_password: String,
    /// Connection state for the indicated wifi network (wifi_new_connect)
    pub wifi_state: WifiConnectStage,
    /// The list of known networks by ssid
    pub known_networks: Vec<String>,
}

/// The non-volatile wifi settings
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct NvSettings {
    /// The wifi configuration that is currently active
    pub config: WifiConfig,
}