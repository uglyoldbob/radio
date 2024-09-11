use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;

use crate::MessageFromAsync;

use super::SubwindowTrait;
use super::CommonWindowProperties;
use super::Subwindow;
use super::MessageToAsync;
use bluer::AdapterEvent;
use bluer::DeviceProperty;
use eframe::egui;
use futures::FutureExt;
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct BluetoothData {
    scanning: bool,
    pub devices: HashMap<bluer::Address, BluetoothDeviceInfo>,
}

async fn query_adapter(adapter: &bluer::Adapter) -> bluer::Result<()> {
    println!("    Address:                    {}", adapter.address().await?);
    println!("    Address type:               {}", adapter.address_type().await?);
    println!("    Friendly name:              {}", adapter.alias().await?);
    println!("    Modalias:                   {:?}", adapter.modalias().await?);
    println!("    Powered:                    {:?}", adapter.is_powered().await?);
    println!("    Discoverabe:                {:?}", adapter.is_discoverable().await?);
    println!("    Pairable:                   {:?}", adapter.is_pairable().await?);
    println!("    UUIDs:                      {:?}", adapter.uuids().await?);
    println!();
    println!("    Active adv. instances:      {}", adapter.active_advertising_instances().await?);
    println!("    Supp.  adv. instances:      {}", adapter.supported_advertising_instances().await?);
    println!("    Supp.  adv. includes:       {:?}", adapter.supported_advertising_system_includes().await?);
    println!("    Adv. capabilites:           {:?}", adapter.supported_advertising_capabilities().await?);
    println!("    Adv. features:              {:?}", adapter.supported_advertising_features().await?);

    Ok(())
}

pub async fn bluetooth(tx: tokio::sync::mpsc::Sender<MessageFromAsync>,
    rx: &mut tokio::sync::mpsc::Receiver<MessageToAsync>) {
    println!("Starting bluetooth code");
    let bluetooth = bluer::Session::new().await.unwrap();
    println!("Got a bluetooth session");

    let mut blue_agent = bluer::agent::Agent::default();
    blue_agent.request_default = true;
    blue_agent.request_pin_code = Some(Box::new(move |a| {
        async move {
            println!("Pin requested {:?}", a); 
            Ok("1234".to_string())
        }.boxed()
    }));
    blue_agent.request_passkey = Some(Box::new(move |a| {
        async move {
            println!("passkey requested {:?}", a); 
            Ok(42)
        }.boxed()
    }));
    blue_agent.display_passkey = Some(Box::new(move |a| {
        async move {
            println!("Need to display passkey {:?}", a); 
            Ok(())
        }.boxed()
    }));
    blue_agent.display_pin_code = Some(Box::new(move |a| {
        async move {
            println!("Need to display pin code {:?}", a); 
            Ok(())
        }.boxed()
    }));
    blue_agent.request_confirmation = Some(Box::new(move |a| {
        async move {
            println!("Confirmation requested {:?}", a);
            Ok(())
        }.boxed()
    }));
    blue_agent.request_authorization = Some(Box::new(move |a| {
        async move {
            println!("authorization requested {:?}", a); 
            Ok(())
        }.boxed()
    }));
    blue_agent.authorize_service = Some(Box::new(move |a| {
        async move {
            println!("authorize service requested {:?}", a); 
            Ok(())
        }.boxed()
    }));
    let blue_agent_handle = bluetooth.register_agent(blue_agent).await;
    println!("Registered a bluetooth agent");

    let profile = bluer::rfcomm::Profile {
        uuid: bluer::Uuid::from_str("0000111e-0000-1000-8000-00805f9b34fb").unwrap(),
        name: Some("Car audio".to_string()),
        service: None,
        role: None,
        channel: None,
        psm: None,
        require_authentication: Some(true),
        require_authorization: Some(true),
        auto_connect: Some(true),
        service_record: None,
        version: None,
        features: Some(1),
        ..Default::default()
    };

    let mut bluetooth_devices: HashMap<bluer::Address, (&bluer::Adapter, Option<bluer::Device>)> = HashMap::new();
    let adapter_names = bluetooth.adapter_names().await.unwrap();
    let adapters: Vec<bluer::Adapter> = adapter_names
        .iter()
        .filter_map(|n| bluetooth.adapter(n).ok())
        .collect();

    tx.send(MessageFromAsync::BluetoothPresent(!adapters.is_empty())).await;

    println!("Enabling bluetooth stuff now");
    for adapter in &adapters {
        adapter.set_powered(true).await.unwrap();
        adapter.set_discoverable(true).await.unwrap();
        adapter.set_pairable(true).await.unwrap();
    }
    println!("Done enabling bluetooth stuff");

    for adapter in &adapters {
        println!("there is an adapter");
        query_adapter(adapter).await;
    }
    println!("Registering a profile");

    let mut h = bluetooth.register_profile(profile).await;

    for adapter in &adapters {
        query_adapter(adapter).await;
    }

    let mut adapter_scanner = Vec::new();
    for a in &adapters {
        let da = a.discover_devices_with_changes().await.unwrap();
        adapter_scanner.push((a, da));
    }

    let mut quit = false;
    let mut scan = false;
    while !quit {
        while let Ok(m) = rx.try_recv() {
            match m {
                MessageToAsync::BluetoothScan(f) => {
                    scan = f;
                }
                MessageToAsync::Quit => {
                    quit = true;
                    println!("Exiting async code now");
                }
            }
        }
        if scan {
            for (adapt, da) in &mut adapter_scanner {
                if let Some(e) = da.next().await {
                    match e {
                        AdapterEvent::DeviceAdded(addr) => {
                            println!("Device added {:?}", addr);
                            bluetooth_devices.insert(addr, (adapt, None));
                            tx.send(MessageFromAsync::NewBluetoothDevice(addr)).await;
                        }
                        AdapterEvent::DeviceRemoved(addr) => {
                            println!("Device removed {:?}", addr);
                            bluetooth_devices.remove_entry(&addr);
                            tx.send(MessageFromAsync::OldBluetoothDevice(addr)).await;
                        }
                        AdapterEvent::PropertyChanged(prop) => {
                            println!("Property changed {:?}", prop);
                        }
                    }
                }
            }
        }
        for (addr, (adapter, dev)) in &mut bluetooth_devices {
            if dev.is_none() {
                if let Ok(d) = adapter.device(*addr) {
                    if let Ok(ps) = d.all_properties().await {
                        for p in ps {
                            tx.send(MessageFromAsync::BluetoothDeviceProperty(*addr, p)).await;
                        }
                    }
                    *dev = Some(d);
                }
            }
        }

        if let Ok(h) = &mut h {
            if let Some(a) = h.next().await {
                println!("Got a connection to car audio");
                let con = a.accept().unwrap();
                let (mut r, mut w) = con.into_split();
                w.write(&vec![0_u8, 0, 0, 0]).await.unwrap();
                match r.read_u8().await {
                    Ok(a) => println!("Recieved bluetooth byte {:x}", a),
                    Err(e) => println!("Error receiving bluetooth data {:?}", e),
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

impl BluetoothData {
    pub fn new() -> Self {
        Self {
            scanning: false,
            devices: HashMap::new(),
        }
    }
}

pub struct BluetoothDeviceInfo {
    name: Option<String>,
    ty: Option<bluer::AddressType>,
    icon: Option<String>,
    class: Option<u32>,
    appearance: Option<u16>,
    uuids: HashSet<bluer::Uuid>,
    paired: bool,
    connected: bool,
    trusted: bool,
    blocked: bool,
    wake: bool,
    alias: Option<String>,
    legacy_pair: bool,
    rssi: Option<i16>,
    txpwr: Option<i16>,
    battery: Option<u8>,
}

impl BluetoothDeviceInfo {
    pub fn new() -> Self {
        Self {
            name: None,
            ty: None,
            icon: None,
            class: None,
            appearance: None,
            uuids: HashSet::new(),
            paired: false,
            connected: false,
            trusted: false,
            blocked: false,
            wake: false,
            alias: None,
            legacy_pair: false,
            rssi: None,
            txpwr: None,
            battery: None,
        }
    }

    pub fn update(&mut self, prop: DeviceProperty) {
        match prop {
            bluer::DeviceProperty::Name(n) => self.name = Some(n),
            bluer::DeviceProperty::RemoteAddress(_) => {}
            bluer::DeviceProperty::AddressType(at) => self.ty = Some(at),
            bluer::DeviceProperty::Icon(icon) => self.icon = Some(icon),
            bluer::DeviceProperty::Class(class) => self.class = Some(class),
            bluer::DeviceProperty::Appearance(a) => self.appearance = Some(a),
            bluer::DeviceProperty::Uuids(u) => self.uuids = u,
            bluer::DeviceProperty::Paired(p) => self.paired = p,
            bluer::DeviceProperty::Connected(c) => self.connected = c,
            bluer::DeviceProperty::Trusted(t) => self.trusted = t,
            bluer::DeviceProperty::Blocked(b) => self.blocked = b,
            bluer::DeviceProperty::WakeAllowed(w) => self.wake = w,
            bluer::DeviceProperty::Alias(a) => self.alias = Some(a),
            bluer::DeviceProperty::LegacyPairing(lp) => self.legacy_pair = lp,
            bluer::DeviceProperty::Modalias(_) => {}
            bluer::DeviceProperty::Rssi(r) => self.rssi = Some(r),
            bluer::DeviceProperty::TxPower(t) => self.txpwr = Some(t),
            bluer::DeviceProperty::ManufacturerData(_) => {}
            bluer::DeviceProperty::ServiceData(_) => {}
            bluer::DeviceProperty::ServicesResolved(_) => {}
            bluer::DeviceProperty::AdvertisingFlags(_) => {}
            bluer::DeviceProperty::AdvertisingData(_) => {}
            bluer::DeviceProperty::BatteryPercentage(b) => self.battery = Some(b),
            _ => {}
        }
    }
}

pub struct BluetoothConfig {
}

impl BluetoothConfig {
    pub fn new() -> Self {
        Self {}
    }
}

impl SubwindowTrait for BluetoothConfig {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            if !common.bluetooth.scanning {
                if ui.button("Scan").clicked() {
                    common.bluetooth.scanning = true;
                    let _ = common.tx.blocking_send(MessageToAsync::BluetoothScan(common.bluetooth.scanning));
                }
            }
            else {
                if ui.button("Stop scanning").clicked() {
                    common.bluetooth.scanning = false;
                    let _ = common.tx.blocking_send(MessageToAsync::BluetoothScan(common.bluetooth.scanning));
                }
            }
            egui::scroll_area::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                let mut bd: Vec<(&bluer::Address, &BluetoothDeviceInfo)> = common.bluetooth.devices.iter().collect();
                bd.sort_by(|(_a1, a2), (_b1, b2)| {
                    b2.rssi.cmp(&a2.rssi)
                });
                for (a, dev) in bd {
                    let t = format!("RSSI: {:?}, icon {:?}", dev.rssi, dev.icon);
                    if let Some(a) = &dev.alias {
                        ui.label(format!("Device: {} {}", a, t));
                    }
                    else {
                        ui.label(format!("Device {:?} {}", a, t));
                    }
                }
            });
        });
        None
    }
}
