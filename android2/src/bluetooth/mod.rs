//! Bluetooth module for android code wanting to do stuff with bluetooth.
//! Adapted from https://github.com/wuwbobo2021/android-bluetooth-serial-rs

use std::collections::BTreeMap;
use std::convert::TryInto;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

mod socket;
use socket::BluetoothSocket;

mod uuid;
use uuid::ParcelUuid;
pub use uuid::Uuid;

pub struct Bluetooth {
    adapter: OnceLock<jni::objects::GlobalRef>,
    java: Arc<Mutex<super::Java>>,
    /// An instance of Intent, created with registerReceiver
    receiver: Option<jni::objects::GlobalRef>,
    /// The broadcast_receiver for the bluetooth uuid
    blue_uuid_receiver: Option<jni_min_helper::BroadcastReceiver>,
}

use jni_min_helper::*;

type ReadCallback = Box<dyn Fn(Option<usize>) + 'static + Send>;

pub struct BluetoothDevice {
    internal: jni::objects::GlobalRef,
    rfcomm_sockets: BTreeMap<String, BluetoothSocket>,
    java: Arc<Mutex<super::Java>>,
}

impl BluetoothDevice {
    pub fn get_uuids(&mut self) -> Result<Vec<ParcelUuid>, std::io::Error> {
        let java2 = self.java.clone();
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| {
            let objs = env
                .call_method(&self.internal, "getUuids", "()[Landroid/os/ParcelUuid;", &[])
                .get_object(env)
                .map_err(|e| jerr(env, e))?;
            let jarr: &jni::objects::JObjectArray = objs.as_ref().into();
            let len = env.get_array_length(jarr).map_err(|e| jerr(env, e))?;
            let mut vec = Vec::with_capacity(len as usize);
            for i in 0..len {
                let uuid = env.get_object_array_element(jarr, i).global_ref(env).map_err(|e| jerr(env, e))?;
                log::error!("UUID {} is {:?}", i, uuid);
                vec.push(ParcelUuid::new(uuid, java2.clone()));
            }
            Ok(vec)
        })
    }

    pub fn get_name(&self) -> Result<String, std::io::Error> {
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| {
            let dev_name = env
                .call_method(&self.internal, "getName", "()Ljava/lang/String;", &[])
                .get_object(env)
                .map_err(|e| jerr(env, e))?;
            if dev_name.is_null() {
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            dev_name.get_string(env).map_err(|e| jerr(env, e))
        })
    }

    pub fn get_uuids_with_sdp(&self) {
        let mut java = self.java.lock().unwrap();
        let result = java.use_env(|env, _context| {
            let dev_name = env
                .call_method(&self.internal, "fetchUuidsWithSdp", "()Z", &[])
                .get_boolean();
            dev_name.map_err(|e| jerr(env, e))
        });
        log::error!("get uuids returned {:?}", result);
    }

    pub fn get_bond_state(&self) -> Result<i32, std::io::Error> {
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| {
            let dev_name = env
                .call_method(&self.internal, "getBondState", "()I", &[])
                .get_int();
            dev_name.map_err(|e| jerr(env, e))
        })
    }

    /// Creates the Android Bluetooth API socket object for RFCOMM communication.
    /// `SPP_UUID` can be used. Note that `connect` is not called automatically.
    pub fn get_rfcomm_socket(
        &mut self,
        uuid: Uuid,
        is_secure: bool,
    ) -> Option<&mut BluetoothSocket> {
        let uuid = uuid.to_str();
        log::warn!("Checking rfcomm for {}", uuid);
        let mut java = self.java.lock().unwrap();
        if !self.rfcomm_sockets.contains_key(uuid) {
            log::warn!("Building rfcomm for {}", uuid);
            let socket = java
                .use_env(|env, _context| {
                    let uuid = uuid.new_jobject(env).map_err(|e| jerr(env, e))?;
                    let uuid = env
                        .call_static_method(
                            "java/util/UUID",
                            "fromString",
                            "(Ljava/lang/String;)Ljava/util/UUID;",
                            &[(&uuid).into()],
                        )
                        .get_object(env)
                        .map_err(|e| jerr(env, e))?;

                    let method_name = if is_secure {
                        "createRfcommSocketToServiceRecord"
                    } else {
                        "createInsecureRfcommSocketToServiceRecord"
                    };
                    env.call_method(
                        &self.internal,
                        method_name,
                        "(Ljava/util/UUID;)Landroid/bluetooth/BluetoothSocket;",
                        &[(&uuid).into()],
                    )
                    .get_object(env)
                    .globalize(env)
                    // TODO: distinguish IOException and other unexpected exceptions
                    .map_err(|e| jerr(env, e))
                })
                .ok()?;
            drop(java);
            log::warn!("Building2 rfcomm for {}", uuid);
            let socket = BluetoothSocket::build(socket, self.java.clone(), uuid);
            if let Ok(a) = socket {
                self.rfcomm_sockets.insert(uuid.to_string(), a);
            }
            log::warn!("Done building rfcomm for {}", uuid);
        }
        self.rfcomm_sockets.get_mut(uuid)
    }
}

const BLUETOOTH_SERVICE: &str = "bluetooth";

/// Maps unexpected JNI errors to `std::io::Error`.
/// (`From<jni::errors::Error>` cannot be implemented for `std::io::Error`
/// here because of the orphan rule). Side effect: `jni_last_cleared_ex()`.
#[inline(always)]
pub(crate) fn jerr(env: &mut jni::JNIEnv, err: jni::errors::Error) -> std::io::Error {
    use jni::errors::Error::*;
    if let JavaException = err {
        let err = jni_min_helper::jni_clear_ex(err);
        jni_min_helper::jni_last_cleared_ex()
            .ok_or(JavaException)
            .and_then(|ex| Ok((ex.get_class_name(env)?, ex.get_throwable_msg(env)?)))
            .map(|(cls, msg)| {
                if cls.contains("SecurityException") {
                    std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg)
                } else if cls.contains("IllegalArgumentException") {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
                } else {
                    std::io::Error::other(format!("{cls}: {msg}"))
                }
            })
            .unwrap_or(std::io::Error::other(err))
    } else {
        std::io::Error::other(err)
    }
}

impl Bluetooth {
    pub fn new(java: Arc<Mutex<super::Java>>) -> Self {
        Self {
            adapter: OnceLock::new(),
            java,
            receiver: None,
            blue_uuid_receiver: None,
        }
    }

    pub fn cancel_discovery(&mut self) {
        self.check_adapter();
        let mut java = self.java.lock().unwrap();
        if let Some(adap) = self.adapter.get() {
            java.use_env(|env, _context| {
                let _ = env
                    .call_method(adap, "cancelDiscovery", "()Z", &[])
                    .clear_ex();
            });
        }
    }

    fn check_adapter(&mut self) {
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, context| {
            if self.adapter.get().is_none() {
                let a = Self::get_adapter(env, &context).unwrap();
                log::error!("Adapter is {:?}", a);
                let _ = self.adapter.set(a);
            } else {
                log::error!("BLUETOOTH ADAPTER ALREADY SET");
            }
        });
        drop(java);
        if self.receiver.is_none() {
            let arg1 = jni_min_helper::BroadcastReceiver::build(|env, _context, intent| {
                log::error!("Broadcast receiver runs now {:?}", intent);
                let action = env
                    .call_method(intent, "getAction", "()Ljava/lang/String;", &[])
                    .get_object(env)?;
                if action.is_null() {
                    return Err(jni::errors::Error::NullPtr("No action"));
                }
                let action = action.get_string(env).map_err(|e| jerr(env, e));
                log::error!("Action is {:?}", action);
                Ok(())
            })
            .unwrap();
            let r = register_receiver(&self.java, &arg1);
            self.blue_uuid_receiver.replace(arg1);
            if let Some(r) = r {
                log::error!("Receiver is {:?}", r);
                self.receiver.replace(r);
            }
        }
    }

    pub fn enable(&mut self) {
        if !self.is_enabled() {
            log::error!("Bluetooth not enabled. Not implemented yet");
            //let mut java = self.java.lock().unwrap();
            /*  java code sample
               Intent enableBtIntent = new Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE);
               startActivityForResult(enableBtIntent, REQUEST_ENABLE_BT);
            */
            todo!();
        }
    }

    pub fn is_enabled(&mut self) -> bool {
        self.check_adapter();
        let mut java = self.java.lock().unwrap();
        java.use_env::<bool, _>(|env, _context| -> bool {
            let adapter = self.adapter.get().unwrap().as_obj();
            let a = env
                .call_method(adapter, "isEnabled", "()Z", &[])
                .get_boolean()
                .map_err(|e| jerr(env, e));
            a.unwrap()
        })
    }

    pub fn get_bonded_devices(&mut self) -> Option<Vec<BluetoothDevice>> {
        self.check_adapter();
        let mut java = self.java.lock().unwrap();
        java.use_env(
            |env, _context| -> Result<Vec<BluetoothDevice>, std::io::Error> {
                let adapter = self.adapter.get().unwrap().as_obj();
                let dev_set = env
                    .call_method(adapter, "getBondedDevices", "()Ljava/util/Set;", &[])
                    .get_object(env)
                    .map_err(|e| jerr(env, e))?;
                if dev_set.is_null() {
                    return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
                }
                let jarr = env
                    .call_method(&dev_set, "toArray", "()[Ljava/lang/Object;", &[])
                    .get_object(env)
                    .map_err(|e| jerr(env, e))?;
                let jarr: &jni::objects::JObjectArray = jarr.as_ref().into();
                let len = env.get_array_length(jarr).map_err(|e| jerr(env, e))?;
                let mut vec = Vec::with_capacity(len as usize);
                for i in 0..len {
                    vec.push(BluetoothDevice {
                        internal: env
                            .get_object_array_element(jarr, i)
                            .global_ref(env)
                            .map_err(|e| jerr(env, e))?,
                        rfcomm_sockets: BTreeMap::new(),
                        java: self.java.clone(),
                    });
                }
                Ok(vec)
            },
        )
        .ok()
    }

    fn get_adapter<'a>(
        env: &mut jni::JNIEnv<'a>,
        context: &jni::objects::JObject,
    ) -> Result<jni::objects::GlobalRef, std::io::Error> {
        let bluetooth_service = BLUETOOTH_SERVICE
            .new_jobject(env)
            .map_err(|e| jerr(env, e))?;
        let manager = env
            .call_method(
                context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[(&bluetooth_service).into()],
            )
            .get_object(env)
            .map_err(|e| jerr(env, e))?;
        if manager.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Cannot get BLUETOOTH_SERVICE",
            ));
        }
        let adapter = env
            .call_method(
                manager,
                "getAdapter",
                "()Landroid/bluetooth/BluetoothAdapter;",
                &[],
            )
            .get_object(env)
            .map_err(|e| jerr(env, e))?;
        if !adapter.is_null() {
            Ok(env.new_global_ref(&adapter).map_err(|e| jerr(env, e))?)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "`getAdapter` returned null",
            ))
        }
    }
}

fn register_receiver(
    java: &Arc<Mutex<super::Java>>,
    arg1: &jni_min_helper::BroadcastReceiver,
) -> Option<jni::objects::GlobalRef> {
    let mut java2 = java.lock().unwrap();
    let mut sig = String::new();
    sig.push_str("(");
    sig.push_str("Landroid/content/BroadcastReceiver;");
    sig.push_str("Landroid/content/IntentFilter;");
    sig.push_str(")Landroid/content/Intent;");
    java2.use_env(|env, context| {
        let mut args = Vec::new();
        let intent_str = "android.bluetooth.device.action.UUID"
            .new_jobject(env)
            .unwrap();
        let arg2 = env.new_object(
            "android/content/IntentFilter",
            "(Ljava/lang/String;)V",
            &[(&intent_str).into()],
        );
        let arg2 = arg2.unwrap();
        args.push(arg1.as_ref());
        args.push(&arg2);
        let args2: Vec<jni::objects::JValueGen<&jni::objects::JObject>> =
            args.iter().map(|a| a.try_into().unwrap()).collect();
        let e = env
            .call_method(context, "registerReceiver", &sig, args2.as_slice())
            .get_object(env)
            .map_err(|e| jerr(env, e))
            .ok()?;
        env.new_global_ref(&e).map_err(|e| jerr(env, e)).ok()
    })
}
