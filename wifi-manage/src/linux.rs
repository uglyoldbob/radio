use std::{io::BufRead, sync::{Arc, Mutex}};

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

    fn scan_for_networks(&self) -> Vec<crate::WifiNetwork> {
        let mut r = Vec::new();
        let command = vec![
            "-t".to_string(),
            "device".to_string(),
            "wifi".to_string(),
            "list".to_string(),
        ];
        let output = std::process::Command::new("nmcli").args(&command).output();
        if let Ok(o) = output {
            let ou = o.stdout;
            let a: &[u8] = &ou;
            let br = std::io::BufReader::new(a);
            for l in br.lines() {
                if let Ok(l) = l {
                    let parts : Vec<String> = l.split(':').map(|a| a.to_string()).collect();
                    let mut actual_parts = Vec::new();
                    let mut this_part = String::new();
                    for p in &parts {
                        let continued = p.ends_with('\\');
                        if continued {
                            let mut m = p.clone();
                            m.pop();
                            m.push(':');
                            this_part.push_str(&m);
                        }
                        else {
                            this_part.push_str(p);
                        }
                        if !continued {
                            actual_parts.push(this_part.clone());
                            this_part.clear();
                        }
                    }

                    let speed_str = actual_parts[5].clone();
                    let speed = if speed_str.contains("Mbit/s") {
                        let mut a = speed_str.split(' ');
                        let f = a.next().unwrap();
                        f.parse::<u32>().unwrap() * 1000000
                    } else {
                        todo!()
                    };
                    let wifi = super::WifiNetwork {
                        bssid: actual_parts[1].clone(),
                        name: actual_parts[2].clone(),
                        channel: actual_parts[4].parse::<u16>().unwrap(),
                        speed,
                        signal: actual_parts[6].parse::<u8>().unwrap(),
                        security: actual_parts[8].clone(),
                    };
                    r.push(wifi);
                }
            }
        }
        r
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
            let wnm = WifiNetworkNmcli { name: con_name.to_string(), ssid: ssid.to_string(), password: password.to_string(),_net: self.clone(), };
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
            let cnm = WifiNetworkNmcli { name: con_name.to_string(), ssid: ssid.to_string(), password: password.to_string(), _net: self.clone(), };
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
    ssid: String,
    password: String,
    _net: Arc<Nmcli>,
}

impl super::WifiHotspotTrait for WifiNetworkNmcli {
    fn password(&self) -> String {
        self.password.clone()
    }

    fn ssid(&self) -> String {
        self.ssid.clone()
    }
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
