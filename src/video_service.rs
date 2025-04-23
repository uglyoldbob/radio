use std::sync::Arc;
use std::sync::Mutex;

use uobradio_comms::v4l;
use uobradio_comms::video::ControlElement;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

pub enum VideoMessage {
    Quit,
    CameraUsed(bool),
    ControlData { id: u32, value: v4l::control::Value },
}

pub struct VideoSource {
    pub image: Arc<Mutex<uobradio_comms::video::VideoFrame>>,
    pub vsend: std::sync::mpsc::Sender<VideoMessage>,
    pub controls: Vec<ControlElement>,
}

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
    pub fn sendable(&self) -> Option<uobradio_comms::video::SendableVideoSource> {
        let img = self.image.lock().ok()?;
        Some(uobradio_comms::video::SendableVideoSource {
            image: Some(img.clone()),
            controls: self.controls.iter().map(|a| a.into()).collect(),
        })
    }

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

pub struct Video {}

impl Video {
    pub fn video_start(mut dev: Device) -> VideoSource {
        let image = Arc::new(Mutex::new(uobradio_comms::video::VideoFrame::new()));
        let (a, b) = std::sync::mpsc::channel();
        let i2 = image.clone();
        let mut fmt = dev.format().expect("Failed to read format");
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
            let mut grab_images = true;
            fmt.width = 320;
            fmt.height = 240;
            fmt.fourcc = FourCC::new(b"YUYV");
            let fmt = dev.set_format(&fmt).expect("Failed to write format");

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
            let mut stream = MmapStream::with_buffers(&mut dev, Type::VideoCapture, 4)
                .expect("Failed to create video buffer stream");
            loop {
                if grab_images {
                    let (buf, _) = stream.next().unwrap();
                    if let Ok(mut i) = i2.lock() {
                        i.pixel_data =
                            Some(uobradio_comms::video::PixelData::Yuyv(buf.to_vec()).to_rgb());
                        i.mirroring();
                    }
                }
                if let Ok(a) = b.try_recv() {
                    match a {
                        VideoMessage::CameraUsed(b) => {
                            grab_images = b;
                        }
                        VideoMessage::Quit => break,
                        VideoMessage::ControlData { id, value } => {
                            let v2 = clone_v4l_value(&value);
                            let _ = dev.set_control(v4l::control::Control { id, value: v2 });
                        }
                    }
                }
            }
        });
        VideoSource {
            image,
            vsend: a,
            controls,
        }
    }
}
