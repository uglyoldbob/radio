//! UUID stuff for android bluetooth

use std::sync::{Arc, Mutex};
use super::super::Java;
use jni_min_helper::*;
use super::jerr;

#[derive(Debug, PartialEq)]
pub enum Uuid {
    SPP,
    A2dpSource,
    A2dpSink,
    Base,
    HspHs,
    HspAg,
    HfpAg,
    HfpHs,
    ObexOpp,
    ObexFtp,
    ObexMas,
    ObexMns,
    ObexPse,
    ObexSync,
    AvrcpRemote,
    NetworkingNap,
    Unknown(String),
}

impl Uuid {
    pub fn as_str(&self) -> &str {
        match self {
            Uuid::SPP => "00001101-0000-1000-8000-00805F9B34FB",
            Uuid::A2dpSource => "0000110a-0000-1000-8000-00805f9b34fb",
            Uuid::HfpHs => "0000111e-0000-1000-8000-00805f9b34fb",
            Uuid::ObexOpp => "00001105-0000-1000-8000-00805f9b34fb",
            Uuid::ObexFtp => "00001106-0000-1000-8000-00805f9b34fb",
            Uuid::ObexSync => "00001104-0000-1000-8000-00805f9b34fb",
            Uuid::A2dpSink => "0000110b-0000-1000-8000-00805f9b34fb",
            Uuid::AvrcpRemote => "0000110e-0000-1000-8000-00805f9b34fb",
            Uuid::ObexPse => "0000112f-0000-1000-8000-00805f9b34fb",
            Uuid::HfpAg => "0000111f-0000-1000-8000-00805f9b34fb",
            Uuid::ObexMas => "00001132-0000-1000-8000-00805f9b34fb",
            Uuid::ObexMns => "00001133-0000-1000-8000-00805f9b34fb",
            Uuid::Base => "00000000-0000-1000-8000-00805f9b34fb",
            Uuid::NetworkingNap => "00001116-0000-1000-8000-00805f9b34fb",
            Uuid::HspHs => "00001108-0000-1000-8000-00805f9b34fb",
            Uuid::HspAg => "00001112-0000-1000-8000-00805f9b34fb",
            Uuid::Unknown(s) => s,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "00001101-0000-1000-8000-00805F9B34FB" => Uuid::SPP,
            "0000110a-0000-1000-8000-00805f9b34fb" => Uuid::A2dpSource,
            "0000111e-0000-1000-8000-00805f9b34fb" => Uuid::HfpHs,
            "00001105-0000-1000-8000-00805f9b34fb" => Uuid::ObexOpp,
            "00001106-0000-1000-8000-00805f9b34fb" => Uuid::ObexFtp,
            "00001104-0000-1000-8000-00805f9b34fb" => Uuid::ObexSync,
            "0000110b-0000-1000-8000-00805f9b34fb" => Uuid::A2dpSink,
            "0000110e-0000-1000-8000-00805f9b34fb" => Uuid::AvrcpRemote,
            "0000112f-0000-1000-8000-00805f9b34fb" => Uuid::ObexPse,
            "0000111f-0000-1000-8000-00805f9b34fb" => Uuid::HfpAg,
            "00001132-0000-1000-8000-00805f9b34fb" => Uuid::ObexMas,
            "00001133-0000-1000-8000-00805f9b34fb" => Uuid::ObexMns,
            "00000000-0000-1000-8000-00805f9b34fb" => Uuid::Base,
            "00001116-0000-1000-8000-00805f9b34fb" => Uuid::NetworkingNap,
            "00001108-0000-1000-8000-00805f9b34fb" => Uuid::HspHs,
            "00001112-0000-1000-8000-00805f9b34fb" => Uuid::HspAg,
            _ => Uuid::Unknown(s.to_string()),
        }
    }
}

impl From<ParcelUuid> for Uuid {
    fn from(value: ParcelUuid) -> Self {
        Uuid::from_str(&value.to_string().unwrap())
    }
}

pub struct ParcelUuid {
    internal: jni::objects::GlobalRef,
    java: Arc<Mutex<Java>>,
}

impl std::fmt::Display for ParcelUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_string() {
            Ok(s) => f.write_str(&s),
            Err(e) => f.write_str(&format!("ERR: {}", e)),
        }
    }
}

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