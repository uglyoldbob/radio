use std::sync::Arc;
use std::sync::Mutex;

use eframe::egui;
use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;
use uobradio_comms::v4l;
use uobradio_comms::video::ControlElement;
use uobradio_comms::video::SendableVideoSource;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

pub enum VideoMessage {
    Quit,
    ControlData { id: u32, value: v4l::control::Value },
}

pub struct VideoSource {
    pub image: Arc<Mutex<uobradio_comms::video::VideoFrame>>,
    pub vsend: std::sync::mpsc::Sender<VideoMessage>,
    pub controls: Vec<ControlElement>,
}

impl VideoSource {
    pub fn sendable(&self) -> Option<uobradio_comms::video::SendableVideoSource> {
        let img = self.image.lock().ok()?;
        Some(uobradio_comms::video::SendableVideoSource {
            image: Some(img.clone()), controls: self.controls.iter().map(|a| a.into()).collect()
        })
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.vsend.send(VideoMessage::Quit).unwrap();
    }
}

pub struct Video {
    which_video: usize,
}

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
            .filter_map(|c| ControlElement::new(c, dev.control(c.id).ok().map(|a| a.value)).ok())
            .collect();
        std::thread::spawn(move || {
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
                let (buf, _) = stream.next().unwrap();
                if let Ok(mut i) = i2.lock() {
                    i.pixel_data = Some(uobradio_comms::video::PixelData::Yuyv(buf.to_vec()).to_rgb());
                    i.mirroring();
                }
                if let Ok(a) = b.try_recv() {
                    match a {
                        VideoMessage::Quit => break,
                        VideoMessage::ControlData { id, value } => {
                            dev.set_control(v4l::control::Control { id, value });
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

    pub fn new() -> Self {
        Self { which_video: 0 }
    }
}
