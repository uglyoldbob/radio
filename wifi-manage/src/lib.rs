#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This crate will eventually be a cross platform wifi managing platform. It is intended to configure wireless network connections that are only setup for the duration of the user program.

use std::sync::Arc;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
/// Return a list of all known wifi adapters on the system
pub fn get_wifi_adapters() -> Result<Vec<String>, String> {
    let mut wifis = Vec::new();
    let output = std::process::Command::new("nmcli")
        .args(&[
            "-t",
            "device",
            "status"])
        .output().map_err(|e| e.to_string())?;
    let s : String = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    for entry in s.lines() {
        if entry.contains(":wifi:") {
            let n : Vec<&str> = entry.split(':').collect();
            wifis.push(n[0].to_string());
        }
    }
    log::error!("Wifi adapter output: {:?}", s);
    Ok(wifis)
}

/// The trait for wifi hotspots
#[enum_dispatch::enum_dispatch]
pub trait WifiHotspotTrait {
}

/// A connection to an existing wifi network
pub enum WifiConnection {
    /// nmcli on linux connected to the network
    Nmcli(linux::WifiNetworkNmcli),
}

/// A hotspot provided by local hardware (also can be called an access point)
#[enum_dispatch::enum_dispatch(WifiHotspotTrait)]
pub enum WifiHotspot {
    /// nmcli on linux created the hotspot
    Nmcli(linux::WifiNetworkNmcli),
}

/// A trait for operating on a specific wifi adapter
#[enum_dispatch::enum_dispatch]
pub trait WifiAdapterTrait {
    /// Attempt to connect to an existing wifi network
    fn connect_to_network(&self, con_name: &str, ssid: &str, pw: &str) -> Result<WifiConnection, ()>;
    /// List available wifi networks
    fn show_networks(&self) -> Vec<String>;
    /// Attempt to construct a hotspot wifi network (aka access point)
    fn build_hotspot(&self, con_name: &str, ssid: &str, pw: &str) -> Result<WifiHotspot, ()>;
    /// Set the adapter to remain enabled after being dropped
    fn set_stay(&self);
}

/// Get a network adapter
pub fn get_network_adapter(name: &str) -> Result<WifiAdapter, ()> {
    #[cfg(target_os = "linux")]
    {
        let n = linux::Nmcli::new(name.to_string());
        return Ok(WifiAdapter::Nmcli(Arc::new(n)));
    }
    Err(())
}

/// A wifi adapter
#[enum_dispatch::enum_dispatch(WifiAdapterTrait)]
pub enum WifiAdapter {
    /// nmcli utility on linux is used to interface with the wifi adapter
    Nmcli(Arc<linux::Nmcli>),
}