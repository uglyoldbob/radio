//! Hotspot workaround code

use nmrs::{builders, WifiSecurity};
use std::collections::HashMap;
use zbus::{proxy, Connection};
use zvariant::{OwnedObjectPath, OwnedValue};

// Type alias matching what NM's D-Bus API expects
type NmSettings = HashMap<String, HashMap<String, OwnedValue>>;

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManagerProxy {
    // Returns list of device object paths
    fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
trait NmSettingsProxy {
    fn add_and_activate_connection(
        &self,
        connection: &NmSettings,
        device: &OwnedObjectPath,
        specific_object: &OwnedObjectPath,
    ) -> zbus::Result<(OwnedObjectPath, OwnedObjectPath)>;
}

// The correct interface for AddAndActivateConnection is actually on NM itself:
#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NmProxy {
    fn add_and_activate_connection(
        &self,
        connection: &NmSettings,
        device: &OwnedObjectPath,
        specific_object: &OwnedObjectPath,
    ) -> zbus::Result<(OwnedObjectPath, OwnedObjectPath)>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmDeviceProxy {
    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;
}

fn to_owned_settings(
    input: HashMap<&str, HashMap<&str, zvariant::Value<'_>>>,
) -> HashMap<String, HashMap<String, OwnedValue>> {
    input
        .into_iter()
        .map(|(section, props)| {
            let owned_props = props
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
                .collect();
            (section.to_string(), owned_props)
        })
        .collect()
}

pub async fn start_hotspot(ssid: String, psk: String, wifi_dev_path: &str) -> Result<(), String> {
    let p = nmrs::WifiSecurity::WpaPsk { psk };
    let co = nmrs::ConnectionOptions::new(true);
    let mut test = nmrs::builders::build_wifi_connection(&ssid, &p, &co);
    service::log::info!("The hotspot details are {:#?}", test);
    let q = test.get("802-11-wireless").map(|i| i.get("mode")).flatten();
    service::log::info!("The mode is {q:?}");
    test.get_mut("802-11-wireless")
        .map(|i| i.insert("mode", "ap".into()));
    let q = test.get("802-11-wireless").map(|i| i.get("mode")).flatten();
    service::log::info!("The mode is {q:?}");
    let hr = build_hotspot(&wifi_dev_path, test).await;
    service::log::info!("The result of making a hotspot is {hr:#?}");
    Ok(())
}

/// construct an access point or hotspot
async fn build_hotspot(
    wifi_hw: &str,
    settings: HashMap<&str, HashMap<&str, zvariant::Value<'_>>>,
) -> Result<(), String> {
    let settings = to_owned_settings(settings);
    let dbus = Connection::system().await.map_err(|e| e.to_string())?;
    let wifi_device = OwnedObjectPath::try_from(wifi_hw).map_err(|e| e.to_string())?;
    let any: OwnedObjectPath = OwnedObjectPath::try_from("/").unwrap();
    let nm = NmProxyProxy::new(&dbus).await.map_err(|e| e.to_string())?;
    let (conn_path, active_conn_path) = nm
        .add_and_activate_connection(&settings, &wifi_device, &any)
        .await
        .map_err(|e| e.to_string())?;
    println!("Connection object path:        {}", conn_path);
    println!("Active connection object path: {}", active_conn_path);
    Ok(())
}
