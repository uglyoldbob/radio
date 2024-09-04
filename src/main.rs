use std::{collections::{HashMap, HashSet}, time::Duration};

use bluer::{AdapterEvent, Uuid};
use eframe::egui::{self, Vec2};
use futures::{pin_mut, StreamExt};

#[enum_dispatch::enum_dispatch]
trait SubwindowTrait {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow>;
}

enum MessageFromAsync {
    BluetoothExists,
    NewBluetoothDevice(bluer::Address),
    OldBluetoothDevice(bluer::Address),
    BluetoothDeviceProperty(bluer::Address, bluer::DeviceProperty),
}

enum MessageToAsync {
    BluetoothScan(bool),
    Quit,
}

struct MainPage {}

impl SubwindowTrait for MainPage {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        let r = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
            if ui.button("quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        r
    }
}

struct BluetoothConfig {}

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
            egui::scroll_area::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.label("This is the bluetooth page");
                if !common.bluetooth_scanning {
                    if ui.button("Scan").clicked() {
                        common.bluetooth_scanning = true;
                        let _ = common.tx.blocking_send(MessageToAsync::BluetoothScan(common.bluetooth_scanning));
                    }
                }
                else {
                    if ui.button("Stop scanning").clicked() {
                        common.bluetooth_scanning = false;
                        let _ = common.tx.blocking_send(MessageToAsync::BluetoothScan(common.bluetooth_scanning));
                    }
                }
                let mut bd: Vec<(&bluer::Address, &BluetoothDeviceInfo)> = common.bluetooth_devices.iter().collect();
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

#[enum_dispatch::enum_dispatch(SubwindowTrait)]
enum Subwindow {
    MainPage(MainPage),
    BluetoothConfig(BluetoothConfig),
}

impl Default for Subwindow {
    fn default() -> Self {
        Subwindow::MainPage(MainPage {})
    }
}

fn main() {
    let (tx, rx) = tokio::sync::mpsc::channel(20);
    let (tx2, rx2) = tokio::sync::mpsc::channel(20);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    let threaded_rt = tokio::runtime::Runtime::new().unwrap();
    threaded_rt.spawn(async {
        async_main(tx, rx2).await;
    });
    eframe::run_native(
        "Uob Radio Gui",
        options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc, rx, tx2)))),
    );
}

struct BluetoothDeviceInfo {
    name: Option<String>,
    ty: Option<bluer::AddressType>,
    icon: Option<String>,
    class: Option<u32>,
    appearance: Option<u16>,
    uuids: HashSet<Uuid>,
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
    fn new() -> Self {
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
}

struct CommonWindowProperties {
    bluetooth_scanning: bool,
    bluetooth_devices: HashMap<bluer::Address, BluetoothDeviceInfo>,
    rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
    tx: tokio::sync::mpsc::Sender<MessageToAsync>,
}

impl CommonWindowProperties {
    pub fn new(rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
        tx: tokio::sync::mpsc::Sender<MessageToAsync>,) -> Self {
        Self {
            bluetooth_scanning: false,
            bluetooth_devices: HashMap::new(),
            rx,
            tx,
        }
    }
}

struct MyEguiApp {
    subwindow: Subwindow,
    check: bool,
    common: CommonWindowProperties,
}

async fn async_main(
    tx: tokio::sync::mpsc::Sender<MessageFromAsync>,
    mut rx: tokio::sync::mpsc::Receiver<MessageToAsync>,
) {
    let bluetooth = bluer::Session::new().await.unwrap();
    let mut bluetooth_devices: HashMap<bluer::Address, (&bluer::Adapter, Option<bluer::Device>)> = HashMap::new();
    let adapter_names = bluetooth.adapter_names().await.unwrap();
    let adapters: Vec<bluer::Adapter> = adapter_names
        .iter()
        .filter_map(|n| bluetooth.adapter(n).ok())
        .collect();
    for adapter in &adapters {
        println!("Adapter name is {}", adapter.name());
    }

    let blue_profiles = vec![bluer::rfcomm::Profile::default()];
    let mut blue_profile_handles = Vec::new();
    for p in &blue_profiles {
        if let Ok(h) = bluetooth.register_profile(p.clone()).await {
            println!("Got a bluetooth profile handle");
            blue_profile_handles.push(h);
        }
    }

    let blue_agent = bluer::agent::Agent::default();
    let blue_agent_handle = bluetooth.register_agent(blue_agent).await;

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
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

impl MyEguiApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        rx: tokio::sync::mpsc::Receiver<MessageFromAsync>,
        tx: tokio::sync::mpsc::Sender<MessageToAsync>,
    ) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self {
            subwindow: Subwindow::MainPage(MainPage {}),
            check: false,
            common: CommonWindowProperties::new(rx, tx),
        }
    }
}

impl eframe::App for MyEguiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.common.tx.blocking_send(MessageToAsync::Quit);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint();
        egui_extras::install_image_loaders(ctx);
        while let Ok(m) = self.common.rx.try_recv() {
            match m {
                MessageFromAsync::BluetoothExists => {
                    println!("Received a bluetooth exists message");
                }
                MessageFromAsync::NewBluetoothDevice(addr) => {
                    self.common.bluetooth_devices.insert(addr, BluetoothDeviceInfo::new());
                }
                MessageFromAsync::OldBluetoothDevice(addr) => {
                    //self.common.bluetooth_devices.remove_entry(&addr);
                }
                MessageFromAsync::BluetoothDeviceProperty(addr, prop) => {
                    println!("Received bluetooth device property: {:?}: {:?}", addr, prop);
                    if let Some(d) = self.common.bluetooth_devices.get_mut(&addr) {
                        match prop {
                            bluer::DeviceProperty::Name(n) => d.name = Some(n),
                            bluer::DeviceProperty::RemoteAddress(_) => {}
                            bluer::DeviceProperty::AddressType(at) => d.ty = Some(at),
                            bluer::DeviceProperty::Icon(icon) => d.icon = Some(icon),
                            bluer::DeviceProperty::Class(class) => d.class = Some(class),
                            bluer::DeviceProperty::Appearance(a) => d.appearance = Some(a),
                            bluer::DeviceProperty::Uuids(u) => d.uuids = u,
                            bluer::DeviceProperty::Paired(p) => d.paired = p,
                            bluer::DeviceProperty::Connected(c) => d.connected = c,
                            bluer::DeviceProperty::Trusted(t) => d.trusted = t,
                            bluer::DeviceProperty::Blocked(b) => d.blocked = b,
                            bluer::DeviceProperty::WakeAllowed(w) => d.wake = w,
                            bluer::DeviceProperty::Alias(a) => d.alias = Some(a),
                            bluer::DeviceProperty::LegacyPairing(lp) => d.legacy_pair = lp,
                            bluer::DeviceProperty::Modalias(_) => {}
                            bluer::DeviceProperty::Rssi(r) => d.rssi = Some(r),
                            bluer::DeviceProperty::TxPower(t) => d.txpwr = Some(t),
                            bluer::DeviceProperty::ManufacturerData(_) => {}
                            bluer::DeviceProperty::ServiceData(_) => {}
                            bluer::DeviceProperty::ServicesResolved(_) => {}
                            bluer::DeviceProperty::AdvertisingFlags(_) => {}
                            bluer::DeviceProperty::AdvertisingData(_) => {}
                            bluer::DeviceProperty::BatteryPercentage(b) => d.battery = Some(b),
                            _ => {}
                        }
                    }
                }
            }
        }
        egui::TopBottomPanel::bottom("Bottom Icons")
            .min_height(74.0)
            .max_height(74.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("B").clicked() {
                        self.subwindow = Subwindow::BluetoothConfig(BluetoothConfig::new());
                    }
                    if ui
                        .add(
                            egui::Image::new(egui::include_image!("../refresh.png"))
                                .maintain_aspect_ratio(true)
                                .fit_to_exact_size(Vec2 { x: 64.0, y: 64.0 })
                                .max_height(64.0)
                                .sense(egui::Sense::click()),
                        )
                        .clicked
                    {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if self.check {
                        ui.label("LABEL");
                        self.check = false;
                    } else {
                        ui.label("POTATO");
                        self.check = true;
                    }
                })
            });
        if let Some(sub) = self.subwindow.update(ctx, frame, &mut self.common) {
            self.subwindow = sub;
        }
    }
}
