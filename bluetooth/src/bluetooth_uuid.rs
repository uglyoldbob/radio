//! UUID stuff for android bluetooth

#[cfg(target_os = "android")]
use super::android::Java;
#[cfg(target_os = "android")]
use super::android::jerr;
#[cfg(target_os = "android")]
use jni_min_helper::*;
#[cfg(target_os = "android")]
use std::sync::{Arc, Mutex};

/// Represents the uuid for a bluetooth service
#[derive(Debug, PartialEq)]
pub enum BluetoothUuid {
    /// Android auto
    AndroidAuto,
    /// Serial port protocol
    SPP,
    /// a2dp source
    A2dpSource,
    /// a2dp sink
    A2dpSink,
    /// base bluetooth profile
    Base,
    /// headset protocol, hs
    HspHs,
    /// headset protocol ag
    HspAg,
    /// handsfree protocol, ag
    HfpAg,
    /// Handsfree protocol, hs
    HfpHs,
    /// Obex opp protocol
    ObexOpp,
    /// Obex ftp protocol
    ObexFtp,
    /// Obex mas protocol
    ObexMas,
    /// Obex mns protocol
    ObexMns,
    /// Obex pse protocol
    ObexPse,
    /// Obex sync protocol
    ObexSync,
    /// Avrcp remote protocol
    AvrcpRemote,
    /// Network nap protocol for bluetooth networking
    NetworkingNap,
    /// An unknown bluetooth uuid
    Unknown(String),
}

impl BluetoothUuid {
    /// Get the uuid as a str reference
    pub fn as_str(&self) -> &str {
        match self {
            BluetoothUuid::SPP => "00001101-0000-1000-8000-00805F9B34FB",
            BluetoothUuid::A2dpSource => "0000110a-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::HfpHs => "0000111e-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexOpp => "00001105-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexFtp => "00001106-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexSync => "00001104-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::A2dpSink => "0000110b-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::AvrcpRemote => "0000110e-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexPse => "0000112f-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::HfpAg => "0000111f-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexMas => "00001132-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::ObexMns => "00001133-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::Base => "00000000-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::NetworkingNap => "00001116-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::HspHs => "00001108-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::HspAg => "00001112-0000-1000-8000-00805f9b34fb",
            BluetoothUuid::AndroidAuto => "4de17a00-52cb-11e6-bdf4-0800200c9a66",
            BluetoothUuid::Unknown(s) => s,
        }
    }

    /// Convert the given str into a Self
    pub fn from_str(s: &str) -> Self {
        match s {
            "00001101-0000-1000-8000-00805F9B34FB" => BluetoothUuid::SPP,
            "0000110a-0000-1000-8000-00805f9b34fb" => BluetoothUuid::A2dpSource,
            "0000111e-0000-1000-8000-00805f9b34fb" => BluetoothUuid::HfpHs,
            "00001105-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexOpp,
            "00001106-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexFtp,
            "00001104-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexSync,
            "0000110b-0000-1000-8000-00805f9b34fb" => BluetoothUuid::A2dpSink,
            "0000110e-0000-1000-8000-00805f9b34fb" => BluetoothUuid::AvrcpRemote,
            "0000112f-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexPse,
            "0000111f-0000-1000-8000-00805f9b34fb" => BluetoothUuid::HfpAg,
            "00001132-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexMas,
            "00001133-0000-1000-8000-00805f9b34fb" => BluetoothUuid::ObexMns,
            "00000000-0000-1000-8000-00805f9b34fb" => BluetoothUuid::Base,
            "00001116-0000-1000-8000-00805f9b34fb" => BluetoothUuid::NetworkingNap,
            "00001108-0000-1000-8000-00805f9b34fb" => BluetoothUuid::HspHs,
            "00001112-0000-1000-8000-00805f9b34fb" => BluetoothUuid::HspAg,
            "4de17a00-52cb-11e6-bdf4-0800200c9a66" => BluetoothUuid::AndroidAuto,
            _ => BluetoothUuid::Unknown(s.to_string()),
        }
    }
}

#[cfg(target_os = "android")]
impl From<ParcelUuid> for BluetoothUuid {
    fn from(value: ParcelUuid) -> Self {
        BluetoothUuid::from_str(&value.to_string().unwrap())
    }
}

#[cfg(target_os = "android")]
pub struct ParcelUuid {
    internal: jni::objects::GlobalRef,
    java: Arc<Mutex<Java>>,
}

#[cfg(target_os = "android")]
impl std::fmt::Display for ParcelUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_string() {
            Ok(s) => f.write_str(&s),
            Err(e) => f.write_str(&format!("ERR: {}", e)),
        }
    }
}

#[cfg(target_os = "android")]
impl ParcelUuid {
    pub fn new(uuid: jni::objects::GlobalRef, java: Arc<Mutex<Java>>) -> Self {
        Self {
            internal: uuid,
            java,
        }
    }

    pub fn to_string(&self) -> Result<String, std::io::Error> {
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| {
            let dev_name = env
                .call_method(&self.internal, "toString", "()Ljava/lang/String;", &[])
                .get_object(env)
                .map_err(|e| jerr(env, e))?;
            if dev_name.is_null() {
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            dev_name.get_string(env).map_err(|e| jerr(env, e))
        })
    }
}
