//! Linux specific bluetooth code

use std::collections::{HashMap, HashSet};

use bluer::{AdapterEvent, DeviceProperty};
use futures::StreamExt;
use futures::FutureExt;

/// The general bluetooth handler for the library. There should be only one per application on linux.
pub struct BluetoothHandler {
    session: bluer::Session,
    adapters: Vec<bluer::Adapter>,
    _blue_agent_handle: bluer::agent::AgentHandle,
}

impl BluetoothHandler {
    /// Construct a new self
    pub async fn new(s: tokio::sync::mpsc::Sender<super::MessageToBluetoothHost>) -> Option<Self> {
        let session = bluer::Session::new().await.ok()?;

        let adapter_names = session.adapter_names().await.unwrap();
        let adapters: Vec<bluer::Adapter> = adapter_names
            .iter()
            .filter_map(|n| session.adapter(n).ok())
            .collect();

        let blue_agent = Self::build_agent(s);
        let blue_agent_handle = session.register_agent(blue_agent).await;
        println!("Registered a bluetooth agent {}", blue_agent_handle.is_ok());
        Some(Self {
            session,
            adapters,
            _blue_agent_handle: blue_agent_handle.ok()?,
        })
    }

    async fn enable(&mut self) {
        for adapter in &self.adapters {
            adapter.set_powered(true).await.unwrap();
            adapter.set_pairable(true).await.unwrap();
        }
    }

    async fn disable(&mut self) {
        self.set_discoverable(false).await;
        for adapter in &self.adapters {
            adapter.set_powered(false).await.unwrap();
            adapter.set_pairable(false).await.unwrap();
        }
    }

    /// Enable or disable the discoverable of all bluetooth adapters
    pub async fn set_discoverable(&mut self, d: bool) {
        for adapter in &self.adapters {
            adapter.set_discoverable(d).await.unwrap();
        }
    }

    /// Register a profile with the bluetooth session
    pub async fn register_rfcomm_profile(&mut self, profile: bluer::rfcomm::Profile) -> Result<bluer::rfcomm::ProfileHandle, bluer::Error> {
        self.session.register_profile(profile).await
    }

    fn build_agent(s: tokio::sync::mpsc::Sender<super::MessageToBluetoothHost>) -> bluer::agent::Agent {
        let mut blue_agent = bluer::agent::Agent::default();
        blue_agent.request_default = true;
        blue_agent.request_pin_code = None;
        blue_agent.request_passkey = None;
        let s2 = s.clone();
        blue_agent.display_passkey = Some(Box::new(move |mut a| {
            println!("Running process for display_passkey: {:?}", a);
            let s3 = s2.clone();
            async move {
                let mut chan = tokio::sync::mpsc::channel(5);
                let _ = s3.send(super::MessageToBluetoothHost::DisplayPasskey(a.passkey, chan.0)).await;
                loop {
                    let f = tokio::time::timeout(std::time::Duration::from_secs(5), chan.1.recv());
                    tokio::select! {
                        asdf = f => {
                            match asdf {
                                Ok(Some(m)) => {
                                    match m {
                                        super::ResponseToPasskey::Yes => {
                                            let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                            return Ok(());
                                        }
                                        super::ResponseToPasskey::No => {
                                            let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                            return Err(bluer::agent::ReqError::Rejected);
                                        }
                                        super::ResponseToPasskey::Cancel => {
                                            let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                            return Err(bluer::agent::ReqError::Canceled);
                                        }
                                        super::ResponseToPasskey::Waiting => {}
                                    }
                                }
                                Ok(None) => {}
                                _ => {
                                    let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                    return Err(bluer::agent::ReqError::Canceled);
                                }
                            }
                        }
                        _ = &mut a.cancel => {
                            let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                            break Err(bluer::agent::ReqError::Canceled);
                        }
                    }
                }
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
        let s2 = s.clone();
        blue_agent.request_confirmation = Some(Box::new(move |a| {
            println!("Need to confirm {:?}", a);
            let s3 = s2.clone();
            async move {
                let mut chan = tokio::sync::mpsc::channel(5);
                let _ = s3.send(super::MessageToBluetoothHost::ConfirmPasskey(a.passkey, chan.0)).await;
                loop {
                    let f = tokio::time::timeout(std::time::Duration::from_secs(5), chan.1.recv());
                    let asdf = f.await;
                    match asdf {
                        Ok(Some(m)) => {
                            match m {
                                super::ResponseToPasskey::Yes => {
                                    let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                    return Ok(());
                                }
                                super::ResponseToPasskey::No => {
                                    let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                    return Err(bluer::agent::ReqError::Rejected);
                                }
                                super::ResponseToPasskey::Cancel => {
                                    let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                                    return Err(bluer::agent::ReqError::Canceled);
                                }
                                super::ResponseToPasskey::Waiting => {}
                            }
                        }
                        Ok(None) => {}
                        _ => {
                            let _ = s3.send(super::MessageToBluetoothHost::CancelDisplayPasskey).await;
                            return Err(bluer::agent::ReqError::Canceled);
                        }
                    }
                }
            }
            .boxed()
        }));
        blue_agent.request_authorization = Some(Box::new(|a| {
            async move {
                println!("Need to authorize {:?}", a);
                Ok(())
            }
            .boxed()
        }));
        blue_agent.authorize_service = Some(Box::new(|a| {
            async move {
                println!("Need to authorize service {:?}", a);
                Ok(())
            }
            .boxed()
        }));
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

    /// run a scan on all the bluetooth adapters
    pub async fn scan<'a>(&'a mut self, bluetooth_devices: &mut HashMap<bluer::Address, (&'a bluer::Adapter, Option<bluer::Device>)>) {
        let mut adapter_scanner = Vec::new();
        for a in &self.adapters {
            let da = a.discover_devices_with_changes().await.unwrap();
            adapter_scanner.push((a, da));
        }

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
        for (addr, (adapter, dev)) in bluetooth_devices {
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
