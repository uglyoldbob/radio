//! bluetooth socket code on android

use super::super::Java;
use super::jerr;
use jni_min_helper::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, SystemTime},
};

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
    read_callback: Arc<Mutex<Option<super::ReadCallback>>>,      // None by default
    read_timeout: Duration,                                      // set for the standard Read trait

    output_stream: jni::objects::GlobalRef,
    jmethod_write: jni::objects::JMethodID,
    jmethod_flush: jni::objects::JMethodID,
    array_write: jni::objects::GlobalRef,
    uuid: String,
    java: Arc<Mutex<Java>>,
}

impl std::fmt::Debug for BluetoothSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BluetoothSocket")
    }
}

impl BluetoothSocket {
    const ARRAY_SIZE: usize = 32 * 1024;

    pub fn build(
        obj: jni::objects::GlobalRef,
        java: Arc<Mutex<Java>>,
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
                let mut java = Java::make(app);
                Self::read_loop(&mut java, socket, input_stream, arc_buf_read, arc_callback)
            }));
            log::warn!("Done connecting");
            Ok(())
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
        }
    }

    fn read_loop(
        java: &mut Java,
        socket: jni::objects::GlobalRef,
        input_stream: jni::objects::GlobalRef,
        buf_read: Arc<Mutex<VecDeque<u8>>>,
        read_callback: Arc<Mutex<Option<super::ReadCallback>>>,
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

    fn read_callback(cb: impl AsRef<Mutex<Option<super::ReadCallback>>>, val: Option<usize>) {
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
