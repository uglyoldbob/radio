use std::sync::Arc;
use std::sync::Mutex;

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;
use futures::SinkExt;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

pub struct Video {
    image: Arc<Mutex<Vec<u8>>>,
    texture: Option<egui::TextureHandle>,
}

impl Video {
    pub fn new() -> Self {
        let mut image = Arc::new(Mutex::new(Vec::new()));
        let i2 = image.clone();
        std::thread::spawn(move || {
            let mut dev = Device::new(0).expect("Failed to open video device");

            let mut fmt = dev.format().expect("Failed to read format");
            fmt.width = 320;
            fmt.height = 240;
            fmt.fourcc = FourCC::new(b"YUYV");
            let fmt = dev.set_format(&fmt).expect("Failed to write format");

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
                let (buf, meta) = stream.next().unwrap();
                let mut buf_rgb = vec![0u8; buf.len() / 2 * 3];
                if let Ok(mut i) = i2.lock() {
                    *i = vec![0; buf_rgb.len()];
                    let a: &mut [u8] = &mut i;
                    let b = buf
                        .iter()
                        .copied()
                        .pixels::<ffimage_yuv::yuv422::Yuyv<u8>>()
                        .colorconvert::<[ffimage_yuv::yuv::Yuv<u8>; 2]>();
                    let c = b.flatten();
                    let d = c.colorconvert::<ffimage::color::Rgb<u8>>();
                    let e = d.bytes();
                    e.write(a);
                }
            }
        });
        Self {
            image,
            texture: None,
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
                if i.len() != 0 {
                    let image = egui::ColorImage {
                        size: [320 as usize, 240 as usize],
                        pixels: i
                            .chunks_exact(3)
                            .map(|i| {
                                egui::Color32::from_rgb(i[0], i[1], i[2])
                            })
                            .collect(),
                    };
                    if let None = self.texture {
                        self.texture =
                            Some(ctx.load_texture("camera0", image, egui::TextureOptions::LINEAR));
                    } else if let Some(t) = &mut self.texture {
                        t.set_partial([0, 0], image, egui::TextureOptions::LINEAR);
                    }
                }
            }
            if let Some(t) = &self.texture {
                let size = ui.available_size();
                let zoom = (size.x / 320.0).min(size.y / 240.0);
                let r = ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                    id: t.id(),
                    size: egui::Vec2 {
                        x: 320.0 * zoom,
                        y: 240.0 * zoom,
                    },
                }));
            }
        });
        None
    }
}
