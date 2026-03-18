//! Handles cameras for the radio

use std::sync::Arc;
use std::sync::Mutex;

use uobradio_comms::v4l;
use uobradio_comms::video::ControlElement;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

/// A message sent to the camera handling thread
pub enum VideoMessage {
    /// Indicator to quit the camera thread
    Quit,
    /// An indicator that the camera is or is not being used. This will eventually cause unused camera threads to sleep.
    CameraUsed(bool),
    /// A new value for a control on a video device
    ControlData {
        /// The id for the control to update
        id: u32,
        /// The new control value
        value: v4l::control::Value,
    },
}

/// A video source for a radio
pub struct VideoSource {
    /// The most recent image for the video source
    pub image: Arc<Mutex<uobradio_comms::video::VideoFrame>>,
    /// The sender to send <VideoMessage> with
    pub vsend: std::sync::mpsc::Sender<VideoMessage>,
    /// The controls that apply to the video source
    pub controls: Vec<ControlElement>,
}

/// Manual implementation of clone for <v4l::control::Value>
pub fn clone_v4l_value(control: &v4l::control::Value) -> v4l::control::Value {
    match control {
        v4l::control::Value::None => v4l::control::Value::None,
        v4l::control::Value::Integer(v) => v4l::control::Value::Integer(*v),
        v4l::control::Value::Boolean(v) => v4l::control::Value::Boolean(*v),
        v4l::control::Value::String(v) => v4l::control::Value::String(v.clone()),
        v4l::control::Value::CompoundU8(items) => v4l::control::Value::CompoundU8(items.clone()),
        v4l::control::Value::CompoundU16(items) => v4l::control::Value::CompoundU16(items.clone()),
        v4l::control::Value::CompoundU32(items) => v4l::control::Value::CompoundU32(items.clone()),
        v4l::control::Value::CompoundPtr(items) => v4l::control::Value::CompoundPtr(items.clone()),
    }
}

impl VideoSource {
    /// Obtain a sendable version of `Self``
    pub fn sendable(&self) -> Option<uobradio_comms::video::SendableVideoSource> {
        let img = self.image.lock().ok()?;
        Some(uobradio_comms::video::SendableVideoSource {
            image: Some(img.clone()),
            controls: self.controls.iter().map(|a| a.into()).collect(),
        })
    }

    /// Send an update to the given control to the specified value.
    pub fn send_update(&mut self, id: usize, val: &v4l::control::Value) -> Option<()> {
        let c = &self.controls[id];
        let v = clone_v4l_value(val);
        self.vsend
            .send(VideoMessage::ControlData { id: c.id, value: v })
            .ok()
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.vsend.send(VideoMessage::Quit).unwrap();
    }
}

/// A placeholder struct for video operations
pub struct Video {}

impl Video {
    /// Create a video source and spawn a thread for reading images from that video source.
    pub fn video_start(dev: Device) -> Result<VideoSource, String> {
        let image = Arc::new(Mutex::new(uobradio_comms::video::VideoFrame::new()));
        let (a, b) = std::sync::mpsc::channel();
        let i2 = image.clone();
        let mut fmt = dev.format().map_err(|e| e.to_string())?;
        let controls: Vec<ControlElement> = dev
            .query_controls()
            .unwrap()
            .iter()
            .filter_map(|c| {
                if let Ok(control) = dev.control(c.id) {
                    ControlElement::new(c, control.value).ok()
                } else {
                    None
                }
            })
            .collect();
        std::thread::spawn(move || {
            let mut grab_images = false;
            fmt.width = 320;
            fmt.height = 240;
            fmt.fourcc = FourCC::new(b"YUYV");
            match dev.set_format(&fmt) {
                Ok(fmt) => {
                    if let Ok(mut i) = i2.lock() {
                        i.width = fmt.width as u16;
                        i.height = fmt.height as u16;
                    }
                    println!("Video caps: {:?}", dev.query_caps());

                    println!("Video controls: {:?}", dev.query_controls());
                    println!("Video formats: {:?}", dev.enum_formats());
                    println!(
                        "Video framesizes YUYV: {:?}",
                        dev.enum_framesizes(FourCC::new(b"YUYV"))
                    );
                    let mut stream = MmapStream::with_buffers(&dev, Type::VideoCapture, 4)
                        .expect("Failed to create video buffer stream");
                    loop {
                        if grab_images {
                            let (buf, _) = stream.next().unwrap();
                            if let Ok(mut i) = i2.lock() {
                                i.pixel_data = Some(
                                    uobradio_comms::video::PixelData::Yuyv(buf.to_vec()).to_rgb(),
                                );
                                i.mirroring();
                            }
                        } else {
                            // prevent high cpu usage when inactive
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        if let Ok(a) = b.try_recv() {
                            match a {
                                VideoMessage::CameraUsed(b) => {
                                    grab_images = b;
                                }
                                VideoMessage::Quit => break,
                                VideoMessage::ControlData { id, value } => {
                                    let v2 = clone_v4l_value(&value);
                                    let _ =
                                        dev.set_control(v4l::control::Control { id, value: v2 });
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    service::log::error!("Failed to set format: {e:?}");
                }
            }
        });
        Ok(VideoSource {
            image,
            vsend: a,
            controls,
        })
    }
}
