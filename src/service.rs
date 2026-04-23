#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This program is for handling the video and audio components for the radio

#[cfg(feature = "wifi")]
mod nmrs_extensions;

#[cfg(feature = "androidauto")]
use std::collections::HashSet;

use std::{
    collections::VecDeque,
    io::{Read, Seek, Write},
    path::PathBuf,
    sync::Arc,
};

#[cfg(feature = "androidauto")]
use android_auto::{
    AndroidAutoAudioInputTrait, AndroidAutoAudioOutputTrait, AndroidAutoInputChannelTrait,
    HeadUnitInfo, NetworkInformation,
};
#[cfg(feature = "bluetooth")]
use bluetooth_rust::{BluetoothAdapterTrait, ResponseToPasskey};
use chrono::DateTime;
use tokio::io::AsyncReadExt;
#[cfg(feature = "androidauto")]
use uobradio_comms::aauto::AndroidAutoMessageFromPhone;
#[cfg(feature = "wifi")]
use uobradio_comms::wireless::WifiConfig;
use uobradio_comms::{
    HvacController, InclinometerOrientation, NonvolatileSettings, PublicData, Sensors,
};
use video_service::VideoSource;

use crate::sensors::{
    BoolSensor, InclinometerSensor, PressureSensor, RpmSensor, TemperatureSensor, VoltageSensor,
};

use crate::sensors::{
    BoolSensorConfig, InclinometerSensorConfig, PressureSensorConfig, RpmSensorConfig,
    TemperatureSensorConfig, VoltageSensorConfig,
};

#[cfg(feature = "bluetooth")]
mod messages;
#[cfg(feature = "bluetooth")]
use messages::*;

mod outputs;
mod sensors;
mod video_service;

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct MainConfiguration {
    /// The desired minimum debug level
    debug_level: Option<service::LogLevel>,
}

/// The optional command line arguments for the service
#[derive(clap::Parser, Debug)]
struct Arguments {
    /// Specify the actual location for the non-volatile configuration file
    #[arg(long)]
    nvconfig: Option<PathBuf>,
}

/// System specific settings (not set by the user)
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct SystemSettings {
    /// Main cabin temperature sensor
    main_cabin_temperature_sensor: TemperatureSensorConfig,
    /// hvac vent temperature sensor
    hvac_vent_temperature_sensor: TemperatureSensorConfig,
    /// Orientation of the system, left-right, forwards-backwards, both in degrees
    orientation: InclinometerSensorConfig,
    /// The engine coolant temperature
    engine_coolant_temp: TemperatureSensorConfig,
    /// The engine oil temperature
    engine_oil_temp: TemperatureSensorConfig,
    /// The engine exhaust temperature
    engine_exhaust_temp: TemperatureSensorConfig,
    /// Front differential temperature
    front_diff_temp: TemperatureSensorConfig,
    /// Rear differential temperature
    rear_diff_temp: TemperatureSensorConfig,
    /// Intake air temperature
    intake_air: TemperatureSensorConfig,
    /// The engine oil pressure (psi)
    engine_oil_pressure: PressureSensorConfig,
    /// The coolant pressure (psi)
    coolant_pressure: PressureSensorConfig,
    /// Transmission temperature
    trans_temp: TemperatureSensorConfig,
    /// Transfer case temperature
    transfer_temp: TemperatureSensorConfig,
    /// Door open sensor
    door_open: BoolSensorConfig,
    /// Engine rpm sensor
    engine_rpm: RpmSensorConfig,
    /// Main system voltage
    main_voltage: VoltageSensorConfig,
    /// Logging settings
    log: SensorLogConfig,
    /// The oil pressure output
    gauge_oil_pressure: outputs::F32OutputConfig,
    /// the coolant temperature gauge output
    gauge_engine_temp: outputs::F32OutputConfig,
    /// The tachometer gauge output
    gauge_tachometer: outputs::F32OutputConfig,
    /// The ac clutch enable
    ac_clutch_enable: outputs::BoolOutputConfig,
    /// Heater enable output
    heater_enable_output: outputs::BoolOutputConfig,
    /// The temperature control output
    hvac_temperature_control: outputs::F32OutputConfig,
    /// The hvac fan output (low medium high)
    hvac_fan_output: outputs::BoolVecOutputConfig,
    /// The offroad lights
    offroad_lights: Vec<outputs::BoolOutputConfig>,
    /// The winch control output
    winch: outputs::BoolVecOutputConfig,
    /// The auxiliary outputs
    aux_out: outputs::BoolVecOutputConfig,
    /// The inverter config
    inverter: outputs::BoolOutputConfig,
    /// Door lock config
    door_lock: outputs::BoolOutputConfig,
    /// Door unlock config
    door_unlock: outputs::BoolOutputConfig,
    /// Window controls
    windows: Vec<outputs::BoolVecOutputConfig>,
    /// Camera led controls
    camera_leds: outputs::BoolVecOutputConfig,
    /// aux input sensors
    aux_in: Vec<sensors::BoolSensorConfig>,
}

/// All the inputs for the system
pub struct SystemInputs {
    /// aux input sensors
    aux_in: Vec<sensors::BoolSensor>,
    /// Main cabin temperature sensor
    main_cabin_temperature_sensor: TemperatureSensor,
    /// hvac vent temperature sensor
    hvac_vent_temperature_sensor: TemperatureSensor,
    /// Orientation of the system, left-right, forwards-backwards, both in degrees
    orientation: InclinometerSensor,
    /// The engine coolant temperature
    engine_coolant_temp: TemperatureSensor,
    /// The engine oil temperature
    engine_oil_temp: TemperatureSensor,
    /// The engine exhaust temperature
    engine_exhaust_temp: TemperatureSensor,
    /// Front differential temperature
    front_diff_temp: TemperatureSensor,
    /// Rear differential temperature
    rear_diff_temp: TemperatureSensor,
    /// Intake air temperature
    intake_air: TemperatureSensor,
    /// The engine oil pressure (psi)
    engine_oil_pressure: PressureSensor,
    /// The coolant pressure (psi)
    coolant_pressure: PressureSensor,
    /// Transmission temperature
    trans_temp: TemperatureSensor,
    /// Transfer case temperature
    transfer_temp: TemperatureSensor,
    /// Door open sensor
    door_open: BoolSensor,
    /// Engine rpm sensor
    engine_rpm: RpmSensor,
    /// Main system voltage
    main_voltage: VoltageSensor,
}

impl SystemInputs {
    /// Get sensor data
    pub fn get_sensor_data(&mut self) -> Result<uobradio_comms::Sensors, String> {
        use sensors::BoolSensorTrait;
        use sensors::InclinometerSensorTrait;
        use sensors::PressureSensorTrait;
        use sensors::RpmSensorTrait;
        use sensors::TemperatureSensorTrait;
        use sensors::VoltageSensorTrait;
        Ok(uobradio_comms::Sensors {
            intake_air_temperature: Some(self.intake_air.poll()?.fahrenheit()),
            orientation: Some(self.orientation.poll()),
            engine_coolant_temp: Some(self.engine_coolant_temp.poll()?.fahrenheit()),
            engine_oil_temp: Some(self.engine_oil_temp.poll()?.fahrenheit()),
            engine_exhaust_temp: Some(self.engine_exhaust_temp.poll()?.fahrenheit()),
            front_diff_temp: Some(self.front_diff_temp.poll()?.fahrenheit()),
            rear_diff_temp: Some(self.rear_diff_temp.poll()?.fahrenheit()),
            engine_oil_pressure: Some(self.engine_oil_pressure.poll()?),
            coolant_pressure: Some(self.coolant_pressure.poll()?),
            trans_temp: Some(self.trans_temp.poll()?.fahrenheit()),
            transfer_temp: Some(self.transfer_temp.poll()?.fahrenheit()),
            door_open: Some(self.door_open.poll()?),
            engine_rpm: Some(self.engine_rpm.poll()?),
            main_voltage: Some(self.main_voltage.poll()?),
        })
    }
}

/// All the outputs for the system
pub struct SystemOutputs {
    /// The oil pressure output
    gauge_oil_pressure: outputs::F32Output,
    /// the coolant temperature gauge output
    gauge_engine_temp: outputs::F32Output,
    /// The tachometer gauge output
    gauge_tachometer: outputs::F32Output,
    /// The ac clutch enable
    ac_clutch_enable: outputs::BoolOutput,
    /// Heater enable output
    heater_enable_output: outputs::BoolOutput,
    /// The temperature control output
    hvac_temperature_control: outputs::F32Output,
    /// The hvac fan output (low medium high)
    hvac_fan_output: outputs::BoolVecOutput,
    /// The offroad lights
    offroad_lights: Vec<outputs::BoolOutput>,
    /// The winch control output
    winch: outputs::BoolVecOutput,
    /// The auxiliary outputs
    aux_out: outputs::BoolVecOutput,
    /// The inverter power enable
    inverter: outputs::BoolOutput,
    /// Door lock output
    door_lock: outputs::BoolOutput,
    /// Door unlock output
    door_unlock: outputs::BoolOutput,
    /// Window controls
    windows: Vec<outputs::BoolVecOutput>,
    /// Camera led controls
    camera_leds: outputs::BoolVecOutput,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            main_cabin_temperature_sensor: Default::default(),
            hvac_vent_temperature_sensor: Default::default(),
            orientation: Default::default(),
            engine_coolant_temp: Default::default(),
            engine_oil_temp: Default::default(),
            engine_exhaust_temp: Default::default(),
            front_diff_temp: Default::default(),
            rear_diff_temp: Default::default(),
            intake_air: Default::default(),
            engine_oil_pressure: Default::default(),
            coolant_pressure: Default::default(),
            trans_temp: Default::default(),
            transfer_temp: Default::default(),
            door_open: Default::default(),
            engine_rpm: Default::default(),
            main_voltage: Default::default(),
            log: Default::default(),
            gauge_oil_pressure: Default::default(),
            gauge_engine_temp: Default::default(),
            gauge_tachometer: Default::default(),
            ac_clutch_enable: Default::default(),
            heater_enable_output: Default::default(),
            hvac_temperature_control: Default::default(),
            hvac_fan_output: Default::default(),
            offroad_lights: vec![
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            winch: Default::default(),
            aux_out: Default::default(),
            inverter: Default::default(),
            door_lock: Default::default(),
            door_unlock: Default::default(),
            windows: vec![
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
            camera_leds: Default::default(),
            aux_in: vec![
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
        }
    }
}

impl SystemSettings {
    /// Load the system settings from the current directory
    pub fn load() -> Self {
        let mut paths = Vec::new();
        paths.push(std::path::Path::new("./settings.toml"));
        #[cfg(target_os = "linux")]
        {
            paths.push(std::path::Path::new("/etc/radio/settings.toml"));
        }

        for p in paths {
            let f = std::fs::File::open(p);
            if let Ok(mut f) = f {
                let mut a = String::new();
                if f.read_to_string(&mut a).is_ok() {
                    match toml::from_str(&a) {
                        Ok(t) => {
                            return t;
                        }
                        Err(e) => {
                            log::error!("Failed to read config file {}: {}", p.display(), e);
                        }
                    }
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let p = std::path::Path::new("/tmp/radio-settings.toml");
            log::info!("Creating example settings at {}", p.display());
            let config = Self::default();
            config.save(p.into());
        }
        Self::default()
    }

    /// Save the system settings to the current directory
    pub fn save(&self, path: std::path::PathBuf) {
        let s = toml::to_string_pretty(self).unwrap();
        if let Ok(mut f) = std::fs::File::create_new(path) {
            f.write_all(s.as_bytes());
        }
    }

    /// Get the inputs for the system
    pub fn get_inputs(&self) -> Result<SystemInputs, String> {
        use sensors::BoolSensorConfigTrait;
        use sensors::InclinometerSensorConfigTrait;
        use sensors::PressureSensorConfigTrait;
        use sensors::RpmSensorConfigTrait;
        use sensors::TemperatureSensorConfigTrait;
        use sensors::VoltageSensorConfigTrait;
        let mut aux = Vec::new();
        for a in &self.aux_in {
            aux.push(a.build()?);
        }
        Ok(SystemInputs {
            aux_in: aux,
            main_cabin_temperature_sensor: self.main_cabin_temperature_sensor.build()?,
            hvac_vent_temperature_sensor: self.hvac_vent_temperature_sensor.build()?,
            orientation: self.orientation.build()?,
            engine_coolant_temp: self.engine_coolant_temp.build()?,
            engine_oil_temp: self.engine_oil_temp.build()?,
            engine_exhaust_temp: self.engine_exhaust_temp.build()?,
            front_diff_temp: self.front_diff_temp.build()?,
            rear_diff_temp: self.rear_diff_temp.build()?,
            intake_air: self.intake_air.build()?,
            engine_oil_pressure: self.engine_oil_pressure.build()?,
            coolant_pressure: self.coolant_pressure.build()?,
            trans_temp: self.trans_temp.build()?,
            transfer_temp: self.transfer_temp.build()?,
            door_open: self.door_open.build()?,
            engine_rpm: self.engine_rpm.build()?,
            main_voltage: self.main_voltage.build()?,
        })
    }

    /// Get the outputs for the system
    pub fn get_outputs(&self) -> Result<SystemOutputs, String> {
        use outputs::BoolOutputConfigTrait;
        use outputs::BoolVecOutputConfigTrait;
        use outputs::F32OutputConfigTrait;
        let mut orls = Vec::new();
        for or in &self.offroad_lights {
            orls.push(or.build()?);
        }
        let mut ws = Vec::new();
        for w in &self.windows {
            ws.push(w.build()?);
        }
        Ok(SystemOutputs {
            gauge_oil_pressure: self.gauge_oil_pressure.build()?,
            gauge_engine_temp: self.gauge_engine_temp.build()?,
            gauge_tachometer: self.gauge_tachometer.build()?,
            ac_clutch_enable: self.ac_clutch_enable.build()?,
            heater_enable_output: self.heater_enable_output.build()?,
            hvac_temperature_control: self.hvac_temperature_control.build()?,
            hvac_fan_output: self.hvac_fan_output.build()?,
            offroad_lights: orls,
            winch: self.winch.build()?,
            aux_out: self.aux_out.build()?,
            inverter: self.inverter.build()?,
            door_lock: self.door_lock.build()?,
            door_unlock: self.door_unlock.build()?,
            windows: ws,
            camera_leds: self.camera_leds.build()?,
        })
    }
}

/// The structure for starting and stopping the android auto service
#[cfg(feature = "androidauto")]
struct AndroidAutoService {
    /// Determines who deals with the android-auto stuff
    addr: std::net::SocketAddr,
    /// The task list of tasks running to make the android auto service work
    tasks: tokio::task::JoinSet<Result<(), String>>,
    /// Used to send messages to the android auto library
    sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
    /// Used to receive android auto messages from a users device
    recv: tokio::sync::mpsc::Receiver<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
}

#[cfg(feature = "androidauto")]
impl Drop for AndroidAutoService {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

#[cfg(feature = "androidauto")]
impl AndroidAutoService {
    /// Construct and start an android auto service
    pub async fn new(com: &AppUserCommon, addr: std::net::SocketAddr) -> Result<Self, String> {
        let tasks = tokio::task::JoinSet::new();

        let aautochan = tokio::sync::mpsc::channel(150);

        #[cfg(feature = "bluetooth")]
        let blue_addresses: Vec<bluetooth_rust::BluetoothAdapterAddress> = {
            if let Some(bluetooth) = com.bluetooth.supports_async() {
                let a = bluetooth.addresses().await;
                log::info!("Found {} bluetooth addresses8", a.len());
                a
            } else {
                panic!("Async not supported");
            }
        };
        #[cfg(feature = "bluetooth")]
        let bluetooth_address = Some(
            blue_addresses
                .first()
                .map(|b| match b {
                    bluetooth_rust::BluetoothAdapterAddress::String(s) => {
                        android_auto::BluetoothInformation {
                            address: s.to_owned(),
                        }
                    }
                    bluetooth_rust::BluetoothAdapterAddress::Byte(b) => {
                        let a = format!(
                            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                            b[0], b[1], b[2], b[3], b[4], b[5]
                        );
                        android_auto::BluetoothInformation { address: a }
                    }
                })
                .expect("No bluetooth hardware found"),
        );

        let config = android_auto::AndroidAutoConfiguration {
            unit: HeadUnitInfo {
                name: "UobRadio".to_string(),
                car_model: "Cherokee".to_string(),
                car_year: "1995".to_string(),
                car_serial: "42".to_string(),
                left_hand: true,
                head_manufacturer: "Uob".to_string(),
                head_model: "XJ1".to_string(),
                sw_build: "0".to_string(),
                sw_version: "1".to_string(),
                native_media: true,
                hide_clock: Some(false),
            },
            custom_certificate: None,
        };

        let aa_chan = tokio::sync::mpsc::channel(100);
        let main = AndroidAutoStuff::new(
            aautochan.0,
            aa_chan.1,
            aa_chan.0.clone(),
            #[cfg(feature = "bluetooth")]
            com.bluetooth.clone(),
            #[cfg(feature = "wifi")]
            com.aa_network.clone().unwrap(),
            #[cfg(feature = "bluetooth")]
            bluetooth_address,
        );
        let com2 = com.token.clone();
        tokio::spawn(async move {
            let mut joinset = tokio::task::JoinSet::new();
            let main = Box::new(main);
            use android_auto::AndroidAutoMainTrait;
            let a = main.run(config, &mut joinset, &com2).await;
            log::error!("Android auto run finished with {:?}", a);
            joinset.abort_all();
        });
        Ok(Self {
            addr,
            tasks,
            sender: aa_chan.0,
            recv: aautochan.1,
        })
    }
}

#[cfg(feature = "swupdate")]
struct SwupdateChannel {
    recv: tokio::sync::mpsc::Receiver<uobradio_comms::MessageToSwupdateChannel>,
    send: tokio::sync::mpsc::Sender<uobradio_comms::MessageFromSwupdateChannel>,
}

#[cfg(feature = "swupdate")]
struct SwupdateChannelRecv {
    send: tokio::sync::mpsc::Sender<uobradio_comms::MessageToSwupdateChannel>,
    recv: tokio::sync::mpsc::Receiver<uobradio_comms::MessageFromSwupdateChannel>,
}

#[cfg(feature = "swupdate")]
impl SwupdateChannelRecv {
    async fn process(&mut self, progress: &mut Option<(u8, u8)>) {
        while let Ok(m) = self.recv.try_recv() {
            match m {
                uobradio_comms::MessageFromSwupdateChannel::Ready => {}
                uobradio_comms::MessageFromSwupdateChannel::Progress(step, percent) => {
                    *progress = Some((step, percent));
                }
            }
        }
    }
}

#[cfg(feature = "swupdate")]
impl SwupdateChannel {
    async fn run(&mut self) {
        while let Err(e) = self.iteration().await {
            service::log::error!("The swupdate comms failed: {e}");
        }
    }

    async fn iteration(&mut self) -> Result<(), String> {
        let url = "ws://127.0.0.1:8080/ws";
        service::log::error!("Starting update with {url}");
        match tokio_tungstenite::connect_async(url).await {
            Ok((ws_stream, _)) => {
                use futures_util::StreamExt;
                let (write, mut read) = ws_stream.split();
                service::log::error!("About to read websocket messages");
                while let Some(Ok(message)) = read.next().await {
                    let a = message
                        .to_text()
                        .map(|a| a.to_string())
                        .ok()
                        .map(|m| {
                            let v: Result<serde_json::Value, serde_json::Error> =
                                serde_json::from_str(&m);
                            v.ok()
                        })
                        .flatten();
                    if let Some(v) = a {
                        let b = v.get("type").map(|a| a.as_str()).flatten();
                        match b {
                            Some("step") => {
                                if let Some(step) = v.get("step").map(|a| a.as_str()).flatten() {
                                    if let Ok(step) = step.parse::<u8>() {
                                        if let Some(percent) =
                                            v.get("percent").map(|a| a.as_str()).flatten()
                                        {
                                            service::log::error!(
                                                "The percentage for step {step} is {percent}"
                                            );
                                            if let Ok(p) = percent.parse::<u8>() {
                                                service::log::error!(
                                                    "The percentage for step {step} is {percent}"
                                                );
                                                self.send.send(uobradio_comms::MessageFromSwupdateChannel::Progress(step, p)).await.map_err(|e|e.to_string())?;
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                service::log::error!("Done reading websocket messages");
            }
            Err(e) => {
                service::log::error!("Failed to open websocket {:?}", e);
            }
        }
        Ok(())
    }
}

/// The common data for an app user
pub struct AppUserCommon {
    /// The command line arguments specify any additional options required
    args: Arguments,
    #[cfg(feature = "androidauto")]
    /// The android auto service
    aauto_service: Option<AndroidAutoService>,
    /// The system specific (not user set) settings.
    system: SystemSettings,
    /// The actual outputs for the system
    outputs: Option<SystemOutputs>,
    /// The actual inputs for the system
    inputs: Option<SystemInputs>,
    /// The network details for android auto
    #[cfg(all(feature = "androidauto", feature = "wifi"))]
    aa_network: Option<NetworkInformation>,
    #[cfg(all(feature = "wifi", target_os = "linux"))]
    /// Used for wifi operations
    wifi: Option<nmrs::NetworkManager>,
    #[cfg(feature = "wifi")]
    /// The wifi device
    wifi_device: Option<nmrs::Device>,
    #[cfg(feature = "wifi")]
    /// The wifi setup
    wifi_setup: Option<uobradio_comms::wireless::WifiMode>,
    #[cfg(feature = "bluetooth")]
    /// The main bluetooth struct
    bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
    #[cfg(feature = "bluetooth")]
    /// Used to receive messages to the bluetooth host
    blue_recv: tokio::sync::mpsc::Receiver<bluetooth_rust::MessageToBluetoothHost>,
    #[cfg(feature = "bluetooth")]
    /// Determines who deals with the bluetooth stuff
    blue_addr: Option<std::net::SocketAddr>,
    /// The video sources in the system
    video: Vec<Result<VideoSource, String>>,
    /// The old nonvolatile settings of the radio, used to see if settings should be saved
    old_settings: NonvolatileSettings,
    /// The nonvolatile settings of the radio
    settings: NonvolatileSettings,
    /// The hvac controls
    hvac: HvacController,
    /// The last sensor data
    sensors: Sensors,
    /// The historic sensor data for showing guage history
    historical_sensors: VecDeque<Sensors>,
    /// The number of records to keep
    num_historical_records: usize,
    /// Public hvac data
    hvac_public: uobradio_comms::PublicData,
    #[cfg(feature = "swupdate")]
    /// The communication for the swupdate websocket
    swupdate_channel: SwupdateChannelRecv,
    /// the shutdown sender
    shutdown_send: tokio::sync::broadcast::Sender<()>,
    #[cfg(feature = "androidauto")]
    token: android_auto::AndroidAutoSetup,
    /// The channel for updating the sensor polling thread
    polling_channel: tokio::sync::mpsc::Sender<MessageToSensorPollThread>,
}

async fn send_log_files(
    p: PathBuf,
    streamw: Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Result<(), String> {
    if let Ok(a) = p.read_dir() {
        for f in a {
            if let Ok(f) = f {
                if let Ok(md) = f.metadata() {
                    if md.is_file() {
                        let name = f.file_name();
                        if let Some(ext) = f.path().extension() {
                            if ext.to_str() == Some("csv") {
                                if let Ok(mut f) = std::fs::File::open(f.path()) {
                                    let mut reader = std::io::BufReader::new(f);
                                    let mut buffer = [0_u8; 65536];
                                    loop {
                                        let count =
                                            reader.read(&mut buffer).map_err(|e| e.to_string())?;
                                        if count == 0 {
                                            break;
                                        }
                                        let packet = uobradio_comms::MessageToApp::LogFilePartial(
                                            name.to_str().unwrap().to_string(),
                                            buffer[..count].to_vec(),
                                        );
                                        packet.send_to_stream(&streamw).await?;
                                    }
                                    let packet = uobradio_comms::MessageToApp::LogFileComplete(
                                        name.to_str().unwrap().to_string(),
                                    );
                                    packet.send_to_stream(&streamw).await?;
                                }
                            }
                        }
                    }
                }
            }
        }
        let packet = uobradio_comms::MessageToApp::LogFileCopiesComplete;
        packet.send_to_stream(&streamw).await?;
    }
    Ok(())
}

async fn receive_message_from_app(
    streamr: &mut tokio::net::tcp::OwnedReadHalf,
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
    streamw: Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    #[cfg(any(feature = "androidauto", feature = "bluetooth"))] addr: std::net::SocketAddr,
    #[cfg(feature = "bluetooth")] send_passkey_response: &mut Option<
        tokio::sync::mpsc::Sender<ResponseToPasskey>,
    >,
    progress: &Option<(u8, u8)>,
) -> Result<(), String> {
    #[cfg(feature = "bluetooth")]
    use bluetooth_rust::MessageFromBluetoothHost;
    use std::collections::BTreeMap;
    use tokio::io::AsyncReadExt;
    use uobradio_comms::MessageToApp;
    let length = streamr
        .read_u32()
        .await
        .map_err(|e| format!("Error reading packet length: {}", e))?;
    let mut packet = vec![0; length as usize];
    let mut index = 0;
    while index < length {
        let l = streamr
            .read(&mut packet[index as usize..])
            .await
            .map_err(|e| format!("Error reading packet data: {}", e))?;
        index += l as u32;
    }
    let packet: Result<(uobradio_comms::MessageFromApp, usize), bincode::error::DecodeError> =
        bincode::serde::decode_from_slice(&packet, bincode::config::standard());
    if let Ok((packet, _length)) = packet {
        #[cfg(feature = "bluetooth")]
        {
            let mut common2 = common.lock().await;
            if common2.blue_addr.is_some() {
                while let Ok(m) = common2.blue_recv.try_recv() {
                    match &m {
                        bluetooth_rust::MessageToBluetoothHost::DisplayPasskey(_, sender) => {
                            *send_passkey_response = Some(sender.clone());
                        }
                        bluetooth_rust::MessageToBluetoothHost::ConfirmPasskey(_, sender) => {
                            *send_passkey_response = Some(sender.clone());
                        }
                        bluetooth_rust::MessageToBluetoothHost::CancelDisplayPasskey => {
                            log::info!("Cancel display passkey");
                            send_passkey_response.take();
                        }
                    }
                    let packet = MessageToApp::BluetoothMessage(m.into());
                    packet.send_to_stream(&streamw).await?;
                    log::info!("Sent bluetooth message to bluetooth master");
                }
            }
        }
        match packet {
            uobradio_comms::MessageFromApp::GpioQuery(query) => {
                let val = {
                    let mut c = common.lock().await;
                    use crate::outputs::BoolOutputTrait;
                    if let Some(os) = &mut c.outputs {
                        match query {
                            uobradio_comms::GpioQuery::CameraLedControl(_) => false,
                            uobradio_comms::GpioQuery::LightControl(i) => {
                                let v = os.offroad_lights[i as usize].last_output();
                                v
                            }
                            uobradio_comms::GpioQuery::InverterPower => os.inverter.last_output(),
                            uobradio_comms::GpioQuery::GetAuxInput(i) => false,
                            uobradio_comms::GpioQuery::GetAuxOutput(i) => {
                                use outputs::BoolVecOutputTrait;
                                if let Ok(v) = os.aux_out.query_channel(i) {
                                    v
                                } else {
                                    log::error!("Failed to read aux output {}", i);
                                    false
                                }
                            }
                        }
                    } else {
                        false
                    }
                };
                let packet = uobradio_comms::MessageToApp::GpioQueryResponse(query, val);
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::StartLogCopy => {
                let p = {
                    let c = common.lock().await;
                    let p = c.system.log.base_path.clone();
                    p
                };
                let streamw2 = streamw.clone();
                tokio::spawn(async move {
                    if let Err(e) = send_log_files(p, streamw2).await {
                        log::error!("Error copying usb files: {}", e);
                    }
                });
            }
            uobradio_comms::MessageFromApp::GetHistoricalData => {
                let mut c = common.lock().await;
                let packet =
                    MessageToApp::HistoricalSensorData(c.historical_sensors.clone().into());
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::GetSensorData => {
                let mut c = common.lock().await;
                let packet = MessageToApp::SensorData(c.sensors.clone());
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::Exit => {
                let common2 = common.lock().await;
                return common2
                    .shutdown_send
                    .send(())
                    .map(|_| ())
                    .map_err(|_| "Failed to send shutdown".to_string());
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::ConnectToSavedWifiNetwork(ssid) => {
                let common2 = common.lock().await;
                if let Some(wifi) = &common2.wifi {
                    if let Ok(Some(connection_path)) =
                        wifi.get_saved_connection_path(ssid.as_str()).await
                    {
                        let _ = nmrs_extensions::activate_saved_wifi(&connection_path).await;
                    }
                }
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::ListAllKnownWifiNetworks => {
                let wifi = {
                    let common2 = common.lock().await;
                    common2.wifi.clone()
                };
                let stream2w = streamw.clone();
                if let Some(wifi) = wifi {
                    let wifis = wifi.list_saved_connections().await;
                    match wifis {
                        Ok(list) => {
                            service::log::info!("WIFI NETWORKS: {:?}", list);
                            let mut new_list = Vec::new();
                            for ssid in &list {
                                if let Ok(Some(path)) =
                                    wifi.get_saved_connection_path(ssid.as_str()).await
                                {
                                    if let Ok(true) =
                                        nmrs_extensions::is_wifi_connection(&path).await
                                    {
                                        new_list.push(ssid.to_string());
                                    }
                                }
                            }
                            let packet = MessageToApp::KnownWifiNetworks(new_list);
                            packet.send_to_stream(&stream2w).await?;
                        }
                        Err(e) => {
                            let packet = MessageToApp::FailedToScanForWifiNetworks {
                                reason: e.to_string(),
                            };
                            packet.send_to_stream(&stream2w).await?;
                        }
                    }
                }
            }
            uobradio_comms::MessageFromApp::GetUpdateProgress => {
                if let Some(p) = progress {
                    let packet = MessageToApp::UpdateProgress(p.0, p.1);
                    packet.send_to_stream(&streamw).await?;
                } else {
                    let packet = MessageToApp::NoUpdateInProgress;
                    packet.send_to_stream(&streamw).await?;
                }
            }
            uobradio_comms::MessageFromApp::StartUpdate => {
                service::log::error!("Starting update");
                tokio::task::spawn_blocking(|| {
                    #[cfg(feature = "swupdate")]
                    {
                        if swupdate_ipc::install_swu("/data/update.swu".into()).is_ok() {
                            std::fs::remove_file("/data/update.swu");
                            let mut t = std::process::Command::new("reboot");
                            let _ = t.output();
                        }
                    }
                });
            }
            uobradio_comms::MessageFromApp::DownloadServerFile(url) => {
                let files = reqwest::get(url).await;
                let mut success = true;
                if let Ok(r) = files {
                    let total_size = r.content_length();
                    let mut current_size = 0;
                    use futures_util::StreamExt;
                    let mut a = r.bytes_stream();
                    let fout = std::fs::File::create("/data/update.swu");
                    if let Ok(mut fout) = fout {
                        while let Some(Ok(chunk)) = a.next().await {
                            use std::io::Write;
                            let csize = chunk.len();
                            current_size += csize;
                            if let Some(total) = total_size {
                                let current = std::cmp::min(current_size, total as usize);
                                let percent = current as f32 / total as f32;
                                let packet = MessageToApp::ServerFileDownloadProgress(percent);
                                packet.send_to_stream(&streamw).await?;
                            }
                            if fout.write_all(&chunk).is_err() {
                                success = false;
                                break;
                            }
                        }
                    }
                }
                let packet = MessageToApp::ServerFileDownloadComplete(success);
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::DownloadServerFileList(url) => {
                let files = reqwest::get(url).await;
                match files {
                    Ok(r) => {
                        let mut files_out = Vec::new();
                        if let Ok(list) = r.text().await {
                            for file in list.lines() {
                                files_out.push(file.to_string());
                            }
                        }
                        let packet = MessageToApp::ListOfServerUpdateFiles {
                            files: Ok(files_out),
                        };
                        packet.send_to_stream(&streamw).await?;
                    }
                    Err(e) => {
                        let packet = MessageToApp::ListOfServerUpdateFiles {
                            files: Err(e.to_string()),
                        };
                        packet.send_to_stream(&streamw).await?;
                    }
                }
            }
            uobradio_comms::MessageFromApp::Hvac(c) => {
                let mut common2 = common.lock().await;
                match c {
                    uobradio_comms::HvacControl::SetMode(m) => common2.hvac.set_mode(m),
                    uobradio_comms::HvacControl::GetPublicData => {
                        let packet = MessageToApp::Ac(uobradio_comms::AcResponse::PublicData(
                            common2.hvac_public.clone(),
                        ));
                        packet.send_to_stream(&streamw).await?;
                    }
                    uobradio_comms::HvacControl::SetAcTargetTemperature(t) => {
                        common2.hvac.set_ac_setpoint(t);
                    }
                    uobradio_comms::HvacControl::SetHeatTargetTemperature(t) => {
                        common2.hvac.set_heat_setpoint(t);
                    }
                    uobradio_comms::HvacControl::SetAutoTargetTemperature(t) => {
                        common2.hvac.set_auto_setpoint(t);
                    }
                    uobradio_comms::HvacControl::SetFanSpeed(f) => {
                        common2.hvac.set_fan_speed(f);
                    }
                }
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::ForgetWifiNetwork(ssid) => {
                let common2 = common.lock().await;
                if let Some(nm) = &common2.wifi {
                    let _ = nm.forget(&ssid).await;
                }
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::GetWifiDetails => {
                let common2 = common.lock().await;
                service::log::info!("Wifi mode is {:?}", common2.wifi_setup);
                let mut packet = None;
                if let Some(wifi) = &common2.wifi_setup {
                    match wifi {
                        uobradio_comms::wireless::WifiMode::Hotspot { ssid, password } => {
                            packet = Some(uobradio_comms::MessageToApp::WifiDetails {
                                ssid: ssid.clone(),
                                password: password.clone(),
                            });
                        }
                        uobradio_comms::wireless::WifiMode::RegularNetwork => {
                            if let Some(nm) = &common2.wifi {
                                let n = nm.current_network().await;
                                service::log::info!("Network is {:?}", n);
                                if let Ok(Some(net)) = n {
                                    packet = Some(uobradio_comms::MessageToApp::WifiDetails {
                                        ssid: net.ssid,
                                        password: None,
                                    });
                                }
                            }
                        }
                    }
                }
                let packet = packet.unwrap_or(uobradio_comms::MessageToApp::NoCurrentWifiNetwork);
                packet.send_to_stream(&streamw).await?;
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::ConnectToNetwork { network, password } => {
                let wifi = {
                    let mut common2 = common.lock().await;
                    common2.wifi_setup.take();
                    common2.wifi.clone()
                };
                let common2 = common.clone();
                let stream2w = streamw.clone();
                tokio::task::spawn(async move {
                    if let Some(wifi) = wifi {
                        if network.secured {
                            let ssid = network.ssid.clone();
                            if let Some(p) = password {
                                log::info!("Start connect to wifi {}", ssid);
                                let p2 = p.clone();
                                let a = wifi
                                    .connect(&ssid, nmrs::WifiSecurity::WpaPsk { psk: p2.clone() })
                                    .await;
                                match a {
                                    Ok(_wifi) => {
                                        log::info!("Connected to wifi network {}", ssid);
                                        let mut common2 = common2.lock().await;
                                        common2.wifi_setup = Some(
                                            uobradio_comms::wireless::WifiMode::RegularNetwork,
                                        );
                                        common2.settings.save(&common2.args.nvconfig);
                                        let packet = MessageToApp::ConnectedToWifiNetwork {
                                            ssid,
                                            password: Some(p),
                                        };
                                        packet.send_to_stream(&stream2w).await?;
                                    }
                                    Err(e) => {
                                        log::error!("Error connecting to {}: {:?}", ssid, e);
                                        let packet =
                                            MessageToApp::FailedToConnectToWifiNetwork { ssid };
                                        packet.send_to_stream(&stream2w).await?;
                                    }
                                }
                            } else {
                                log::error!("No password for wifi defined");
                            }
                        }
                    } else {
                        log::error!("No wifi adapter found?");
                    }
                    Ok::<(), String>(())
                });
            }
            #[cfg(feature = "wifi")]
            uobradio_comms::MessageFromApp::ScanForWifiNetworks => {
                let wifi = {
                    let common2 = common.lock().await;
                    common2.wifi.clone()
                };
                let stream2w = streamw.clone();
                if let Some(wifi) = wifi {
                    log::info!("Scanning for wifi networks");
                    let wifis = wifi.scan_networks().await;
                    match wifis {
                        Ok(_) => {
                            let list = wifi.list_networks().await;
                            match list {
                                Ok(list) => {
                                    log::info!("Done scanning for wifi networks");
                                    let packet = MessageToApp::WifiList(list);
                                    packet.send_to_stream(&stream2w).await?;
                                }
                                Err(e) => {
                                    let packet = MessageToApp::FailedToScanForWifiNetworks {
                                        reason: e.to_string(),
                                    };
                                    packet.send_to_stream(&stream2w).await?;
                                }
                            }
                        }
                        Err(e) => {
                            let packet = MessageToApp::FailedToScanForWifiNetworks {
                                reason: e.to_string(),
                            };
                            packet.send_to_stream(&stream2w).await?;
                        }
                    }
                }
            }
            uobradio_comms::MessageFromApp::ExternalRadio(rc) => match rc {
                uobradio_comms::RadioCommand::StartTransmission => {
                    log::info!("Start external radio transmission");
                }
                uobradio_comms::RadioCommand::StopTransmission => {
                    log::info!("Stop external radio transmission");
                }
                uobradio_comms::RadioCommand::TransmissionDataPartial(d) => {
                    log::info!("Process {} bytes of radio transmission data", d.len());
                }
            },
            #[cfg(feature = "androidauto")]
            uobradio_comms::MessageFromApp::AndroidAutoMessage(m) => match m {
                uobradio_comms::aauto::AndroidAutoMessageToPhone::Test => todo!(),
                uobradio_comms::aauto::AndroidAutoMessageToPhone::Message(m) => {
                    let mut common2 = common.lock().await;
                    if let Some(aauto) = &common2.aauto_service {
                        if addr == aauto.addr {
                            if let Err(e) = aauto.sender.send(m).await {
                                let m =
                                    uobradio_comms::aauto::AndroidAutoMessageFromPhone::Disconnect;
                                let packet = MessageToApp::AndroidAutoMessage(m);
                                packet.send_to_stream(&streamw).await?;
                                common2.aauto_service.take();
                                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                common2.aauto_service =
                                    AndroidAutoService::new(&common2, addr).await.ok();
                            }
                        }
                    }
                }
            },
            #[cfg(feature = "androidauto")]
            uobradio_comms::MessageFromApp::RequestAndroidAutoControl => {
                let mut common = common.lock().await;
                let r = if common.aauto_service.is_none() {
                    log::info!("Setting {:?} as android auto master", addr);
                    let r = AndroidAutoService::new(&common, addr).await;
                    if let Err(e) = &r {
                        log::error!("Error starting android auto service: {}", e);
                    }
                    common.aauto_service = r.ok();
                    common.aauto_service.is_some()
                } else {
                    false
                };
                let packet = uobradio_comms::MessageToApp::AndroidAutoHandlerResult(r);
                packet.send_to_stream(&streamw).await?;
            }
            #[cfg(feature = "bluetooth")]
            uobradio_comms::MessageFromApp::BluetoothMessage(m) => {
                let common2 = common.lock().await;
                if Some(addr) == common2.blue_addr {
                    if let MessageFromBluetoothHost::PasskeyMessage(m) = m {
                        if let Some(sender) = &send_passkey_response {
                            if sender.send(m).await.is_err() {
                                let m = uobradio_comms::ActualMessageToBluetoothHost::CancelDisplayPasskey;
                                let packet = MessageToApp::BluetoothMessage(m);
                                packet.send_to_stream(&streamw).await?;
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "bluetooth")]
            uobradio_comms::MessageFromApp::SetBluetoothDiscovery(val) => {
                let common2 = common.lock().await;
                let a = {
                    if let Some(bluetooth) = common2.bluetooth.supports_async() {
                        Some(
                            bluetooth
                                .set_discoverable(true)
                                .await
                                .expect("Failed to make bluetooth discoverable"),
                        )
                    } else {
                        None
                    }
                };
                if let Some(_) = a {
                    let a = uobradio_comms::ActualMessageToBluetoothHost::BluetoothEnabled(val);
                    let packet = uobradio_comms::MessageToApp::BluetoothMessage(a);
                    packet.send_to_stream(&streamw).await?;
                    log::info!("Set bluetooth discoverable success");
                } else {
                    log::error!("Failed to change bluetooth discoverable to {}", val);
                }
            }
            #[cfg(feature = "bluetooth")]
            uobradio_comms::MessageFromApp::RequestBluetoothControl => {
                let mut common = common.lock().await;
                let r = if common.blue_addr.is_none() {
                    log::info!("Setting {:?} as bluetooth master", addr);
                    common.blue_addr = Some(addr);
                    true
                } else {
                    false
                };
                let packet = uobradio_comms::MessageToApp::BluetoothHandlerResult(r);
                log::info!("Sending bluetooth response {:?}", packet);
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::RequestSettings => {
                let common2 = common.lock().await;
                log::info!("Sending settings to user: {:?}", common2.settings);
                let packet = uobradio_comms::MessageToApp::NewSettings(common2.settings.clone());
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::NewSettings {
                settings,
                #[cfg(feature = "wifi")]
                wifi_reconnect,
            } => {
                let mut common2 = common.lock().await;
                let _ = common2
                    .polling_channel
                    .send(MessageToSensorPollThread::NewLogInterval(
                        settings.logging_interval_seconds,
                    ))
                    .await;
                common2.settings = settings;
                log::info!("Saving nonvolatile config to {:?}", common2.args.nvconfig);
                common2.settings.save(&common2.args.nvconfig);
                #[cfg(feature = "wifi")]
                {
                    let mut wifi_changed = false;
                    if common2.old_settings.wifi_config.hotspot_configuration
                        != common2.settings.wifi_config.hotspot_configuration
                    {
                        common2.old_settings.wifi_config.hotspot_configuration =
                            common2.settings.wifi_config.hotspot_configuration.clone();
                        wifi_changed = true;
                    }
                    if common2.old_settings.wifi_config != common2.settings.wifi_config {
                        common2.old_settings.wifi_config = common2.settings.wifi_config.clone();
                        wifi_changed = true;
                    }
                    if wifi_changed {
                        setup_wifi(common2).await;
                    }
                }
            }
            uobradio_comms::MessageFromApp::CameraSettingControl(id, control, data) => {
                let a: uobradio_comms::v4l::control::Value = data.into();
                let mut common2 = common.lock().await;
                if let Some(Ok(vid)) = common2.video.get_mut(id as usize) {
                    let _ = vid.send_update(control as usize, &a);
                    vid.controls[control as usize].value = a;
                }
            }
            uobradio_comms::MessageFromApp::RequestCameras => {
                let common2 = common.lock().await;
                let mut map = BTreeMap::new();
                for (i, cam) in common2.video.iter().enumerate() {
                    if let Ok(cam) = cam {
                        if let Some(c) = cam.sendable() {
                            map.insert(i as u8, c);
                        }
                    }
                }
                let packet = uobradio_comms::MessageToApp::CamerasBtreeMap(map);
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::Ping(id) => {
                let packet = uobradio_comms::MessageToApp::PingReply(id);
                packet.send_to_stream(&streamw).await?;
            }
            uobradio_comms::MessageFromApp::RequestCamera(index) => {
                let common2 = common.lock().await;
                if let Some(Ok(v)) = common2.video.get(index as usize) {
                    let jpeg = {
                        let frame = v.image.lock().unwrap();
                        frame.get_jpeg()
                    };
                    let response = uobradio_comms::MessageToApp::CameraDataJpeg(index, jpeg);
                    response.send_to_stream(&streamw).await?;
                }
            }
            uobradio_comms::MessageFromApp::GpioControl(gpio) => {
                {
                    let mut common2 = common.lock().await;
                    if let Some(os) = &mut common2.outputs {
                        match gpio {
                            uobradio_comms::Gpio::AuxOutput(id, v) => {
                                log::info!("Set aux output {} to {}", id, v);
                                use outputs::BoolVecOutputTrait;
                                os.aux_out.set_channel(id, v);
                            }
                            uobradio_comms::Gpio::GetAuxInput(id) => {
                                log::error!("NONSENSICAL Request for aux input control {}", id);
                            }
                            uobradio_comms::Gpio::InverterPower(p) => {
                                log::info!("Set inverter power to {}", p);
                                use outputs::BoolOutputTrait;
                                os.inverter.output(p);
                            }
                            uobradio_comms::Gpio::LightControl(id, v) => {
                                log::info!("Set light output {} to {}", id, v);
                                use outputs::BoolOutputTrait;
                                os.offroad_lights[id as usize].output(v);
                            }
                            uobradio_comms::Gpio::WinchControl(f, r) => {
                                log::info!("Winch control {} {}", f, r);
                                use outputs::BoolVecOutputTrait;
                                os.winch.output(&[f, r]);
                            }
                            uobradio_comms::Gpio::CameraLedControl(i, s) => {
                                log::info!("Camera led {} to {}", i, s);
                                use outputs::BoolVecOutputTrait;
                                os.camera_leds.set_channel(i, s);
                            }
                            uobradio_comms::Gpio::LockDoors => {
                                log::info!("Received request to lock all doors");
                                let common3 = common.clone();
                                tokio::spawn(async move {
                                    use outputs::BoolOutputTrait;
                                    {
                                        let mut c = common3.lock().await;
                                        if let Some(os) = &mut c.outputs {
                                            os.door_lock.output(true);
                                        }
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(1000))
                                        .await;
                                    {
                                        let mut c = common3.lock().await;
                                        if let Some(os) = &mut c.outputs {
                                            os.door_lock.output(true);
                                        }
                                    }
                                });
                            }
                            uobradio_comms::Gpio::UnlockDoors => {
                                log::info!("Recieved request to unlock all doors");
                                let common3 = common.clone();
                                tokio::spawn(async move {
                                    use outputs::BoolOutputTrait;
                                    {
                                        let mut c = common3.lock().await;
                                        if let Some(os) = &mut c.outputs {
                                            os.door_unlock.output(true);
                                        }
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(1000))
                                        .await;
                                    {
                                        let mut c = common3.lock().await;
                                        if let Some(os) = &mut c.outputs {
                                            os.door_unlock.output(true);
                                        }
                                    }
                                });
                            }
                            uobradio_comms::Gpio::WindowControl { id, up, down } => {
                                use outputs::BoolVecOutputTrait;
                                log::info!("Window {} {}/{}", id, up, down);
                                if let Some(w) = os.windows.get_mut(id as usize) {
                                    w.output(&[up, down]);
                                }
                            }
                        }
                    }
                }
                let response = uobradio_comms::MessageToApp::GpioConfirmation(gpio);
                response.send_to_stream(&streamw).await?;
            }
        }
        #[cfg(feature = "androidauto")]
        {
            let mut common2 = common.lock().await;
            if let Some(aauto) = &mut common2.aauto_service {
                while let Ok(m) = aauto.recv.try_recv() {
                    let packet = MessageToApp::AndroidAutoMessage(m);
                    packet.send_to_stream(&streamw).await?;
                }
            }
        }
    } else {
        log::error!("Failed to process packet");
        return Err("Received bad packet".to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
/// Processes a tcp connection from an app
pub async fn process_app(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
) -> Result<(), String> {
    log::info!("Processing an app at {:?}", addr);

    let (mut streamr, streamw) = stream.into_split();

    let streamw = std::sync::Arc::new(tokio::sync::Mutex::new(streamw));

    #[cfg(feature = "bluetooth")]
    let mut send_passkey_response = None;

    #[cfg(feature = "swupdate")]
    let mut progress: Option<(u8, u8)> = None;
    #[cfg(not(feature = "swupdate"))]
    let progress: Option<(u8, u8)> = None;

    loop {
        #[cfg(feature = "swupdate")]
        {
            let mut c = common.lock().await;
            c.swupdate_channel.process(&mut progress).await;
        }
        receive_message_from_app(
            &mut streamr,
            common.clone(),
            streamw.clone(),
            #[cfg(any(feature = "androidauto", feature = "bluetooth"))]
            addr,
            #[cfg(feature = "bluetooth")]
            &mut send_passkey_response,
            &progress,
        )
        .await?;
    }
}

/// Start the udp listener, responsible for making a radio discoverable on the network.
async fn udp_listener(_common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:13456").await.unwrap();
    log::info!("Starting radio listener");
    let mut response = vec![0; 1500];
    loop {
        log::info!("Waiting for a udp client");
        while let Ok((n, addr)) = socket.recv_from(&mut response).await {
            log::info!("Got request from {:?} {} {:x?}", addr, n, &response[0..n]);
            let packet =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((packet, _len)) = packet {
                if let uobradio_comms::MessageFromApp::Ping(val) = packet {
                    log::info!("got ping packet {}", val);
                    let response = bincode::serde::encode_to_vec(
                        uobradio_comms::MessageToApp::PingReply(13457),
                        bincode::config::standard(),
                    )
                    .unwrap();
                    let _ = socket.send_to(&response, addr).await;
                }
            } else {
                log::info!("invalid packet received {:x?}", response);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct SensorLogConfig {
    base_path: PathBuf,
}

impl Default for SensorLogConfig {
    fn default() -> Self {
        SensorLogConfig {
            base_path: "/tmp".into(),
        }
    }
}

impl SensorLogConfig {
    /// Start the sensor log
    pub async fn start_log(&mut self) -> Result<SensorLog, String> {
        let mut p = self.base_path.clone();
        p.push("index");
        let i: u32 = {
            match std::fs::File::options().read(true).write(true).open(&p) {
                Ok(mut f) => {
                    let mut contents = String::new();
                    f.read_to_string(&mut contents).map_err(|e| e.to_string())?;
                    let i = contents
                        .parse()
                        .map_err(|e: std::num::ParseIntError| e.to_string())?;
                    let j: u32 = i + 1;
                    f.rewind().map_err(|e| e.to_string())?;
                    f.write_all(j.to_string().as_bytes())
                        .map_err(|e| e.to_string())?;
                    i
                }
                Err(_e) => {
                    let mut f = std::fs::File::create_new(p).map_err(|e| e.to_string())?;
                    f.write_all("1".as_bytes()).map_err(|e| e.to_string())?;
                    0
                }
            }
        };
        let mut p2 = self.base_path.clone();
        p2.push(format!("{}.csv", i));
        SensorLog::new(p2)
    }
}

struct SensorLog {
    w: csv::Writer<std::fs::File>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SensorLogRecord {
    #[serde(with = "chrono::serde::ts_seconds")]
    time: DateTime<chrono::Utc>,
    humidity: f32,
    cabin_temp: f32,
    vent_temp: f32,
    /// Orientation of the system, left-right, in degrees
    pub orientx: f32,
    /// Orientation of the system, forwards-backwards, in degrees
    pub orienty: f32,
    /// The engine coolant temperature
    pub engine_coolant_temp: f32,
    /// The engine oil temperature
    pub engine_oil_temp: f32,
    /// The engine exhaust temperature
    pub engine_exhaust_temp: f32,
    /// Intake air temperatue for the engine
    pub intake_air_temperature: f32,
    /// Front differential temperature
    pub front_diff_temp: f32,
    /// Rear differential temperature
    pub rear_diff_temp: f32,
    /// The engine oil pressure (psi)
    pub engine_oil_pressure: f32,
    /// The coolant pressure (psi)
    pub coolant_pressure: f32,
    /// Transmission temperature
    pub trans_temp: f32,
    /// Transfer case temperature
    pub transfer_temp: f32,
    /// Door open sensor
    pub door_open: bool,
    /// Engine rpm sensor
    pub engine_rpm: u16,
    /// Main system voltage
    pub main_voltage: f32,
}

impl SensorLog {
    pub fn new(f: PathBuf) -> Result<Self, String> {
        Ok(Self {
            w: csv::WriterBuilder::new()
                .terminator(csv::Terminator::CRLF)
                .from_path(f)
                .map_err(|e| e.to_string())?,
        })
    }

    /// Add a row of log data
    pub async fn log_entry(&mut self, hvac: uobradio_comms::PublicData, sensors: Sensors) {
        let record = SensorLogRecord {
            time: chrono::Utc::now(),
            humidity: hvac.humidity.unwrap_or_default(),
            cabin_temp: hvac.cabin_temperature.unwrap_or_default(),
            vent_temp: hvac.hvac_vent_temperature,
            orientx: sensors
                .orientation
                .clone()
                .unwrap_or(InclinometerOrientation { x: 0.0, y: 0.0 })
                .x,
            orienty: sensors
                .orientation
                .clone()
                .unwrap_or(InclinometerOrientation { x: 0.0, y: 0.0 })
                .y,
            engine_coolant_temp: sensors.engine_coolant_temp.unwrap_or_default(),
            engine_oil_temp: sensors.engine_oil_temp.unwrap_or_default(),
            engine_exhaust_temp: sensors.engine_exhaust_temp.unwrap_or_default(),
            intake_air_temperature: sensors.intake_air_temperature.unwrap_or_default(),
            front_diff_temp: sensors.front_diff_temp.unwrap_or_default(),
            rear_diff_temp: sensors.rear_diff_temp.unwrap_or_default(),
            engine_oil_pressure: sensors.engine_oil_pressure.unwrap_or_default(),
            coolant_pressure: sensors.coolant_pressure.unwrap_or_default(),
            trans_temp: sensors.trans_temp.unwrap_or_default(),
            transfer_temp: sensors.transfer_temp.unwrap_or_default(),
            door_open: sensors.door_open.unwrap_or_default(),
            engine_rpm: sensors.engine_rpm.unwrap_or_default(),
            main_voltage: sensors.main_voltage.unwrap_or_default(),
        };
        self.w.serialize(record);
        self.w.flush();
    }
}

enum MessageToSensorPollThread {
    RestartLog,
    NewLogInterval(u8),
}

#[cfg(feature = "bluetooth")]
/// runs the bluetooth stuff for the service
async fn bluetooth_task(
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
    mut kill: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), String> {
    let b = {
        let common2 = common.lock().await;
        common2.bluetooth.clone()
    };
    let mut notifications = tokio::sync::mpsc::channel(5);
    start_mns(&b, 17, notifications.0).await?;
    tokio::spawn(async move {
        log::info!("Running mas code now");
        obex_main(&b).await;
    });
    while let Some(m) = notifications.1.recv().await {
        log::info!("Received notification : {:?}", m);
    }
    Ok(())
}

/// Polls the sensors in the system
async fn sensor_polling(
    common: Arc<tokio::sync::Mutex<AppUserCommon>>,
    mut kill: tokio::sync::broadcast::Receiver<()>,
    mut recv: tokio::sync::mpsc::Receiver<MessageToSensorPollThread>,
) -> Result<(), String> {
    let mut interval_fast = tokio::time::interval(std::time::Duration::from_millis(100));
    let mut interval_1s = tokio::time::interval(std::time::Duration::from_millis(1000));
    let mut interval_log_time = {
        let c = common.lock().await;
        c.settings.logging_interval_seconds as u64 * 1000
    };
    let mut interval_logging =
        tokio::time::interval(std::time::Duration::from_millis(interval_log_time));
    let mut interval_1500ms = tokio::time::interval(std::time::Duration::from_millis(1500));

    let mut logger = None;
    {
        let mut c = common.lock().await;
        if let Ok(log) = c.system.log.start_log().await {
            logger = Some(log);
        }
    }
    loop {
        tokio::select! {
            Some(m) = recv.recv() => {
                match m {
                    MessageToSensorPollThread::RestartLog => {
                        let mut c = common.lock().await;
                        if let Ok(log) = c.system.log.start_log().await {
                            logger = Some(log);
                        }
                    }
                    MessageToSensorPollThread::NewLogInterval(i) => {
                        if i as u64 != interval_log_time {
                            interval_log_time = i as u64 * 1000;
                            interval_logging = tokio::time::interval(std::time::Duration::from_millis(interval_log_time));
                        }
                    }
                }
            }
            _ = interval_fast.tick() => {
                let mut c = common.lock().await;
                if let Some(inputs) = &mut c.inputs {
                    match inputs.get_sensor_data() {
                        Ok(s) => {
                            use crate::outputs::F32OutputTrait;
                            if let Some(p) = s.engine_oil_pressure {
                                if let Some(os) = &mut c.outputs {
                                    if let Err(e) = os.gauge_oil_pressure.output(p) {
                                        log::error!("Error writing oil pressure gauge: {}", e);
                                    }
                                }
                            }
                            if let Some(p) = s.engine_coolant_temp {
                                if let Some(os) = &mut c.outputs {
                                    if let Err(e) = os.gauge_engine_temp.output(p) {
                                        log::error!("Error writing engine temp gauge: {}", e);
                                    }
                                }
                            }
                            if let Some(p) = s.engine_rpm {
                                if let Some(os) = &mut c.outputs {
                                    if let Err(e) = os.gauge_tachometer.output(p as f32) {
                                        log::error!("Error writing tachometer gauge: {}", e);
                                    }
                                }
                            }
                            c.sensors = s;
                        }
                        Err(e) => log::error!("Error getting sensor data: {}", e),
                    }
                }
                let c2 = c.sensors.clone();
                c.historical_sensors.push_back(c2);
                if c.historical_sensors.len() > c.num_historical_records {
                    c.historical_sensors.pop_front();
                }
            }
            _ = interval_logging.tick() => {
                if let Some(log) = &mut logger {
                    let c = common.lock().await;
                    log.log_entry(c.hvac_public.clone(), c.sensors.clone()).await;
                }
            }
            _ = interval_1s.tick() => {
                use crate::sensors::TemperatureSensorTrait;
                 use crate::outputs::BoolVecOutputTrait;
                let mut c = common.lock().await;
                let (cabin, vent) = if let Some(ins) = &mut c.inputs {
                    let a = ins.main_cabin_temperature_sensor.poll();
                    let b = ins.hvac_vent_temperature_sensor.poll();
                    (a, b)
                } else {
                    (Err("No inputs".to_string()), Err("No inputs".to_string()))
                };
                if let Ok(cabin) = cabin {
                    c.hvac.set_cabin_temperature(cabin.fahrenheit());
                }
                if let Ok(vent) = vent {
                    c.hvac.set_hvac_vent_temperature(vent.fahrenheit());
                }

                c.hvac_public = c.hvac.get_public_data();

                c.hvac.run_controls();
                let fan_duty = c.hvac.get_fan_speed();
                let fan_duty = fan_duty as f32 / 255.0;
                let fan_output = if fan_duty < 0.05 {
                    [false, false, false]
                } else if fan_duty < 1.0 / 3.0 {
                    [true, false, false]
                } else if fan_duty < 2.0 / 3.0 {
                    [false, true, false]
                } else {
                    [false, false, true]
                };
                if let Some(os) = &mut c.outputs {
                    if let Err(e) = os.hvac_fan_output.output(&fan_output) {
                        log::error!("Failed to write fan output: {}", e);
                    }
                }
                //log::info!("Fan speed: {}", fan_duty);
                let ac_duty = c.hvac.get_ac_compressor_duty_cycle();
                //log::info!("AC Compressor duty: {:02}", ac_duty * 100.0);
                let heat_duty = c.hvac.get_heat_control_duty_cycle();
                //log::info!("Heat duty cycle: {:02}", heat_duty * 100.0);
            }
            _ = interval_1500ms.tick() => {
            }
            _ = kill.recv() => {
                log::info!("Stopping sensor polling");
                break Ok(());
            }
        }
    }
}

/// Run the tcp listener for a radio, reporting an error if anything went wrong setting up the service
async fn tcp_listener(common: Arc<tokio::sync::Mutex<AppUserCommon>>) -> Result<(), String> {
    let tcp = tokio::net::TcpListener::bind("0.0.0.0:13457").await;
    if let Ok(tcp) = tcp {
        loop {
            log::info!("Waiting for a tcp client");
            if let Ok((stream, addr)) = tcp.accept().await {
                let common2 = common.clone();
                std::thread::spawn(move || {
                    //this is required becuase some of the nmrs futures are not send
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    let local = tokio::task::LocalSet::new();

                    rt.block_on(local.run_until(async move {
                        let a = tokio::task::spawn_local(async move {
                            log::info!("Got a tcp client {:?}", addr);
                            let r = process_app(stream, addr, common2.clone()).await;
                            #[cfg(any(feature = "androidauto", feature = "bluetooth"))]
                            let mut common3 = common2.lock().await;
                            #[cfg(feature = "bluetooth")]
                            if Some(addr) == common3.blue_addr {
                                log::info!("Setting {:?} as no longer the bluetooth master", addr);
                                common3.blue_addr.take();
                            }
                            #[cfg(feature = "androidauto")]
                            if let Some(aauto) = &common3.aauto_service {
                                if addr == aauto.addr {
                                    log::info!(
                                        "Setting {:?} as no longer the android auto master",
                                        addr
                                    );
                                    common3.aauto_service.take();
                                }
                            }
                            log::info!("Completed handling user {:?}", r);
                            r
                        })
                        .await
                        .unwrap();
                        log::error!("tcp finished with {:?}", a);
                    }));
                });
            }
        }
    } else {
        panic!("Unable to open tcp listener to listen for apps connecting");
    }
}

#[cfg(feature = "androidauto")]
/// An internally used structure for sending messages between the android auto user and the frontend
struct InternalAndroidAutoStuff {
    /// Used internally to relay android auto messages from the users phone
    sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
    /// Temporary storage for the android auto crate to use to send us messages
    recvr: Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>>,
    /// Used for sending responses to the android auto crate
    frame_sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
}

#[cfg(feature = "androidauto")]
/// Stores communication links for android auto
#[derive(Clone)]
struct AndroidAutoStuff {
    /// The protected internals
    inner: Arc<tokio::sync::Mutex<InternalAndroidAutoStuff>>,
    #[cfg(feature = "bluetooth")]
    /// The bluetooth reference
    bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
    #[cfg(feature = "bluetooth")]
    /// This is defined if there is actually a bluetooth adapter present
    bluetooth_config: Option<android_auto::BluetoothInformation>,
    /// The network information
    #[cfg(feature = "wifi")]
    network: Arc<android_auto::NetworkInformation>,
    /// The input channel config
    input_config: android_auto::InputConfiguration,
    /// The video channel config
    video_config: android_auto::VideoConfiguration,
    /// The sensors config
    sensors: android_auto::SensorInformation,
}

#[cfg(feature = "androidauto")]
impl AndroidAutoStuff {
    /// Construct a new self
    /// #Arguments
    /// sendr: The channel for sending responses from the user device to the android auto handler
    /// recvr: The channel that receives android auto messages to be sent back to the phone
    /// frame_sender: The channel that sends android auto messages to the user device
    /// bluetooth: The bluetooth adapter to use
    /// network: The network details to use for android auto
    /// bluetooth_config: Contains the bluetooth configuration
    pub fn new(
        sendr: tokio::sync::mpsc::Sender<uobradio_comms::aauto::AndroidAutoMessageFromPhone>,
        recvr: tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>,
        frame_sender: tokio::sync::mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
        #[cfg(feature = "bluetooth")] bluetooth: Arc<bluetooth_rust::BluetoothAdapter>,
        #[cfg(feature = "wifi")] network: android_auto::NetworkInformation,
        #[cfg(feature = "bluetooth")] bluetooth_config: Option<android_auto::BluetoothInformation>,
    ) -> Self {
        let inner = InternalAndroidAutoStuff {
            sendr,
            recvr: Some(recvr),
            frame_sender,
        };
        let mut s = HashSet::new();
        s.insert(android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS);
        s.insert(android_auto::Wifi::sensor_type::Enum::NIGHT_DATA);
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            #[cfg(feature = "bluetooth")]
            bluetooth,
            #[cfg(feature = "wifi")]
            network: Arc::new(network),
            input_config: android_auto::InputConfiguration {
                touchscreen: Some((800, 480)),
                keycodes: vec![1, 2, 3, 4, 5],
            },
            video_config: android_auto::VideoConfiguration {
                resolution: android_auto::Wifi::video_resolution::Enum::_480p,
                fps: android_auto::Wifi::video_fps::Enum::_30,
                dpi: 111,
            },
            sensors: android_auto::SensorInformation { sensors: s },
            #[cfg(feature = "bluetooth")]
            bluetooth_config,
        }
    }
}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoAudioOutputTrait for AndroidAutoStuff {
    async fn open_output_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelOpen(t))
            .await;
        Ok(())
    }

    async fn close_output_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelClose(t))
            .await;
        Ok(())
    }

    async fn receive_output_audio(&self, t: android_auto::AudioChannelType, data: Vec<u8>) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioContent(t, data))
            .await;
    }

    async fn start_output_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelStart(t))
            .await;
    }

    async fn stop_output_audio(&self, t: android_auto::AudioChannelType) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::AudioChannelStop(t))
            .await;
    }
}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoInputChannelTrait for AndroidAutoStuff {
    async fn binding_request(&self, _code: u32) -> Result<(), ()> {
        Ok(())
    }

    fn retrieve_input_configuration(&self) -> &android_auto::InputConfiguration {
        &self.input_config
    }
}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoAudioInputTrait for AndroidAutoStuff {
    async fn open_input_channel(&self) -> Result<(), ()> {
        Ok(())
    }
    async fn audio_input_ack(&self, chan: u8, ack: android_auto::Wifi::AVMediaAckIndication) {}

    async fn close_input_channel(&self) -> Result<(), ()> {
        Ok(())
    }
    async fn start_input_audio(&self) {
        log::error!("Start audio input channel");
    }
    async fn stop_input_audio(&self) {
        log::error!("Stop audio input channel");
    }
}

#[cfg(all(feature = "androidauto", feature = "wifi"))]
#[async_trait::async_trait]
impl android_auto::AndroidAutoWirelessTrait for AndroidAutoStuff {
    async fn setup_bluetooth_profile(
        &self,
        suggestions: &bluetooth_rust::BluetoothRfcommProfileSettings,
    ) -> Result<bluetooth_rust::BluetoothRfcommProfileAsync, String> {
        if let Some(b) = self.bluetooth.supports_async() {
            b.register_rfcomm_profile(suggestions.clone()).await
        } else {
            Err("Async not supported".to_string())
        }
    }

    fn get_wifi_details(&self) -> android_auto::NetworkInformation {
        self.network.as_ref().to_owned()
    }
}

#[cfg(all(feature = "androidauto", feature = "usb"))]
#[async_trait::async_trait]
impl android_auto::AndroidAutoWiredTrait for AndroidAutoStuff {}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoMainTrait for AndroidAutoStuff {
    #[cfg(feature = "bluetooth")]
    fn supports_bluetooth(&self) -> Option<&dyn android_auto::AndroidAutoBluetoothTrait> {
        if self.bluetooth_config.is_some() {
            Some(self)
        } else {
            None
        }
    }

    #[cfg(feature = "wifi")]
    fn supports_wireless(&self) -> Option<Arc<dyn android_auto::AndroidAutoWirelessTrait>> {
        Some(Arc::new(self.clone()))
    }

    #[cfg(feature = "usb")]
    fn supports_wired(&self) -> Option<Arc<dyn android_auto::AndroidAutoWiredTrait>> {
        Some(Arc::new(self.clone()))
    }

    async fn get_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>> {
        let mut s = self.inner.lock().await;
        s.recvr.take()
    }

    async fn connect(&self) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::Connect).await;
    }

    async fn disconnect(&self) {
        let s = self.inner.lock().await;
        let _ = s.sendr.send(AndroidAutoMessageFromPhone::Disconnect).await;
    }
}

#[cfg(all(feature = "androidauto", feature = "bluetooth"))]
#[async_trait::async_trait]
impl android_auto::AndroidAutoBluetoothTrait for AndroidAutoStuff {
    async fn do_stuff(&self) {}
    /// This is probably fine because the supports_bluetooth function already checked this
    /// Removing the bluetooth adapter while the code is running might be problematic here
    fn get_config(&self) -> &android_auto::BluetoothInformation {
        self.bluetooth_config.as_ref().unwrap()
    }
}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoSensorTrait for AndroidAutoStuff {
    fn get_supported_sensors(&self) -> &android_auto::SensorInformation {
        &self.sensors
    }

    async fn start_sensor(&self, stype: android_auto::Wifi::sensor_type::Enum) -> Result<(), ()> {
        if self.sensors.sensors.contains(&stype) {
            let mut m3 = android_auto::Wifi::SensorEventIndication::new();
            match stype {
                android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS => {
                    let mut ds = android_auto::Wifi::DrivingStatus::new();
                    ds.set_status(android_auto::Wifi::DrivingStatusEnum::UNRESTRICTED as i32);
                    m3.driving_status.push(ds);
                }
                android_auto::Wifi::sensor_type::Enum::NIGHT_DATA => {
                    let mut ds = android_auto::Wifi::NightMode::new();
                    ds.set_is_night(false);
                    m3.night_mode.push(ds);
                }
                _ => {
                    todo!();
                }
            }
            let s = self.inner.lock().await;
            let m = android_auto::AndroidAutoMessage::Sensor(m3);
            s.frame_sender.send(m.sendable()).await.map_err(|_| ())?;
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(feature = "androidauto")]
#[async_trait::async_trait]
impl android_auto::AndroidAutoVideoChannelTrait for AndroidAutoStuff {
    async fn receive_video(&self, data: Vec<u8>, _timestamp: Option<u64>) {
        let s = self.inner.lock().await;
        let _ = s
            .sendr
            .send(AndroidAutoMessageFromPhone::VideoContent(data))
            .await;
    }

    async fn setup_video(&self) -> Result<(), ()> {
        Ok(())
    }

    async fn teardown_video(&self) {}

    async fn wait_for_focus(&self) {}

    async fn set_focus(&self, _focus: bool) {}

    fn retrieve_video_configuration(&self) -> &android_auto::VideoConfiguration {
        &self.video_config
    }
}

#[cfg(feature = "wifi")]
/// Returns the first wifi interface found on the system
async fn get_wifi_interface(nmrs: &nmrs::NetworkManager) -> Option<nmrs::Device> {
    if let Ok(devs) = nmrs.list_wireless_devices().await {
        for dev in devs {
            if dev.device_type == nmrs::DeviceType::Wifi {
                service::log::info!("Found wifi device {:?}", dev);
                return Some(dev);
            }
        }
    }
    None
}

#[cfg(feature = "wifi")]
/// Sets up the wifi hardware according to settings
/// Call this when initially setting up wifi, or when changing wifi settings
/// Wifi connections will likey drop and reconnect
async fn setup_wifi(mut common2: tokio::sync::MutexGuard<'_, AppUserCommon>) {
    log::info!("Setup wifi with {:?}", common2.settings.wifi_config.config);
    {
        if let Some(wifi) = &common2.wifi {
            let _ = wifi
                .set_wifi_enabled(!matches!(
                    common2.settings.wifi_config.config,
                    WifiConfig::Disabled
                ))
                .await;
        }
    }
    if matches!(common2.settings.wifi_config.config, WifiConfig::Hotspot) {
        if let Some(wd) = &common2.wifi_device {
            let wifi_dev_path = wd.path.clone();
            if nmrs_extensions::start_hotspot(
                common2.settings.wifi_config.hotspot_configuration.0.clone(),
                common2.settings.wifi_config.hotspot_configuration.1.clone(),
                &wifi_dev_path,
            )
            .await
            .is_ok()
            {
                common2.wifi_setup = Some(uobradio_comms::wireless::WifiMode::Hotspot {
                    ssid: common2.settings.wifi_config.hotspot_configuration.0.clone(),
                    password: Some(common2.settings.wifi_config.hotspot_configuration.1.clone()),
                });
            }
        }
    } else {
        if let Some(nm) = &common2.wifi {
            let _ = nm
                .forget(&common2.settings.wifi_config.hotspot_configuration.0)
                .await;
        }
    }
    match &common2.settings.wifi_config.config {
        WifiConfig::Ready => {
            common2.wifi_setup = Some(uobradio_comms::wireless::WifiMode::RegularNetwork);
        }
        WifiConfig::Hotspot => {}
        WifiConfig::RegularNetwork => {
            common2.wifi_setup = Some(uobradio_comms::wireless::WifiMode::RegularNetwork);
        }
        WifiConfig::Disabled => {
            common2.wifi_setup.take();
        }
    }
}

/// Run the main obex code on all paired devices
pub async fn obex_main(adapter: &bluetooth_rust::BluetoothAdapter) -> Result<(), String> {
    if let Some(a) = adapter.supports_async() {
        if let Some(devs) = a.get_paired_devices() {
            for mut dev in devs {
                use bluetooth_rust::BluetoothDeviceTrait;
                log::info!("Connect to {:?}", dev.get_address());
                let a = connect_to_mas(adapter, dev).await;
                log::info!("Result of connect: {:?}", a);
            }
        }
    }
    log::info!("All devices processed, waiting for MNS connections...");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// The main function for the service
async fn smain() {
    #[cfg(target_family = "windows")]
    {
        std::panic::set_hook(Box::new(|p| {
            service::log::debug!("Panic {:?}", p);
        }))
    }

    let args = <Arguments as clap::Parser>::parse();
    #[cfg(feature = "androidauto")]
    let token = android_auto::setup();

    let f = tokio::fs::File::open("./service.toml").await;
    let settings = if let Ok(mut f) = f {
        let mut config_raw = Vec::new();
        f.read_to_end(&mut config_raw).await.unwrap();
        let config_str = String::from_utf8(config_raw).unwrap();
        toml::from_str(&config_str).unwrap()
    } else {
        MainConfiguration::default()
    };

    let (shutdown_send, mut shutdown_recv) = tokio::sync::broadcast::channel::<()>(1);

    let mut vs = Vec::new();
    if let Ok(d) = uobradio_comms::v4l::Device::new(0) {
        vs.push(video_service::Video::video_start(d));
    }
    let s = NonvolatileSettings::load(&args.nvconfig);
    let sys = SystemSettings::load();
    let outputs = sys.get_outputs();
    let inputs = sys.get_inputs();
    if let Err(e) = &outputs {
        log::error!("Failed to build outputs: {}", e);
    }
    let outputs = outputs.ok();
    if let Err(e) = &inputs {
        log::error!("Failed to build inputs: {}", e);
    }
    let inputs = inputs.ok();
    #[cfg(feature = "bluetooth")]
    let (bluechan, bluetooth) = {
        let bluechan = tokio::sync::mpsc::channel(5);
        let mut bluetooth = bluetooth_rust::BluetoothAdapterBuilder::new();
        bluetooth.with_sender(bluechan.0);
        service::log::info!("Building bluetooth object 3");
        let bluetooth = Arc::new(
            bluetooth
                .async_build()
                .await
                .expect("Could not open bluetooth"),
        );
        service::log::info!("Building bluetooth object success");
        (bluechan.1, bluetooth)
    };

    #[cfg(feature = "wifi")]
    let wifi = nmrs::NetworkManager::new().await.ok();

    #[cfg(all(feature = "wifi", feature = "androidauto"))]
    let mut network = android_auto::NetworkInformation {
        ssid: s.wifi_config.hotspot_configuration.0.clone(),
        psk: s.wifi_config.hotspot_configuration.1.clone(),
        mac_addr: String::new(), //to be populated later
        ip: "10.42.0.1".to_string(),
        port: 5277,
        security_mode: android_auto::Bluetooth::SecurityMode::WPA2_PERSONAL,
        ap_type: android_auto::Bluetooth::AccessPointType::STATIC,
    };

    #[cfg(feature = "wifi")]
    let mut wifi_device = None;
    #[cfg(feature = "wifi")]
    if let Some(wifi) = &wifi {
        if let Some(dev) = get_wifi_interface(wifi).await {
            #[cfg(feature = "androidauto")]
            {
                network.mac_addr = dev.identity.current_mac.clone();
            }
            wifi_device = Some(dev);
        }
    }

    #[cfg(feature = "swupdate")]
    let swc1 = tokio::sync::mpsc::channel(5);
    #[cfg(feature = "swupdate")]
    let swc2 = tokio::sync::mpsc::channel(5);

    #[cfg(feature = "swupdate")]
    tokio::spawn(async move {
        let mut swupdate = SwupdateChannel {
            send: swc1.0,
            recv: swc2.1,
        };
        swupdate.run().await;
    });
    let shutdown_recv2 = shutdown_send.subscribe();
    let shutdown_recv3 = shutdown_send.subscribe();
    let polling_channel = tokio::sync::mpsc::channel(5);

    let mut hvac = HvacController::new();
    hvac.set_ac_setpoint(s.hvac.ac_target);
    hvac.set_auto_setpoint(s.hvac.auto_target);
    hvac.set_heat_setpoint(s.hvac.heat_target);
    hvac.set_mode(s.hvac.current_mode);

    let auc = AppUserCommon {
        args,
        #[cfg(feature = "wifi")]
        wifi,
        #[cfg(feature = "wifi")]
        wifi_device,
        #[cfg(feature = "androidauto")]
        aauto_service: None,
        #[cfg(all(feature = "androidauto", feature = "wifi"))]
        aa_network: Some(network),
        system: sys,
        #[cfg(feature = "wifi")]
        wifi_setup: None,
        #[cfg(feature = "bluetooth")]
        bluetooth: bluetooth.clone(),
        #[cfg(feature = "bluetooth")]
        blue_recv: bluechan,
        #[cfg(feature = "bluetooth")]
        blue_addr: None,
        video: vs,
        old_settings: s.clone(),
        settings: s.clone(),
        hvac,
        #[cfg(feature = "swupdate")]
        swupdate_channel: SwupdateChannelRecv {
            send: swc2.0,
            recv: swc1.1,
        },
        shutdown_send,
        #[cfg(feature = "androidauto")]
        token,
        hvac_public: PublicData::default(),
        sensors: Sensors::default(),
        historical_sensors: VecDeque::new(),
        num_historical_records: 1800,
        polling_channel: polling_channel.0,
        outputs,
        inputs,
    };

    let common = Arc::new(tokio::sync::Mutex::new(auc));

    #[cfg(feature = "wifi")]
    {
        let c = common.lock().await;
        setup_wifi(c).await;
    }

    let mut tasks: tokio::task::JoinSet<Result<(), String>> = tokio::task::JoinSet::new();
    let common2 = common.clone();
    tasks.spawn(async move {
        udp_listener(common2)
            .await
            .inspect_err(|a| log::error!("Radio tcp listener ended: {:?}", a))
    });
    let common2 = common.clone();
    tasks.spawn(async move {
        tcp_listener(common2)
            .await
            .inspect_err(|a| log::error!("Radio tcp listener ended: {:?}", a))
    });
    let common2 = common.clone();
    tasks.spawn(async move {
        sensor_polling(common2, shutdown_recv2, polling_channel.1)
            .await
            .inspect_err(|a| log::error!("Sensor polling ended: {:?}", a))
    });
    #[cfg(feature = "bluetooth")]
    {
        let common2 = common.clone();
        tasks.spawn(async move {
            bluetooth_task(common2, shutdown_recv3)
                .await
                .inspect_err(|a| log::error!("Bluetooth task ended: {:?}", a))
        });
    }

    tokio::select! {
        r = tasks.join_next() => {
            service::log::error!("A task exited {:?}, closing server in 5 seconds", r);
            tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
        }
        _ = tokio::signal::ctrl_c() => {
            let c = common.lock().await;
            let _ = c.shutdown_send.send(());
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        _ = shutdown_recv.recv() => {}
    }
    service::log::error!("Closing server now");
}

service::ServiceAsyncMacro!(service_starter, smain, u64);

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), u32> {
    let service = service::Service::new("uobradio".to_string());
    //service.new_log(service::LogLevel::Info);
    simple_logger::SimpleLogger::new()
        .with_level(service::LogLevel::Info.level_filter())
        .init()
        .unwrap();
    if let Err(e) = service::DispatchAsync!(service, service_starter) {
        Err(e)
    } else {
        Ok(())
    }
}
