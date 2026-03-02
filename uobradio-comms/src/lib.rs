//! Module for communicating with a uobradio

#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

mod hvac;
pub use hvac::*;

use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

#[cfg(feature = "androidauto")]
pub mod aauto;
pub mod settings;
pub mod video;

#[cfg(any(feature = "wifi", feature = "bluetooth"))]
pub mod wireless;

#[cfg(feature = "androidauto")]
use android_auto::AudioChannelType;
#[cfg(target_os = "linux")]
pub use v4l;

/// A list of radios, according the ip adress and port the live at
pub type UobRadios = BTreeMap<std::net::SocketAddr, UobRadio>;

/// Defines the state for the state machine of receiving packets from the radio
#[derive(Debug)]
pub enum RadioReceiveStatus {
    /// The radio is not connected
    Disconnected,
    /// Wait for entire length of packet from the radio until the specified time, storing what has been received so far, and the number of bytes received
    WaitForLength(std::time::Instant, [u8; 4], u8),
    /// Wait for the entire packet of a known length until the specified time, storing the data received so far and the length of the entire packet expected
    WaitForPacket(std::time::Instant, Vec<u8>, u32, u32),
    /// The state machine has received a packet and is ready to process the packet
    GotPacket(Vec<u8>),
}

/// A type that allows for polling of a value, without sending a whole ton of requests.
/// This limits the number of outstanding requests to one. This is useful for queries that take a while to run, compared to how often the data is displayed to the user.
#[derive(Debug)]
pub enum Pollable<T: std::fmt::Debug> {
    /// The variable is idle
    Idle {
        /// The last known value
        last_known: Option<T>,
    },
    /// A request for the variable has been sent
    Waiting {
        /// The last known value
        last_known: Option<T>,
        /// Waiting since time
        waiting_since: std::time::Instant,
    },
    /// A value has been received
    Value {
        /// The value stored
        v: T,
    },
}

impl<T: std::fmt::Debug> Default for Pollable<T> {
    fn default() -> Self {
        Self::Idle { last_known: None }
    }
}

impl<T: std::fmt::Debug> Pollable<T> {
    /// Try to get the contained value
    pub fn value(&self) -> Option<&T> {
        match self {
            Pollable::Idle { last_known } => last_known.as_ref(),
            Pollable::Waiting { last_known, waiting_since: _ } => last_known.as_ref(),
            Pollable::Value { v } => Some(v),
        }
    }

    /// Provides the new value for the object, None means it won't update the last know
    pub fn new_value_optional(&mut self, v: Option<T>) {
        match v {
            Some(v) => {
                *self = Pollable::Value { v };
            }
            None => {
                let b = std::mem::replace(self, Pollable::Idle { last_known: None });
                match b {
                    Pollable::Idle { last_known } => {
                        if let Some(v) = v {
                            *self = Pollable::Idle { last_known: Some(v) };
                        } else {
                            *self = Pollable::Idle { last_known };
                        }
                    }
                    Pollable::Waiting { last_known, waiting_since: _ } => {
                        if let Some(v2) = last_known {
                            *self = Pollable::Value { v: v2 };
                        }
                        else {
                            *self = Pollable::Idle { last_known };
                        }
                    }
                    Pollable::Value { v } => {
                        *self = Pollable::Value { v };
                    }
                };        
            }
        }
    }

    /// Runs a closure when poll action is possible
    pub fn poll_action<U: FnOnce()>(&mut self, closure: U) {
        let b = std::mem::replace(self, Pollable::Idle { last_known: None });
        let a = match b {
            Pollable::Idle { last_known } => {
                *self = Pollable::Waiting { last_known, waiting_since: std::time::Instant::now() };
                true
            }
            Pollable::Waiting { last_known, waiting_since } => {
                if std::time::Instant::now().duration_since(waiting_since) > std::time::Duration::from_secs(5) {
                    *self = Pollable::Idle { last_known };
                } else {
                    *self = Pollable::Waiting { last_known, waiting_since };
                }
                false
            }
            Pollable::Value { v } => {
                *self = Pollable::Waiting { last_known: Some(v), waiting_since: std::time::Instant::now() };
                true
            }
        };
        if a {
            closure();
        }
    }
}

/// The port to listen to for udp communication
const UDP_PORT: u16 = 13456;

/// The commands that can be issued for an audio channel
#[derive(Debug)]
pub enum PendingAudioCommand {
    /// Start the audio channel
    Start,
    /// Stop the audio channel
    Stop,
}

#[cfg(feature = "androidauto")]
/// Manages the user facing aspects of an android auto server
pub struct AndroidAutoServerFrontend {
    /// The video data received so far from the android auto device, h264 encoded video packets
    video_buf: Vec<u8>,
    /// The audio channel data received so far
    audio_bufs: [Vec<u8>; 3],
    /// Audio channel data for pending commands
    audio_commands: [Option<PendingAudioCommand>; 3],
    /// Waiting on acceptance for android auto server frontend
    waiting: bool,
    /// Frontend is running now
    running: bool,
}

#[cfg(feature = "androidauto")]
impl AndroidAutoServerFrontend {
    /// Construct a new self, initialize as waiting for control
    pub fn new() -> Self {
        Self {
            video_buf: Vec::new(),
            audio_bufs: [Vec::new(), Vec::new(), Vec::new()],
            audio_commands: [const { None }; 3],
            waiting: true,
            running: false,
        }
    }

    /// Should the frontend be running?
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Retrieve the data received so far for the android audo video stream and then clear the buffer for new data to be received.
    pub fn get_android_video_buf(&mut self) -> Option<Vec<u8>> {
        if !self.video_buf.is_empty() {
            let b = self.video_buf.clone();
            self.video_buf.clear();
            Some(b)
        } else {
            None
        }
    }
}

#[cfg(feature = "androidauto")]
impl Drop for AndroidAutoServerFrontend {
    fn drop(&mut self) {}
}

/// Represents all possible sensors on the radio
#[derive(Default)]
pub struct Sensors {
    /// Orientation of the system, left-right, forwards-backwards, both in degrees
    pub orientation: Option<(f32, f32)>,
    /// The cabin air temperature
    pub cabin_temp: Option<f32>,
    /// The cabin humidity
    pub cabin_humidity: Option<f32>,
    /// The engine coolant temperature
    pub engine_coolant_temp: Option<f32>,
    /// The engine oil temperature
    pub engine_oil_temp: Option<f32>,
    /// The engine exhaust temperature
    pub engine_exhaust_temp: Option<f32>,
    /// Front differential temperature
    pub front_diff_temp: Option<f32>,
    /// Rear differential temperature
    pub rear_diff_temp: Option<f32>,
    /// The engine oil pressure (psi)
    pub engine_oil_pressure: Option<f32>,
    /// Transmission temperature
    pub trans_temp: Option<f32>,
    /// Transfer case temperature
    pub transfer_temp: Option<f32>,
    /// Door open sensor
    pub door_open: Option<bool>,
    /// Engine rpm sensor
    pub engine_rpm: Option<u16>,
    /// Main system voltage
    pub main_voltage: Option<f32>,
}

/// Represents a uob radio connection
pub struct UobRadio {
    /// Address of where the radio can be contacted
    address: std::net::SocketAddr,
    /// The stream, if comms are currently open
    comms: Option<std::net::TcpStream>,
    /// The status of the receiving state machine
    status: RadioReceiveStatus,
    /// The time to wait until to send another camera image request
    waiting_until: Option<std::time::Instant>,
    /// The length of time for timeouts in receiving packets from the radio
    timeout: std::time::Duration,
    /// The next time a ping should be sent
    ping_time: std::time::Instant,
    /// The map of cameras for the radio, by camera id
    cameras: Option<BTreeMap<u8, video::SendableVideoSource>>,
    /// Am i waiting on the camera options from the radio? TODO, add a timeout feature to this (probably `Option<std::time::Instant>`)
    waiting_for_camera_options: bool,
    /// Am I the handler for bluetooth on the radio. Primarily used by the application on the radio itself.
    /// None indicates I am waiting to see if I was accepted as the bluetooth handler.
    #[cfg(feature = "bluetooth")]
    bluetooth_handler: Option<bool>,
    /// The passkey to be displayed for the user to see during bluetooth pairing.
    #[cfg(feature = "bluetooth")]
    pub display_passkey: Option<u32>,
    /// The passkey to be displayed for confirmation by the user
    #[cfg(feature = "bluetooth")]
    pub confirm_passkey: Option<u32>,
    /// Is an android auto frontend running on this radio?
    #[cfg(feature = "androidauto")]
    aauto: Option<AndroidAutoServerFrontend>,
    /// The sensors on the system
    pub sensors: Sensors,
}

impl UobRadio {
    fn new(address: std::net::SocketAddr, timeout_secs: u64) -> Self {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        Self {
            address,
            comms: None,
            status: RadioReceiveStatus::Disconnected,
            waiting_until: None,
            timeout: timeout,
            ping_time: std::time::Instant::now() + timeout / 3,
            cameras: None,
            waiting_for_camera_options: false,
            #[cfg(feature = "bluetooth")]
            bluetooth_handler: Some(false),
            #[cfg(feature = "androidauto")]
            aauto: None,
            #[cfg(feature = "bluetooth")]
            display_passkey: None,
            #[cfg(feature = "bluetooth")]
            confirm_passkey: None,
            sensors: Sensors::default(),
        }
    }

    fn update_ping_time(&mut self) {
        self.ping_time = std::time::Instant::now() + self.timeout / 3;
    }

    fn check_ping_time(&self) -> bool {
        std::time::Instant::now() > self.ping_time
    }

    /// Get the list of all cameras, as a reference
    pub fn cameras(&self) -> Option<&BTreeMap<u8, video::SendableVideoSource>> {
        self.cameras.as_ref()
    }

    /// Get the list of all cameras, mutably
    pub fn cameras_mut(&mut self) -> Option<&mut BTreeMap<u8, video::SendableVideoSource>> {
        self.cameras.as_mut()
    }
}

/// A potentially unused struct
#[cfg(not(target_os = "android"))]
pub struct FakeMessageAboutAppUser {
    /// who cares
    pub addr: std::net::SocketAddr,
    /// who cares
    pub send: std::sync::mpsc::Sender<MessageToApp>,
}

impl Drop for UobRadio {
    fn drop(&mut self) {}
}

/// Commands to manipulate the gpio on a radio
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Gpio {
    /// Control the winch output, IN, OUT. Both together is invalid.
    WinchControl(bool, bool),
    /// Enable or disable the leds for the given camera
    CameraLedControl(u8, bool),
    /// Lock all doors
    LockDoors,
    /// Unlock doors
    UnlockDoors,
    /// Control a door window up or down
    WindowControl {
        /// The window id
        id: u8,
        /// Make the window go up. Up and down at the same time is invalid
        up: bool,
        /// Make the window go down. Up and down at the same time is invalid
        down: bool,
    },
    /// Light control (light id and whether to enable or disable the light)
    LightControl(u8, bool),
    /// Control inverter main power
    InverterPower(bool),
    /// Control an auxiliary output
    AuxOutput(u8, bool),
    /// Retrieve the value of an auxiliarry input
    GetAuxInput(u8),
}

/// Commands for an external radio attached to the radio
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum RadioCommand {
    /// Start a transmission
    StartTransmission,
    /// Stop a transmission
    StopTransmission,
    /// Data to transmit
    TransmissionDataPartial(Vec<u8>),
}

/// A message that can be sent from an app.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageFromApp {
    /// A ping message
    Ping(u16),
    #[cfg(feature = "test")]
    /// The service should exit
    Exit,
    /// Request an image from the specified camera
    RequestCamera(u8),
    /// Request the entire btreemap of all cameras
    RequestCameras,
    /// Manipulate gpio in the manner specified
    GpioControl(Gpio),
    /// The camera index with the bincode encoded data for the setting to change
    CameraSettingControl(u8, u8, video::ControlValue),
    /// Update the nonvolatile settings on the radio
    NewSettings {
        /// The new settings
        settings: NonvolatileSettings,
        #[cfg(feature = "wifi")]
        /// Should the wifi be reconnected?
        wifi_reconnect: bool,
    },
    /// Request all nonvolatile settings
    RequestSettings,
    #[cfg(feature = "bluetooth")]
    /// Request from the the app user that handles bluetooth pairing stuff
    RequestBluetoothControl,
    #[cfg(feature = "androidauto")]
    /// Request from the app user that handles android auto stuff
    RequestAndroidAutoControl,
    #[cfg(feature = "bluetooth")]
    /// Enable or disable bluetooth discoverable
    SetBluetoothDiscovery(bool),
    #[cfg(feature = "bluetooth")]
    /// A generic bluetooth command
    BluetoothMessage(bluetooth_rust::MessageFromBluetoothHost),
    #[cfg(feature = "androidauto")]
    /// A generic android auto command to the "phone"
    AndroidAutoMessage(aauto::AndroidAutoMessageToPhone),
    /// A command to operate the external radio
    ExternalRadio(RadioCommand),
    #[cfg(feature = "wifi")]
    /// Scan for wifi networks with the wifi adapter
    ScanForWifiNetworks,
    #[cfg(feature = "wifi")]
    /// List all known wifi networks
    ListAllKnownWifiNetworks,
    #[cfg(feature = "wifi")]
    /// Connect to the specified network
    ConnectToNetwork {
        /// The majority of the network details
        network: nmrs::Network,
        /// The password
        password: Option<String>,
    },
    #[cfg(feature = "wifi")]
    /// Connect to a saved wifi network
    ConnectToSavedWifiNetwork(String),
    #[cfg(feature = "wifi")]
    /// forget the given wifi network
    ForgetWifiNetwork(String),
    #[cfg(feature = "wifi")]
    /// Get the ssid and password for the current wifi network
    GetWifiDetails,
    /// Ac control messages
    Hvac(HvacControl),
    /// Download list of update files from update server specified
    DownloadServerFileList(String),
    /// Download the firmware file from the update server
    DownloadServerFile(String),
    /// Start the update
    StartUpdate,
    /// Query update progress
    GetUpdateProgress,
}

#[cfg(feature = "bluetooth")]
use bluetooth_rust::{MessageFromBluetoothHost, MessageToBluetoothHost};

#[cfg(feature = "bluetooth")]
impl From<MessageToBluetoothHost> for ActualMessageToBluetoothHost {
    fn from(value: MessageToBluetoothHost) -> Self {
        match value {
            MessageToBluetoothHost::DisplayPasskey(passkey, _) => {
                ActualMessageToBluetoothHost::DisplayPasskey(passkey)
            }
            MessageToBluetoothHost::ConfirmPasskey(passkey, _) => {
                ActualMessageToBluetoothHost::ConfirmPasskey(passkey)
            }
            MessageToBluetoothHost::CancelDisplayPasskey => {
                ActualMessageToBluetoothHost::CancelDisplayPasskey
            }
        }
    }
}

#[cfg(feature = "bluetooth")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
/// Messages that can be sent specifically to the app user hosting the bluetooth controls
pub enum ActualMessageToBluetoothHost {
    /// The passkey used for pairing devices
    DisplayPasskey(u32),
    /// The passkey to display and confirm
    ConfirmPasskey(u32),
    /// Cancal the passkey display
    CancelDisplayPasskey,
    /// The status of bluetooth discovery
    BluetoothEnabled(bool),
}

/// A message that can be sent to an app. The main radio application is also considered an app. Therefore anything the main radio can do, the mobile app has the potential to also do.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageToApp {
    /// Contains the ping id number for tcp communications, or the port number for udp communications
    PingReply(u16),
    /// Contains a jpeg image from a single camera
    CameraDataJpeg(u8, Vec<u8>),
    /// Send the btreemap of all cameras
    CamerasBtreeMap(BTreeMap<u8, video::SendableVideoSource>),
    /// The new settings for the radio
    NewSettings(NonvolatileSettings),
    #[cfg(feature = "bluetooth")]
    /// Tell the app if they were accepted as a bluetooth handler
    BluetoothHandlerResult(bool),
    #[cfg(feature = "androidauto")]
    /// Tell the app if they were accepted as an android auto handler
    AndroidAutoHandlerResult(bool),
    #[cfg(feature = "bluetooth")]
    /// A bluetooth message from the bluetooth stuff
    BluetoothMessage(ActualMessageToBluetoothHost),
    #[cfg(feature = "androidauto")]
    /// A generic android auto command from the "phone"
    AndroidAutoMessage(aauto::AndroidAutoMessageFromPhone),
    #[cfg(feature = "wifi")]
    /// A list of wifi networks
    WifiList(Vec<nmrs::Network>),
    #[cfg(feature = "wifi")]
    /// The details for the current wifi network
    WifiDetails {
        /// The ssid of the network
        ssid: String,
        /// The password of the network
        password: Option<String>,
    },
    #[cfg(feature = "wifi")]
    /// There is no wifi network
    NoCurrentWifiNetwork,
    #[cfg(feature = "wifi")]
    /// Indicates a new connection to a wifi network
    ConnectedToWifiNetwork {
        /// The ssid of the network
        ssid: String,
        /// The password of the network
        password: Option<String>,
    },
    #[cfg(feature = "wifi")]
    /// Indicates a failure to connect to the indicated wifi network
    FailedToConnectToWifiNetwork {
        /// The ssid of the network
        ssid: String,
    },
    #[cfg(feature = "wifi")]
    /// Indicates a failure to scan for wifi networks
    FailedToScanForWifiNetworks {
        /// The reason for failure
        reason: String,
    },
    #[cfg(feature = "wifi")]
    /// The list of known wifi networks
    KnownWifiNetworks(Vec<String>),
    /// A response to an ac control command
    Ac(AcResponse),
    /// The list of files on the remote update server
    ListOfServerUpdateFiles {
        /// The files
        files: Vec<String>,
    },
    /// The requested file download successfully completed?
    ServerFileDownloadComplete(bool),
    /// The percentage of the file download
    ServerFileDownloadProgress(f32),
    /// The percentage of the update progress
    UpdateProgress(u8, u8),
    /// No update in progress
    NoUpdateInProgress,
}

impl MessageFromApp {
    /// Send the message to the given stream.
    pub fn send_to_stream(&self, stream: &mut std::net::TcpStream) -> Result<(), String> {
        let packet = bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap();
        stream
            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
            .map_err(|e| e.to_string())?;
        stream.write_all(&packet).map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl MessageToApp {
    /// Send the message to the given stream.
    #[cfg(not(target_os = "android"))]
    pub async fn send_to_stream(
        &self,
        stream: &std::sync::Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    ) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let mut stream = stream.lock().await;
        let packet = bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap();
        stream
            .write_all(&((packet.len() as u32).to_be_bytes()[0..4]))
            .await
            .map_err(|e| e.to_string())?;
        stream.write_all(&packet).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl UobRadio {
    #[cfg(feature = "androidauto")]
    /// Is the android auto frontend running?
    pub fn android_auto_frontend(&self) -> bool {
        self.aauto.as_ref().map(|a| a.is_running()).unwrap_or(false)
    }

    /// Call this to prcess received packets. The closure allows the user to specify additional processing for any packets received.
    pub fn process_received<F: FnMut(&MessageToApp)>(
        &mut self,
        mut closure: F,
    ) -> Result<(), String> {
        self.connect();
        if let Some(stream) = &mut self.comms {
            loop {
                use std::io::Read;
                let mut got_length = None;
                if let RadioReceiveStatus::WaitForLength(time, l, i) = &mut self.status {
                    if std::time::Instant::now() > *time {
                        return Err("Timeout waiting for length".to_string());
                    }
                    match stream.read(&mut l[*i as usize..]) {
                        Ok(a) => {
                            if (a + *i as usize) == 4 {
                                let length = u32::from_be_bytes(*l);
                                got_length = Some(length);
                            }
                            *i += a as u8;
                        }
                        Err(e) => {
                            if let std::io::ErrorKind::WouldBlock = e.kind() {
                                return Ok(());
                            } else {
                                return Err(e.to_string());
                            }
                        }
                    }
                }
                if let Some(length) = got_length {
                    self.status = RadioReceiveStatus::WaitForPacket(
                        std::time::Instant::now() + self.timeout,
                        vec![0; length as usize],
                        length,
                        0,
                    );
                }
                let mut go_idle = false;
                if let RadioReceiveStatus::WaitForPacket(time, packet, length, l) = &mut self.status
                {
                    if std::time::Instant::now() > *time {
                        return Err("Timeout waiting for packet".to_string());
                    }
                    match stream.read(&mut packet[*l as usize..]) {
                        Ok(a) => {
                            if (a + *l as usize) == *length as usize {
                                let packet: Result<
                                    (MessageToApp, usize),
                                    bincode::error::DecodeError,
                                > = bincode::serde::decode_from_slice(
                                    &packet,
                                    bincode::config::standard(),
                                );
                                if let Ok((packet, length2)) = packet {
                                    if length2 != *length as usize {
                                        log::error!(
                                            "Wrong packet length received {}/{}",
                                            length,
                                            length2
                                        );
                                        return Err("Invalid packet received".to_string());
                                    }
                                    match &packet {
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::NoCurrentWifiNetwork => {}
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::KnownWifiNetworks(_) => {}
                                        MessageToApp::NoUpdateInProgress => {}
                                        MessageToApp::UpdateProgress(_, _) => {}
                                        MessageToApp::ServerFileDownloadProgress(_) => {}
                                        MessageToApp::ServerFileDownloadComplete(_) => {}
                                        MessageToApp::ListOfServerUpdateFiles { files: _ } => {}
                                        MessageToApp::Ac(_c) => { }
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::FailedToConnectToWifiNetwork { ssid: _ } => {}
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::FailedToScanForWifiNetworks { reason } => {
                                            log::error!("Failed to scan for wifi networks: {reason}");
                                        }
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::ConnectedToWifiNetwork { ssid: _, password: _ } => {}
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::WifiDetails { ssid: _, password: _ } => {}
                                        #[cfg(feature = "wifi")]
                                        MessageToApp::WifiList(_list) => {}
                                        #[cfg(feature = "androidauto")]
                                        MessageToApp::AndroidAutoMessage(m) => {
                                            match m {
                                                aauto::AndroidAutoMessageFromPhone::AudioChannelOpen(_) => {
                                                }
                                                aauto::AndroidAutoMessageFromPhone::AudioChannelClose(_) => {
                                                }
                                                aauto::AndroidAutoMessageFromPhone::AudioChannelStart(c) => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        log::info!("Audio channel start {:?}", c);
                                                        let i = match c {
                                                            AudioChannelType::Media => 0,
                                                            AudioChannelType::Speech => 1,
                                                            AudioChannelType::System => 2,
                                                        };
                                                        aauto.audio_commands[i] = Some(PendingAudioCommand::Start);
                                                    }
                                                }
                                                aauto::AndroidAutoMessageFromPhone::AudioChannelStop(c) => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        log::info!("Audio channel stop {:?}", c);
                                                        let i = match c {
                                                            AudioChannelType::Media => 0,
                                                            AudioChannelType::Speech => 1,
                                                            AudioChannelType::System => 2,
                                                        };
                                                        aauto.audio_bufs[i].clear();
                                                        aauto.audio_commands[i] = Some(PendingAudioCommand::Stop);
                                                    }
                                                }
                                                aauto::AndroidAutoMessageFromPhone::AudioContent(c, data) => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        log::info!("Audio channel data {:?} {}", c, data.len());
                                                        let i = match c {
                                                            AudioChannelType::Media => 0,
                                                            AudioChannelType::System => 1,
                                                            AudioChannelType::Speech => 2,
                                                        };
                                                        aauto.audio_bufs[i].append(&mut data.to_owned());
                                                    }
                                                }
                                                aauto::AndroidAutoMessageFromPhone::VideoContent(data) => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        aauto.video_buf.append(&mut data.to_owned());
                                                    }
                                                }
                                                aauto::AndroidAutoMessageFromPhone::Disconnect => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        log::error!("Android auto no longer running");
                                                        aauto.video_buf.clear();
                                                        aauto.running = false;
                                                    }
                                                }
                                                aauto::AndroidAutoMessageFromPhone::Connect => {
                                                    if let Some(aauto) = &mut self.aauto {
                                                        log::error!("Android auto now running");
                                                        aauto.running = true;
                                                    }
                                                }
                                            }
                                        }
                                        MessageToApp::PingReply(_) => {}
                                        MessageToApp::NewSettings(_) => {}
                                        #[cfg(feature = "bluetooth")]
                                        MessageToApp::BluetoothMessage(m) => {
                                            match m {
                                                ActualMessageToBluetoothHost::DisplayPasskey(pass) => {
                                                    self.display_passkey.replace(*pass);
                                                }
                                                ActualMessageToBluetoothHost::ConfirmPasskey(pass) => {
                                                    self.confirm_passkey.replace(*pass);
                                                }
                                                ActualMessageToBluetoothHost::CancelDisplayPasskey => {
                                                    self.display_passkey.take();
                                                    self.confirm_passkey.take();
                                                }
                                                ActualMessageToBluetoothHost::BluetoothEnabled(_) => {}
                                            }
                                        }
                                        MessageToApp::CameraDataJpeg(id, data) => {
                                            self.waiting_until = None;
                                            if let Some(cameras) = &mut self.cameras {
                                                let vsrc = cameras.get_mut(id);
                                                if let Some(camera) = vsrc {
                                                    if let Some(img) = crate::video::PixelImage::<crate::video::RgbPixel>::from_jpeg_image(&data) {
                                                        camera.image.replace(img.into());
                                                    }
                                                }
                                            }
                                        }
                                        MessageToApp::CamerasBtreeMap(map) => {
                                            self.cameras.replace(map.to_owned());
                                        }
                                        #[cfg(feature = "androidauto")]
                                        MessageToApp::AndroidAutoHandlerResult(result) => {
                                            log::error!("Android auto result is {}", result);
                                            if let Some(aauto) = &mut self.aauto {
                                                aauto.waiting = !*result;
                                            }
                                        }
                                        #[cfg(feature = "bluetooth")]
                                        MessageToApp::BluetoothHandlerResult(result) => {
                                            //log::error!("Bluetooth result is {}", result);
                                            self.bluetooth_handler = Some(*result);
                                        }
                                    }
                                    closure(&packet);
                                }
                                go_idle = true;
                            }
                            *l += a as u32;
                        }
                        Err(e) => {
                            if let std::io::ErrorKind::WouldBlock = e.kind() {
                                return Ok(());
                            } else {
                                return Err(e.to_string());
                            }
                        }
                    }
                }
                if go_idle {
                    self.status = RadioReceiveStatus::WaitForLength(
                        std::time::Instant::now() + self.timeout,
                        [0; 4],
                        0,
                    );
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "androidauto")]
    /// Attempt to pull all current android auto video data
    pub fn get_android_auto_video_buf(&mut self) -> Option<Vec<u8>> {
        if let Some(aauto) = &mut self.aauto {
            aauto.get_android_video_buf()
        } else {
            None
        }
    }

    #[cfg(feature = "androidauto")]
    /// Process any pending commands on audio channels
    pub fn process_pending_audio_commands<F: FnMut(AudioChannelType, PendingAudioCommand)>(
        &mut self,
        mut f: F,
    ) {
        if let Some(aauto) = &mut self.aauto {
            if let Some(c) = aauto.audio_commands[0].take() {
                f(AudioChannelType::Media, c);
            }
            if let Some(c) = aauto.audio_commands[1].take() {
                f(AudioChannelType::System, c);
            }
            if let Some(c) = aauto.audio_commands[2].take() {
                f(AudioChannelType::Speech, c);
            }
        }
    }

    #[cfg(feature = "androidauto")]
    /// Transmit the given audio data to the android auto device
    pub fn transmit_audio(&mut self, data: Vec<i16>) {
        if self.aauto.is_some() {
            let timestamp: u64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            let data2 = data.iter().map(|e| e.to_le_bytes()).flatten().collect();
            let p = android_auto::AndroidAutoMessage::Audio(Some(timestamp), data2);
            let m2 = aauto::AndroidAutoMessageToPhone::Message(p.sendable());
            //let _ = self.send_packet(
            //    MessageFromApp::AndroidAutoMessage(m2),
            //);
        }
    }

    #[cfg(feature = "androidauto")]
    /// Process all audio data received with a closure for all channel types, then clear it
    pub fn process_received_audio<F: FnMut(AudioChannelType, &[i16])>(&mut self, mut f: F) {
        if let Some(aauto) = &mut self.aauto {
            if !aauto.audio_bufs[0].is_empty() {
                let r: &[u8] = aauto.audio_bufs[0].as_ref();
                let r2: Vec<i16> = r
                    .chunks_exact(2)
                    .map(|v| i16::from_le_bytes([v[0], v[1]]))
                    .collect();
                f(AudioChannelType::Media, &r2);
                aauto.audio_bufs[0].clear();
            }
            if !aauto.audio_bufs[1].is_empty() {
                let r: &[u8] = aauto.audio_bufs[1].as_ref();
                let r2: Vec<i16> = r
                    .chunks_exact(2)
                    .map(|v| i16::from_le_bytes([v[0], v[1]]))
                    .collect();
                f(AudioChannelType::System, &r2);
                aauto.audio_bufs[1].clear();
            }
            if !aauto.audio_bufs[2].is_empty() {
                let r: &[u8] = aauto.audio_bufs[2].as_ref();
                let r2: Vec<i16> = r
                    .chunks_exact(2)
                    .map(|v| i16::from_le_bytes([v[0], v[1]]))
                    .collect();
                f(AudioChannelType::Speech, &r2);
                aauto.audio_bufs[2].clear();
            }
        }
    }

    /// Send a request for all cameras available on the radio. Does nothing if camera info has already been received or if waiting on camera data from the radio.
    pub fn get_cameras(&mut self) -> bool {
        if self.cameras.is_none() {
            if !self.waiting_for_camera_options {
                let packet = MessageFromApp::RequestCameras;
                if let Some(comms) = &mut self.comms {
                    let a = packet.send_to_stream(comms);
                    self.waiting_for_camera_options = a.is_ok();
                    self.update_ping_time();
                }
            }
        }
        !self.cameras.is_none()
    }

    /// Send a ping to the radio. Only actually sends a ping when required based on the last time a packet was sent. This prevents needless pings from being sent.
    pub fn ping(&mut self) -> Result<(), String> {
        self.connect();
        let time = self.check_ping_time();
        #[cfg(feature = "bluetooth")]
        {
            let blue_waiting = self.confirm_passkey.is_some() || self.display_passkey.is_some();
            if let Some(comms) = &mut self.comms {
                if time {
                    let packet = MessageFromApp::Ping(1);
                    packet.send_to_stream(comms)?;
                    if blue_waiting {
                        let packet = MessageFromApp::BluetoothMessage(
                            MessageFromBluetoothHost::PasskeyMessage(
                                bluetooth_rust::ResponseToPasskey::Waiting,
                            ),
                        );
                        packet.send_to_stream(comms)?;
                    }
                    self.update_ping_time();
                }
                Ok(())
            } else {
                Err("Not connected".to_string())
            }
        }
        #[cfg(not(feature = "bluetooth"))]
        {
            if let Some(comms) = &mut self.comms {
                if time {
                    let packet = MessageFromApp::Ping(1);
                    packet.send_to_stream(comms)?;
                }
            }
            Ok(())
        }
    }

    /// Run this function whenever a connection to the radio is needed.
    /// Does nothing if already connected to the radio.
    pub fn connect(&mut self) {
        if self.comms.is_none() {
            let tcp = std::net::TcpStream::connect(self.address);
            if let Ok(tcp) = tcp {
                let _ = tcp.set_nonblocking(true);
                self.comms.replace(tcp);
                self.status = RadioReceiveStatus::WaitForLength(
                    std::time::Instant::now() + self.timeout,
                    [0; 4],
                    0,
                );
                self.waiting_until = None;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

    /// Send a packet to the radio, handling disconnects if sending should fail.
    pub fn send_packet(&mut self, packet: MessageFromApp) -> Result<(), ()> {
        if let Some(stream) = &mut self.comms {
            if packet.send_to_stream(stream).is_err() {
                self.update_ping_time();
                self.disconnect();
                Err(())
            } else {
                Ok(())
            }
        } else {
            Err(())
        }
    }

    #[cfg(feature = "bluetooth")]
    /// Try to establish self as the handler for bluetooth. Does nothing if already established as the handler
    pub fn try_get_bluetooth(&mut self) {
        if Some(false) == self.bluetooth_handler {
            if self
                .send_packet(MessageFromApp::RequestBluetoothControl)
                .is_ok()
            {
                self.bluetooth_handler.take();
            }
        }
    }

    #[cfg(feature = "androidauto")]
    /// Try to establish self as the handler for android auto. Does nothing if already established as the handler
    pub fn try_get_android_auto(&mut self) {
        if self.aauto.is_none() {
            if self
                .send_packet(MessageFromApp::RequestAndroidAutoControl)
                .is_ok()
            {
                log::error!("Initializing an android auto server frontend");
                self.aauto.replace(AndroidAutoServerFrontend::new());
            }
        }
    }

    /// Disconnect from the radio for some reasion
    pub fn disconnect(&mut self) {
        self.comms.take();
        self.status = RadioReceiveStatus::Disconnected;
        #[cfg(feature = "bluetooth")]
        {
            self.bluetooth_handler = Some(false);
        }
        #[cfg(feature = "androidauto")]
        {
            self.aauto.take();
        }
        self.waiting_until = None;
    }

    /// Send gpio data to the radio
    pub fn send_gpio(&mut self, gpio: Gpio) -> Result<(), String> {
        self.connect();
        if let Some(comms) = &mut self.comms {
            log::error!("Sending gpio request {:?}", gpio);
            let packet = MessageFromApp::GpioControl(gpio);
            packet.send_to_stream(comms)?;
            self.update_ping_time();
            Ok(())
        } else {
            Err("Not connected".to_string())
        }
    }

    /// Send a request to obtain the image of the specified camera
    pub fn send_camera_request(&mut self, index: u8) -> Result<(), String> {
        self.connect();
        if let Some(inst) = &self.waiting_until {
            if *inst < std::time::Instant::now() {
                self.waiting_until = None;
            }
        } else {
            if let Some(comms) = &mut self.comms {
                let packet = MessageFromApp::RequestCamera(index);
                packet.send_to_stream(comms)?;
                self.waiting_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
                self.update_ping_time();
            }
        }
        Ok(())
    }

    /// Indicate that the camera image request has been completed.
    pub fn finish_camera_request(&mut self) {
        log::error!("Finishing camera request");
        self.waiting_until = None;
    }

    /// Construct a new self, corresponding to localhost. Used by the main radio application.
    pub fn localhost() -> Self {
        let ip: std::net::Ipv4Addr = std::net::Ipv4Addr::new(127, 0, 0, 1);
        let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(ip), 13457);
        UobRadio::new(addr, 5)
    }

    /// Run a detection to find all uob radios on the local network.
    /// times is the number of broadcast packets to send out. Since it is udp, there is no guarantee that 100% of packets will be received.
    pub fn detect_radios(times: u8) -> Result<UobRadios, std::io::Error> {
        let mut radios = UobRadios::new();
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::new(5, 0)))?;
        socket.set_broadcast(true)?;
        let packet =
            bincode::serde::encode_to_vec(MessageFromApp::Ping(0), bincode::config::standard())
                .unwrap();
        for _ in 0..times {
            socket.send_to(&packet, format!("255.255.255.255:{}", UDP_PORT))?;
        }
        let mut response = vec![0; 1500];
        while let Ok((n, addr)) = socket.recv_from(&mut response) {
            let packet: Result<(MessageToApp, usize), bincode::error::DecodeError> =
                bincode::serde::decode_from_slice(&response[0..n], bincode::config::standard());
            if let Ok((MessageToApp::PingReply(port), _n)) = packet {
                let mut tcp_addr = addr.clone();
                tcp_addr.set_port(port);
                let r = UobRadio::new(tcp_addr, 5);
                radios.insert(addr, r);
            }
        }
        Ok(radios)
    }
}

/// The volatile settings for the radio
#[derive(Default)]
pub struct VolatileSettings {
    /// The settings page settings
    pub settings: settings::Settings,
    /// The image to display for the video screen
    pub video_texture: Option<egui::TextureHandle>,
    /// The volatile hvac settings
    pub hvac: hvac::VolatileSettings,
    /// Which video stream to look at
    pub which_video: u8,
    #[cfg(any(feature = "wifi", feature = "bluetooth"))]
    /// The wifi page settings
    pub wireless: wireless::Settings,
}

/// Non-volatile settings that should be saved to nonvolatile storage of some kind
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct NonvolatileSettings {
    /// The hvac settings
    pub hvac: hvac::Settings,
    #[cfg(feature = "wifi")]
    /// The wifi configuration
    pub wifi_config: wireless::NvSettings,
}

impl NonvolatileSettings {
    /// Save the non-volatile settings to the current directory
    pub fn save(&self, path_override: &Option<std::path::PathBuf>) {
        let d = toml::to_string(self).unwrap();
        let f = if let Some(p) = path_override {
            std::fs::File::create(p)
        } else {
            std::fs::File::create("./service-settings.toml")
        };
        if let Ok(mut f) = f {
            let _ = f.write_all(d.as_bytes());
        }
    }

    /// Load the nonvolatile settings from the current directory
    pub fn load(path_override: &Option<std::path::PathBuf>) -> Self {
        let f = if let Some(p) = path_override {
            std::fs::File::open(p)
        } else {
            std::fs::File::open("./service-settings.toml")
        };
        match f {
            Ok(mut f) => {
                let mut contents = Vec::new();
                let _ = f.read_to_end(&mut contents);
                match str::from_utf8(&contents) {
                    Ok(contents) => {
                        let s = toml::from_str::<NonvolatileSettings>(contents);
                        match s {
                            Ok(s) => s,
                            Err(e) => {
                                log::error!("Failed to parse nonvolatile settings {e}");
                                Self::default()
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failure converting nonvolatile settings {e}");
                        Self::default()
                    }
                }
            }
            Err(e) => {
                log::error!("Nonvolatile settings error {e}");
                let a = Self::default();
                a.save(path_override);
                a
            }
        }
    }
}

/// The messages to send to the swupdate websocket channel
pub enum MessageToSwupdateChannel {
    /// Exit the websocket comms
    Exit,
}

/// The messages to receive from the swupdate websocket channel
pub enum MessageFromSwupdateChannel {
    /// the websocket is ready
    Ready,
    /// A progress message
    Progress(u8, u8),
}
