use std::sync::Arc;
use std::sync::Mutex;

use super::SubwindowTrait;
use super::CommonWindowProperties;
use super::Subwindow;
use eframe::egui;

use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;
use futures::SinkExt;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Output;
use v4l::FourCC;

pub struct Video {
    image: Arc<Mutex<Vec<u8>>>,
}

impl Video {
    pub fn new() -> Self {
        let mut image = Arc::new(Mutex::new(Vec::new()));
        let i2 = image.clone();
        std::thread::spawn(move || {
            let mut dev = Device::new(0).expect("Failed to open video device");
            println!("Video caps: {:?}", dev.query_caps());
            println!("Video controls: {:?}", dev.query_controls());
            println!("Video framesizes: {:?}", dev.enum_framesizes(FourCC::new(b"YUYV")));
            let mut stream = MmapStream::with_buffers(&mut dev, Type::VideoCapture, 4).expect("Failed to create video buffer stream");
            loop {
                let (buf, meta) = stream.next().unwrap();
                let mut buf_rgb = vec![0; buf.len()];
                if let Ok(mut i) = i2.lock() {
                    *i = vec![0; buf.len()];
                    let a: &mut [u8] = &mut i;
                    buf.iter().copied().pixels::<ffimage_yuv::yuv::Yuv<u8>>().colorconvert::<ffimage::color::Rgb<u8>>().bytes().write(a);
                }
                println!(
                    "Video buffer size: {}, seq: {}, timestamp: {}",
                   buf.len(),
                   meta.sequence,
                   meta.timestamp
               );
            }
        });
        Self {
            image,
        }
    }
}

impl SubwindowTrait for Video {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        common: &mut CommonWindowProperties,
    ) -> Option<Subwindow> {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("This is the video page");
            if let Ok(i) = self.image.lock() {
                let is = egui::ImageSource::Bytes {
                    uri: "video0".into(),
                    bytes: i.clone().into(),
                };
                ui.image(is);
            }
        });
        None
    }
}
