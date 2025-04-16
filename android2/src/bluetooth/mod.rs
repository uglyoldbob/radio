//! Bluetooth module for android code wanting to do stuff with bluetooth.
//! Adapted from https://github.com/wuwbobo2021/android-bluetooth-serial-rs

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::SystemTime;

pub struct Bluetooth {
    adapter: OnceLock<jni::objects::GlobalRef>,
    java: Arc<Mutex<super::Java>>,
    /// An instance of Intent, created with registerReceiver
    receiver: Option<jni::objects::GlobalRef>,
    /// The broadcast_receiver for the bluetooth uuid
    blue_uuid_receiver: Option<jni_min_helper::BroadcastReceiver>,
}

/// The UUID for the well-known SPP profile.
pub const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";

use jni_min_helper::*;

type ReadCallback = Box<dyn Fn(Option<usize>) + 'static + Send>;

/// Manages the Bluetooth socket and IO streams. It uses a read buffer and a background thread,
/// because the timeout of the Java `InputStream` from the `BluetoothSocket` cannot be set.
/// The read timeout defaults to 0 (it does not block).
///
/// Reference:
/// <https://developer.android.com/develop/connectivity/bluetooth/transfer-data>
pub struct BluetoothSocket {
    internal: jni::objects::GlobalRef,

    input_stream: jni::objects::GlobalRef,
    buf_read: Arc<Mutex<VecDeque<u8>>>,
    thread_read: Option<JoinHandle<Result<(), std::io::Error>>>, // the returned value is unused
    read_callback: Arc<Mutex<Option<ReadCallback>>>,             // None by default
    read_timeout: Duration,                                      // set for the standard Read trait

    output_stream: jni::objects::GlobalRef,
    jmethod_write: jni::objects::JMethodID,
    jmethod_flush: jni::objects::JMethodID,
    array_write: jni::objects::GlobalRef,
    uuid: String,
    java: Arc<Mutex<super::Java>>,
}

impl std::fmt::Debug for BluetoothSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BluetoothSocket")
    }
}

impl BluetoothSocket {
    const ARRAY_SIZE: usize = 32 * 1024;

    fn build(
        obj: jni::objects::GlobalRef,
        java: Arc<Mutex<super::Java>>,
        uuid: &str,
    ) -> Result<Self, std::io::Error> {
        let mut java2 = java.lock().unwrap();
        let input_stream = java2.use_env(|env, _context| {
            // the streams may (or may NOT) be usable after reconnection (check Android SDK source)
            env.call_method(&obj, "getInputStream", "()Ljava/io/InputStream;", &[])
                .get_object(env)
                .globalize(env)
                .map_err(|e| jerr(env, e))
        })?;
        let output_stream = java2.use_env(|env, _context| {
            env.call_method(&obj, "getOutputStream", "()Ljava/io/OutputStream;", &[])
                .get_object(env)
                .globalize(env)
                .map_err(|e| jerr(env, e))
        })?;
        let jmethod_write = java2.use_env(|env, _context| {
            env.get_method_id("java/io/OutputStream", "write", "([BII)V")
                .map_err(|e| jerr(env, e))
        })?;
        let jmethod_flush = java2.use_env(|env, _context| {
            env.get_method_id("java/io/OutputStream", "flush", "()V")
                .map_err(|e| jerr(env, e))
        })?;

        let array_size = Self::ARRAY_SIZE as i32;
        let array_write = java2.use_env(|env, _context| {
            env.new_byte_array(array_size)
                .global_ref(env)
                .map_err(|e| jerr(env, e))
        })?;
        drop(java2);
        Ok(Self {
            internal: obj,

            input_stream,
            buf_read: Arc::new(Mutex::new(VecDeque::new())),
            thread_read: None,
            read_callback: Arc::new(Mutex::new(None)),
            read_timeout: Duration::from_millis(0),

            output_stream,
            jmethod_write,
            jmethod_flush,
            array_write,
            uuid: uuid.to_string(),
            java,
        })
    }

    /// Gets the connection status of this socket.
    #[inline(always)]
    pub fn is_connected(&self) -> Result<bool, std::io::Error> {
        let mut java2 = self.java.lock().unwrap();
        java2.use_env(|env, _context| self.is_connected2(env))
    }

    /// Gets the connection status of this socket.
    #[inline(always)]
    fn is_connected2(&self, env: &mut jni::JNIEnv) -> Result<bool, std::io::Error> {
        env.call_method(&self.internal, "isConnected", "()Z", &[])
            .get_boolean()
            .map_err(|e| jerr(env, e))
    }

    /// Attempts to connect to a remote device. When connected, it creates a
    /// backgrond thread for reading data, which terminates itself on disconnection.
    /// Do not reuse the socket after disconnection, because the underlying OS
    /// implementation is probably incapable of reconnecting the device, just like
    /// `java.net.Socket`.
    pub fn connect(&mut self) -> Result<(), std::io::Error> {
        if self.is_connected()? {
            return Ok(());
        }
        let mut java = self.java.lock().unwrap();
        log::warn!("Connecting to {}", self.uuid);
        let app = java.get_app();
        let connected = java.use_env(|env, _context| {
            env.call_method(&self.internal, "connect", "()V", &[])
                .map_err(|e| jerr(env, e))
                .inspect_err(|e| log::error!("Connect error is {:?}", e))?;
            self.is_connected2(env)
        })?;
        log::warn!("Connected status is {}", connected);
        if connected {
            let socket = self.internal.clone();
            let input_stream = self.input_stream.clone();
            let arc_buf_read = self.buf_read.clone();
            let arc_callback = self.read_callback.clone();
            self.thread_read.replace(std::thread::spawn(move || {
                let mut java = super::Java::make(app);
                Self::read_loop(&mut java, socket, input_stream, arc_buf_read, arc_callback)
            }));
            log::warn!("Done connecting");
            Ok(())
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
        }
    }

    fn read_loop(
        java: &mut super::Java,
        socket: jni::objects::GlobalRef,
        input_stream: jni::objects::GlobalRef,
        buf_read: Arc<Mutex<VecDeque<u8>>>,
        read_callback: Arc<Mutex<Option<ReadCallback>>>,
    ) -> Result<(), std::io::Error> {
        java.use_env(|env, _context| {
            let jmethod_read = env
                .get_method_id("java/io/InputStream", "read", "([BII)I")
                .map_err(|e| jerr(env, e))?;
            let read_size = env
                .call_method(&socket, "getMaxReceivePacketSize", "()I", &[])
                .get_int()
                .map(|i| {
                    if i > 0 {
                        let sz = i as usize;
                        (Self::ARRAY_SIZE / sz) * sz
                    } else {
                        Self::ARRAY_SIZE
                    }
                })
                .unwrap_or(Self::ARRAY_SIZE);

            let mut vec_read = vec![0u8; read_size];
            let array_read = env
                .new_byte_array(read_size as i32)
                .auto_local(env)
                .map_err(|e| jerr(env, e))?;
            let array_read: &jni::objects::JByteArray<'_> = array_read.as_ref().into();

            loop {
                use jni::signature::*;
                // Safety: arguments passed to `call_method_unchecked` are correct.
                let read_len = unsafe {
                    env.call_method_unchecked(
                        &input_stream,
                        jmethod_read,
                        ReturnType::Primitive(Primitive::Int),
                        &[
                            jni::sys::jvalue {
                                l: array_read.as_raw(),
                            },
                            jni::sys::jvalue {
                                i: 0 as jni::sys::jint,
                            },
                            jni::sys::jvalue {
                                i: read_size as jni::sys::jint,
                            },
                        ],
                    )
                }
                .get_int();
                if let Ok(len) = read_len {
                    use std::io::Write;
                    let len = if len > 0 {
                        len as usize
                    } else {
                        continue;
                    };
                    // Safety: casts `&mut [u8]` to `&mut [i8]` for `get_byte_array_region`,
                    // `input_stream.read(..)` = `len` <= `read_size` = `vec_read.len()`.
                    let tmp_read = unsafe {
                        std::slice::from_raw_parts_mut(vec_read.as_mut_ptr() as *mut i8, len)
                    };
                    env.get_byte_array_region(array_read, 0, tmp_read)
                        .map_err(|e| jerr(env, e))?;
                    buf_read
                        .lock()
                        .unwrap()
                        .write_all(&vec_read[..len])
                        .unwrap();
                    Self::read_callback(&read_callback, Some(len));
                } else {
                    if let Some(ex) = jni_last_cleared_ex() {
                        let ex_msg = ex.get_throwable_msg(env).unwrap().to_lowercase();
                        if ex_msg.contains("closed") {
                            // Note: will it change in future Android versions?
                            let _ = env
                                .call_method(&socket, "close", "()V", &[])
                                .map_err(jni_clear_ex_ignore);
                            Self::read_callback(&read_callback, None);
                            return Ok(());
                        }
                    }
                    let is_connected = env
                        .call_method(&socket, "isConnected", "()Z", &[])
                        .get_boolean()
                        .map_err(|e| jerr(env, e))?;
                    if !is_connected {
                        Self::read_callback(&read_callback, None);
                        return Ok(());
                    }
                }
            }
        })
    }

    fn read_callback(cb: impl AsRef<Mutex<Option<ReadCallback>>>, val: Option<usize>) {
        let mut lck = cb.as_ref().lock().unwrap();
        if let Some(callback) = lck.take() {
            drop(lck);
            callback(val);
            let mut lck = cb.as_ref().lock().unwrap();
            if lck.is_none() {
                lck.replace(callback);
            }
        }
    }

    /// Closes this socket and releases any system resources associated with it.
    /// If the stream is already closed then invoking this method has no effect.
    pub fn close(&mut self) -> Result<(), std::io::Error> {
        use std::io::Write;
        if !self.is_connected()? {
            return Ok(());
        }
        let _ = self.flush();
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| -> Result<(), std::io::Error> {
            env.call_method(&self.internal, "close", "()V", &[])
                .clear_ex()
                .map_err(|e| jerr(env, e))
        })?;
        if let Some(th) = self.thread_read.take() {
            let _ = th.join();
        }
        Ok(())
    }
}

impl std::io::Read for BluetoothSocket {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let t_timeout = SystemTime::now() + self.read_timeout;

        let mut cnt_read = 0;
        let mut disconnected = false;
        while cnt_read < buf.len() {
            let mut lck_buf_read = self.buf_read.lock().unwrap();
            if let Ok(cnt) = lck_buf_read.read(&mut buf[cnt_read..]) {
                cnt_read += cnt;
            }
            drop(lck_buf_read);
            if cnt_read >= buf.len() {
                break;
            } else if !self.is_connected()? {
                disconnected = true;
                break;
            } else if let Ok(dur_rem) = t_timeout.duration_since(SystemTime::now()) {
                std::thread::sleep(Duration::from_millis(100).min(dur_rem));
            } else {
                break;
            }
        }

        if cnt_read > 0 {
            Ok(cnt_read)
        } else if !disconnected {
            Err(std::io::Error::from(std::io::ErrorKind::TimedOut))
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
        }
    }
}

impl std::io::Write for BluetoothSocket {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let array_write: &jni::objects::JByteArray<'_> = self.array_write.as_obj().into();
        let mut java = self.java.lock().unwrap();
        let al = java
            .use_env(|env, _context| env.get_array_length(array_write).map_err(|e| jerr(env, e)))?
            as usize;
        if al < buf.len() {
            self.array_write = java.use_env(|env, _context| {
                // replace the prepared reusable Java array with a larger array
                env.byte_array_from_slice(buf)
                    .global_ref(env)
                    .map_err(|e| jerr(env, e))
            })?;
        } else {
            java.use_env(|env, _context| -> std::io::Result<()> {
                // Safety: casts `&[u8]` to `&[i8]` for `set_byte_array_region`.
                let buf =
                    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const i8, buf.len()) };
                env.set_byte_array_region(array_write, 0, buf)
                    .map_err(|e| jerr(env, e))
            })?;
        }

        use jni::signature::*;
        java.use_env(|env, _context| {
            // Safety: arguments passed to `call_method_unchecked` are correct.
            unsafe {
                env.call_method_unchecked(
                    &self.output_stream,
                    self.jmethod_write,
                    ReturnType::Primitive(Primitive::Void),
                    &[
                        jni::sys::jvalue {
                            l: self.array_write.as_raw(),
                        },
                        jni::sys::jvalue {
                            i: 0 as jni::sys::jint,
                        },
                        jni::sys::jvalue {
                            i: buf.len() as jni::sys::jint,
                        },
                    ],
                )
            }
            .clear_ex()
            .map_err(|e| {
                if !self.is_connected().unwrap_or(false) {
                    std::io::Error::from(std::io::ErrorKind::NotConnected)
                } else {
                    jerr(env, e)
                }
            })
            .map(|_| buf.len())
        })
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        let mut java = self.java.lock().unwrap();
        java.use_env(|env, _context| {
            use jni::signature::*;
            unsafe {
                env.call_method_unchecked(
                    &self.output_stream,
                    self.jmethod_flush,
                    ReturnType::Primitive(Primitive::Void),
                    &[],
                )
            }
            .clear_ex()
            .map_err(|e| jerr(env, e))
        })
    }
}

impl Drop for BluetoothSocket {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub struct BluetoothDevice {
    internal: jni::objects::GlobalRef,
    rfcomm_sockets: BTreeMap<String, BluetoothSocket>,
    java: Arc<Mutex<super::Java>>,
}

impl BluetoothDevice {
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
        uuid: &str,
        is_secure: bool,
    ) -> Option<&mut BluetoothSocket> {
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
            let arg1 = jni_min_helper::BroadcastReceiver::build(|env, context, intent| {
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
