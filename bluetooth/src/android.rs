//! Android specific bluetooth code

use winit::platform::android::activity::AndroidApp;
use jni_min_helper::*;

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

#[ouroboros::self_referencing]
pub struct Java {
    app: AndroidApp,
    java: jni::JavaVM,
    #[borrows(java)]
    #[not_covariant]
    env: jni::AttachGuard<'this>,
}

impl Java {
    /// Use the java environment with a closure that returns a type. Generally used to make calls to java code.
    pub fn use_env<T, F: FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> T>(
        &mut self,
        f: F,
    ) -> T {
        let context = unsafe {
            jni::objects::JObject::from_raw(
                self.borrow_app().activity_as_ptr() as *mut jni::sys::_jobject
            )
        };
        self.with_env_mut(|a| f(a, context))
    }

    /// Retrieve a clone of the androidapp object
    pub fn get_app(&self) -> AndroidApp {
        self.borrow_app().clone()
    }

    /// Make a new java object using the androidapp object
    pub fn make(app: AndroidApp) -> Self {
        let vm = unsafe {
            jni::JavaVM::from_raw(app.vm_as_ptr() as *mut *const jni::sys::JNIInvokeInterface_)
        }
        .unwrap();
        JavaBuilder {
            app,
            java: vm,
            env_builder: |java: &jni::JavaVM| java.attach_current_thread().unwrap(),
        }
        .build()
    }
}

use std::convert::TryInto;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

mod socket;
pub use socket::BluetoothSocket;

use crate::uuid::ParcelUuid;

mod device;
pub use device::BluetoothDevice;

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

const BLUETOOTH_SERVICE: &str = "bluetooth";

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
            }
        });
        drop(java);
        if self.receiver.is_none() {
            let arg1 = jni_min_helper::BroadcastReceiver::build(|env, _context, intent| {
                let action = env
                    .call_method(intent, "getAction", "()Ljava/lang/String;", &[])
                    .get_object(env)?;
                if action.is_null() {
                    return Err(jni::errors::Error::NullPtr("No action"));
                }
                let action = action.get_string(env).map_err(|e| jerr(env, e));
                Ok(())
            })
            .unwrap();
            let r = register_receiver(&self.java, &arg1, "android.bluetooth.device.action.UUID");
            self.blue_uuid_receiver.replace(arg1);
            if let Some(r) = r {
                log::error!("Receiver is {:?}", r);
                self.receiver.replace(r);
            }
        }
    }

    pub fn enable(&mut self) {
        if !self.is_enabled() {
            log::error!("Bluetooth not enabled. Requesting it to be enabled");
            let mut java = self.java.lock().unwrap();
            java.use_env(|env, context| {
                let arg = "android.bluetooth.adapter.action.REQUEST_ENABLE"
                    .new_jobject(env)
                    .map_err(|e| jerr(env, e))
                    .unwrap();
                let intent = env
                    .new_object(
                        "android/content/Intent",
                        "(Ljava/lang/String;)V",
                        &[(&arg).into()],
                    )
                    .unwrap();
                let mut args = Vec::new();
                args.push(&intent);
                let mut args2: Vec<jni::objects::JValueGen<&jni::objects::JObject>> =
                    args.iter().map(|a| a.try_into().unwrap()).collect();
                args2.push(1.into());
                let a = env.call_method(
                    context,
                    "startActivityForResult",
                    "(Landroid/content/Intent;I)V",
                    args2.as_slice(),
                );
                log::error!("Results of bluetooth enable is {:?}", a);
            })
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
                    vec.push(BluetoothDevice::new(
                        env.get_object_array_element(jarr, i)
                            .global_ref(env)
                            .map_err(|e| jerr(env, e))?,
                        self.java.clone(),
                    ));
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
    intent_str: &str,
) -> Option<jni::objects::GlobalRef> {
    let mut java2 = java.lock().unwrap();
    let mut sig = String::new();
    sig.push_str("(");
    sig.push_str("Landroid/content/BroadcastReceiver;");
    sig.push_str("Landroid/content/IntentFilter;");
    sig.push_str(")Landroid/content/Intent;");
    java2.use_env(|env, context| {
        let mut args = Vec::new();
        let intent_str = intent_str.new_jobject(env).unwrap();
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
