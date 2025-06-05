use std::sync::{Arc, Mutex};

/// The struct for using nmcli
pub struct Nmcli {
    /// The name of the wifi interface to communicate with
    interface: String,
    /// Set to true to keep wifi hardware enabled when dropped
    pub dont_drop: Mutex<bool>,
}

impl Nmcli {
    /// Construct a new network adapter, interface must actually exist or errors will occur later on
    pub fn new(interface: String) -> Self {
        let _ = std::process::Command::new("nmcli")
            .args(&["radio", "wifi", "on"])
            .output();
        Self {
            interface,
            dont_drop: Mutex::new(false),
        }
    }
}

impl Drop for Nmcli {
    fn drop(&mut self) {
        let dd = self.dont_drop.lock().unwrap();
        if !*dd {
            let _ = std::process::Command::new("nmcli")
            .args(&["radio", "wifi", "off"])
            .output();
        }
    }
}

impl super::WifiAdapterTrait for Arc<Nmcli> {
    fn set_stay(&self) {
        let mut dd = self.dont_drop.lock().unwrap();
        *dd = true;
    }

    fn build_hotspot(&self, con_name: &str, ssid: &str, password: &str) -> Result<super::WifiHotspot, ()> {
        let command = vec![
            "device".to_string(),
            "wifi".to_string(),
            "hotspot".to_string(),
            "ifname".to_string(),
            self.interface.clone(),
            "con-name".to_string(),
            con_name.to_string(),
            "ssid".to_string(),
            ssid.to_string(),
            "password".to_string(),
            password.to_string(),
        ];

        let output = std::process::Command::new("nmcli")
            .args(&command)
            .status()
            .map(|s| s.code() == Some(0))
            .map_err(|_| ())?;
        if output {
            let wnm = WifiNetworkNmcli { name: con_name.to_string(), _net: self.clone(), };
            Ok(super::WifiHotspot::Nmcli(wnm))
        } else {
            Err(())
        }
    }

    fn connect_to_network(&self, con_name: &str, ssid: &str, password: &str) -> Result<crate::WifiConnection, ()> {
        let output = std::process::Command::new("nmcli")
            .args(&[
                "d",
                "wifi",
                "connect",
                ssid,
                "password",
                &password,
                "ifname",
                &self.interface,
            ])
            .output()
            .map_err(|_| ())?;
        if output.status.success() {
            let cnm = WifiNetworkNmcli { name: con_name.to_string(), _net: self.clone(), };
            Ok(super::WifiConnection::Nmcli(cnm))
        }
        else {
            Err(())
        }
    }

    fn show_networks(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A struct for managing a connection of a wifi network
pub struct WifiNetworkNmcli {
    name: String,
    _net: Arc<Nmcli>,
}

impl WifiNetworkNmcli {
    /// Stop serving a wireless network.
    ///
    /// **NOTE: All users connected will automatically be disconnected.**
    fn stop(&mut self) -> Result<(), ()> {
        let output = std::process::Command::new("nmcli")
            .args(&["con", "down", &self.name])
            .output()
            .map_err(|_| ())?;
        if output.status.success() {
            Ok(())
        }
        else {
            Err(())
        }
    }
}

impl Drop for WifiNetworkNmcli {
    fn drop(&mut self) {
        let _ = self.stop();
        let command = vec![
            "connection".to_string(),
            "delete".to_string(),
            self.name.clone(),
        ];
        let output = std::process::Command::new("nmcli").args(&command).spawn();
        if let Ok(mut child) = output {
            let _ = child.wait();
        }
    }
}
