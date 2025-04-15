use std::sync::OnceLock;

pub struct Bluetooth {
    adapter: OnceLock<jni::objects::GlobalRef>,
}

use jni_min_helper::*;

#[derive(Clone, Debug)]
pub struct BluetoothDevice {
    internal: jni::objects::GlobalRef,
}

impl BluetoothDevice {
    pub fn getName(&self, java: &mut super::Java) -> Result<String, std::io::Error> {
        java.use_env(|env, context| {
            let dev_name = env
                .call_method(&self.internal, "getName", "()Ljava/lang/String;", &[])
                .get_object(env)
                .map_err(|e|jerr(env, e))?;
            if dev_name.is_null() {
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            dev_name.get_string(env).map_err(|e|jerr(env, e))
        })
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
    pub fn new() -> Self {
        Self {
            adapter: OnceLock::new(),
        }
    }

    pub fn do_test(&mut self, java: &mut crate::Java) {
        java.use_env(|env, context| {
            self.check_adapter(env, &context);
        })
    }

    fn check_adapter(&mut self, env: &mut jni::JNIEnv, context: &jni::objects::JObject) {
        if self.adapter.get().is_none() {
            let a = self.get_adapter(env, &context).unwrap();
            log::error!("Adapter is {:?}", a);
            self.adapter.set(a);
        }
        else {
            log::error!("BLUETOOTH ADAPTER ALREADY SET");
        }
    }

    pub fn enable(&mut self, java: &mut crate::Java) {
        if !self.isEnabled(java) {
            log::error!("Bluetooth not enabled. Not implemented yet");
            /*  java code sample
                Intent enableBtIntent = new Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE);
                startActivityForResult(enableBtIntent, REQUEST_ENABLE_BT);
             */
            todo!();
        }
    }

    pub fn isEnabled(&mut self, java: &mut crate::Java) -> bool {
        java.use_env::<bool,_>(|env, context| -> bool {
            self.check_adapter(env, &context);
            let adapter = self.adapter.get().unwrap().as_obj();
            let a = env.call_method(adapter, "isEnabled", "()Z", &[])
                .get_boolean()
                .map_err(|e| jerr(env, e));
            a.unwrap()
        })
    }

    pub fn getBondedDevices(&mut self, java: &mut crate::Java) -> Option<Vec<BluetoothDevice>> {
        java.use_env(|env, context| -> Result<Vec<BluetoothDevice>, std::io::Error> {
            self.check_adapter(env, &context);
            let adapter = self.adapter.get().unwrap().as_obj();
            let dev_set = env
                .call_method(adapter, "getBondedDevices", "()Ljava/util/Set;", &[])
                .get_object(env)
                .map_err(|e|jerr(env, e))?;
            if dev_set.is_null() {
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            let jarr = env
                .call_method(&dev_set, "toArray", "()[Ljava/lang/Object;", &[])
                .get_object(env)
                .map_err(|e|jerr(env, e))?;
            let jarr: &jni::objects::JObjectArray = jarr.as_ref().into();
            let len = env.get_array_length(jarr).map_err(|e|jerr(env, e))?;
            let mut vec = Vec::with_capacity(len as usize);
            for i in 0..len {
                vec.push(BluetoothDevice {
                    internal: env
                        .get_object_array_element(jarr, i)
                        .global_ref(env)
                        .map_err(|e|jerr(env, e))?,
                });
            }
            Ok(vec)
        }).ok()
    }

    fn get_adapter<'a>(&mut self, env: &mut jni::JNIEnv<'a>, context: &jni::objects::JObject) -> Result<jni::objects::GlobalRef, std::io::Error> {
        let bluetooth_service = BLUETOOTH_SERVICE.new_jobject(env).map_err(|e| jerr(env, e))?;
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
            .get_object(env).map_err(|e| jerr(env, e))?;
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
