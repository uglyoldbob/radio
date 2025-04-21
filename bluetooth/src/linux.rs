//! Linux specific bluetooth code

use std::{collections::{HashMap, HashSet}, str::FromStr, time::Duration};

use bluer::{AdapterEvent, DeviceProperty};
use futures::StreamExt;
use futures::FutureExt;

/// The general bluetooth handler for the library. There should be only one per application on linux.
pub struct BluetoothHandler {
    session: bluer::Session,
    adapters: Vec<bluer::Adapter>,
    blue_agent_handle: bluer::agent::AgentHandle,
}

impl BluetoothHandler {
    /// Construct a new self
    pub async fn new() -> Option<Self> {
        let session = bluer::Session::new().await.ok()?;
        let blue_agent = Self::build_agent();
        let blue_agent_handle = session.register_agent(blue_agent).await;
        println!("Registered a bluetooth agent {}", blue_agent_handle.is_ok());
        Some(Self {
            session,
            adapters: Vec::new(),
            blue_agent_handle: blue_agent_handle.ok()?,
        })
    }

    fn build_agent() -> bluer::agent::Agent {
        let mut blue_agent = bluer::agent::Agent::default();
        blue_agent.request_default = true;
        blue_agent.request_pin_code = None;
        blue_agent.request_passkey = None;
        blue_agent.display_passkey = Some(Box::new(|a| {
            async move {
                println!("Need to display passkey {:?}", a);
                a.cancel.await.unwrap();
                Ok(())
            }
            .boxed()
        }));
        blue_agent.display_pin_code = Some(Box::new(|a| {
            async move {
                println!("Need to display pin code {:?}", a);
                a.cancel.await.unwrap();
                Ok(())
            }
            .boxed()
        }));
        blue_agent.request_confirmation = Some(Box::new(|a| {
            async move {
                println!("Need to confirm {:?}", a);
                Ok(())
            }
            .boxed()
        }));
        blue_agent.request_authorization = None;
        blue_agent.authorize_service = None;
        blue_agent
    }

    /// Issues the specified bluetooth command, with an optional response for the command
    pub async fn issue_command(&mut self, cmd: super::BluetoothCommand) -> Option<super::BluetoothResponse> {
        match cmd {
            super::BluetoothCommand::QueryNumAdapters => {
                Some(super::BluetoothResponse::Adapters(0))
            }
            _ => None,
        }
    }
}

async fn query_adapter(adapter: &bluer::Adapter) -> bluer::Result<()> {
    println!(
        "    Address:                    {}",
        adapter.address().await?
    );
    println!(
        "    Address type:               {}",
        adapter.address_type().await?
    );
    println!("    Friendly name:              {}", adapter.alias().await?);
    println!(
        "    Modalias:                   {:?}",
        adapter.modalias().await?
    );
    println!(
        "    Powered:                    {:?}",
        adapter.is_powered().await?
    );
    println!(
        "    Discoverabe:                {:?}",
        adapter.is_discoverable().await?
    );
    println!(
        "    Pairable:                   {:?}",
        adapter.is_pairable().await?
    );
    println!(
        "    UUIDs:                      {:?}",
        adapter.uuids().await?
    );
    println!();
    println!(
        "    Active adv. instances:      {}",
        adapter.active_advertising_instances().await?
    );
    println!(
        "    Supp.  adv. instances:      {}",
        adapter.supported_advertising_instances().await?
    );
    println!(
        "    Supp.  adv. includes:       {:?}",
        adapter.supported_advertising_system_includes().await?
    );
    println!(
        "    Adv. capabilites:           {:?}",
        adapter.supported_advertising_capabilities().await?
    );
    println!(
        "    Adv. features:              {:?}",
        adapter.supported_advertising_features().await?
    );

    Ok(())
}

/// Dummy function
pub async fn bluetooth(
) {
    println!("Starting bluetooth code");
    let bluetooth = bluer::Session::new().await.unwrap();
    println!("Got a bluetooth session");

    let profile = bluer::rfcomm::Profile {
        uuid: bluer::Uuid::from_str(crate::uuid::Uuid::HfpHs.as_str()).unwrap(),
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

    let mut bluetooth_devices: HashMap<bluer::Address, (&bluer::Adapter, Option<bluer::Device>)> =
        HashMap::new();
    let adapter_names = bluetooth.adapter_names().await.unwrap();
    let adapters: Vec<bluer::Adapter> = adapter_names
        .iter()
        .filter_map(|n| bluetooth.adapter(n).ok())
        .collect();

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
    let profile = tokio::task::spawn(async move {
        if let Ok(h) = &mut h {
            println!("Got a connection to car audio?");
        }
    });

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
        if scan {
            for (adapt, da) in &mut adapter_scanner {
                if let Some(e) = da.next().await {
                    match e {
                        AdapterEvent::DeviceAdded(addr) => {
                            println!("Device added {:?}", addr);
                            bluetooth_devices.insert(addr, (adapt, None));
                        }
                        AdapterEvent::DeviceRemoved(addr) => {
                            println!("Device removed {:?}", addr);
                            bluetooth_devices.remove_entry(&addr);
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
                        }
                    }
                    *dev = Some(d);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    profile.await.unwrap();
}

/// Dummy struct
pub struct BluetoothData {
    scanning: bool,
    devices: HashMap<bluer::Address, BluetoothDeviceInfo>,
}

impl BluetoothData {
    /// construct a new self
    pub fn new() -> Self {
        Self {
            scanning: false,
            devices: HashMap::new(),
        }
    }
}

/// Holds the known informatio for a bluetooth device
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
    /// Construct a new self
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

    /// Update the device with the given property
    fn update(&mut self, prop: DeviceProperty) {
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
