#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(unused_extern_crates)]

//! This library is intended to eventually be a cross-platform bluetooth handling platform
//! Android portions adapted from https://github.com/wuwbobo2021/android-bluetooth-serial-rs


#[cfg(target_os = "android")]
use std::sync::Arc;
#[cfg(target_os = "android")]
use std::sync::Mutex;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::Java;
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "linux")]
pub use bluer::rfcomm::Profile as RfcommProfile;

#[cfg(target_os = "linux")]
pub use bluer::Uuid;

#[cfg(target_os = "linux")]
pub use bluer::rfcomm::ProfileHandle as RfcommProfileHandle;

#[cfg(target_os = "linux")]
pub use bluer::rfcomm::Listener as RfcommListener;

#[cfg(target_os = "linux")]
pub use bluer::rfcomm::SocketAddr as RfcommSocketAddr;

#[cfg(target_os = "linux")]
pub use bluer::Address as BluetoothAddress;

mod bluetooth_uuid;
pub use bluetooth_uuid::BluetoothUuid;

/// Commands issued to the library
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum BluetoothCommand {
    /// Detect all bluetooth adapters present on the system
    DetectAdapters,
    /// Find out how many bluetooth adapters are detected
    QueryNumAdapters,
}

/// Messages that can be sent specifically to the app user hosting the bluetooth controls
pub enum MessageToBluetoothHost {
    /// The passkey used for pairing devices
    DisplayPasskey(u32, tokio::sync::mpsc::Sender<ResponseToPasskey>),
    /// The passkey to confirm for pairing
    ConfirmPasskey(u32, tokio::sync::mpsc::Sender<ResponseToPasskey>),
    /// Cancal the passkey display
    CancelDisplayPasskey,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
/// Messages that are send directly from the bluetooth host
pub enum MessageFromBluetoothHost {
    /// A response about the active pairing passkey
    PasskeyMessage(ResponseToPasskey),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
/// The user response to a bluetooth passkey
pub enum ResponseToPasskey {
    /// The passkey is accepted
    Yes,
    /// The passkey is not accepted
    No,
    /// The process is canceled by the user
    Cancel,
    /// Waiting on the user to decide
    Waiting,
}

/// Responses issued by the library
pub enum BluetoothResponse {
    /// The number of bluetooth adapters detected
    Adapters(usize),
}

/// Represents a bluetooth adapter that communicates to bluetooth devices
pub struct BluetoothAdapter {
    #[cfg(target_os = "android")]
    adapter: android::Bluetooth,
    #[cfg(target_os = "linux")]
    adapter: linux::BluetoothHandler,
}

/// A bluetooth device
#[cfg(target_os = "android")]
pub struct BluetoothDevice(android::BluetoothDevice);

/// A bluetooth device
#[cfg(target_os = "android")]
pub struct BluetoothSocket<'a>(&'a mut android::BluetoothSocket);

#[cfg(target_os = "android")]
impl BluetoothAdapter {
    /// Construct a new bluetooth adapter object
    pub fn new(app: AndroidApp) -> Self {
        let java = android::Java::make(app);
        Self {
            adapter: android::Bluetooth::new(Arc::new(Mutex::new(java))),
        }
    }

    /// Retrieve a list of paired bluetooth devices, if possible
    pub fn get_paired_devices(&mut self) -> Option<Vec<BluetoothDevice>> {
        let devs = self.adapter.get_bonded_devices();
        if let Some(devs) = devs {
            Some(devs.into_iter().map(|a| BluetoothDevice(a)).collect())
        }
        else {
            None
        }
    }

    /// Cancel bluetooth discovery on the bluetooth adapter
    pub fn cancel_discovery(&mut self) {
        self.adapter.cancel_discovery()
    }
}

#[cfg(target_os = "android")]
impl BluetoothDevice {
    /// Run the service discovery protocol to discover available uuids for this device
    pub fn run_sdp(&mut self) {
        self.0.get_uuids_with_sdp();
    }

    /// Get all known uuids for this device
    pub fn get_uuids(&mut self) -> Result<Vec<Uuid>, std::io::Error> {
        self.0.get_uuids()
    }

    /// Retrieve the device name
    pub fn get_name(&self) -> Result<String, std::io::Error> {
        self.0.get_name()
    }

    /// Retrieve the device address
    pub fn get_address(&mut self) -> Result<String, std::io::Error> {
        self.0.get_address()
    }

    /// Retrieve the device pairing (bonding) status
    pub fn get_bond_state(&self) -> Result<i32, std::io::Error> {
        self.0.get_bond_state()
    }

    /// Attempt to get an rfcomm socket for the given uuid and seciruty setting
    pub fn get_rfcomm_socket(
        &mut self,
        uuid: Uuid,
        is_secure: bool,
    ) -> Option<BluetoothSocket> {
        self.0.get_rfcomm_socket(uuid, is_secure).map(|a| BluetoothSocket(a))
    }
}

#[cfg(target_os = "android")]
impl<'a> BluetoothSocket<'a> {
    /// Attempts to connect to a remote device. When connected, it creates a
    /// backgrond thread for reading data, which terminates itself on disconnection.
    /// Do not reuse the socket after disconnection, because the underlying OS
    /// implementation is probably incapable of reconnecting the device, just like
    /// `java.net.Socket`.
    pub fn connect(&mut self) -> Result<(), std::io::Error> {
        self.0.connect()
    }
}