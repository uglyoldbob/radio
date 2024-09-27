use std::sync::Arc;
use std::sync::Mutex;

use super::CommonWindowProperties;
use super::Subwindow;
use super::SubwindowTrait;
use eframe::egui;

use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

#[derive(Copy, Clone)]
#[repr(C)]
struct RgbPixel {
    a: [u8; 3],
}

#[derive(Clone)]
enum PixelData {
    Yuyv(Vec<u8>),
    Rgb(Vec<u8>),
    Egui(Vec<egui::Color32>),
}

impl PixelData {
    fn yuyv_to_rgb(vec: &[u8]) -> Vec<u8> {
        let mut a = vec![0u8; vec.len() / 2 * 3];
        vec.iter()
            .copied()
            .pixels::<ffimage_yuv::yuv422::Yuyv<u8>>()
            .colorconvert::<[ffimage_yuv::yuv::Yuv<u8>; 2]>()
            .flatten()
            .colorconvert::<ffimage::color::Rgb<u8>>()
            .bytes()
            .write(&mut a);
        a
    }

    fn rgb_to_egui(vec: &[u8]) -> Vec<egui::Color32> {
        vec.chunks_exact(3)
            .map(|i| egui::Color32::from_rgb(i[0], i[1], i[2]))
            .collect()
    }

    fn to_rgb(self) -> Self {
        match self {
            PixelData::Yuyv(vec) => PixelData::Rgb(Self::yuyv_to_rgb(&vec)),
            PixelData::Rgb(vec) => PixelData::Rgb(vec),
            PixelData::Egui(vec) => todo!(),
        }
    }

    fn get_rgb(&self) -> Vec<u8> {
        match self {
            PixelData::Yuyv(vec) => Self::yuyv_to_rgb(&vec),
            PixelData::Rgb(vec) => vec.clone(),
            PixelData::Egui(_vec) => todo!(),
        }
    }

    fn to_egui(self) -> Self {
        match self {
            PixelData::Yuyv(vec) => {
                let a = Self::yuyv_to_rgb(&vec);
                PixelData::Egui(Self::rgb_to_egui(&a))
            }
            PixelData::Rgb(vec) => PixelData::Egui(Self::rgb_to_egui(&vec)),
            PixelData::Egui(vec) => PixelData::Egui(vec),
        }
    }

    fn get_egui(&self) -> Vec<egui::Color32> {
        match self {
            PixelData::Yuyv(vec) => {
                let a = Self::yuyv_to_rgb(&vec);
                Self::rgb_to_egui(&a)
            }
            PixelData::Rgb(vec) => Self::rgb_to_egui(&vec),
            PixelData::Egui(vec) => vec.clone(),
        }
    }

    fn general_mirror<T: Clone>(width: u16, hflip: bool, vflip: bool, pixels: &mut Vec<T>) {
        if hflip && !vflip {
            for e in pixels.chunks_exact_mut(width as usize) {
                e.reverse();
            }
        } else if hflip && vflip {
            *pixels = pixels
                .rchunks_exact(width as usize)
                .flat_map(|a| {
                    let mut b = a.to_vec();
                    b.reverse();
                    b
                })
                .collect();
        } else if !hflip && vflip {
            *pixels = pixels
                .rchunks_exact(width as usize)
                .flat_map(|a| a.to_vec())
                .collect();
        }
    }

    fn mirroring(&mut self, width: u16, hflip: bool, vflip: bool) {
        match self {
            PixelData::Yuyv(_vec) => todo!(),
            PixelData::Rgb(vec) => {
                let mut pixels: Vec<RgbPixel> = vec
                    .chunks_exact(3)
                    .map(|a| RgbPixel {
                        a: [a[0], a[1], a[2]],
                    })
                    .collect();
                Self::general_mirror(width, hflip, vflip, &mut pixels);
                *vec = pixels.iter().flat_map(|a| a.a).collect();
            }
            PixelData::Egui(vec) => {
                Self::general_mirror(width, hflip, vflip, vec);
            }
        }
    }
}

struct VideoFrame {
    width: u16,
    height: u16,
    pixel_data: Option<PixelData>,
    hmirror: bool,
    vmirror: bool,
}

impl VideoFrame {
    fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixel_data: None,
            hmirror: false,
            vmirror: false,
        }
    }

    fn mirroring(&mut self) {
        if let Some(pd) = &mut self.pixel_data {
            pd.mirroring(self.width, self.hmirror, self.vmirror);
        }
    }
}

pub struct Video {
    image: Arc<Mutex<VideoFrame>>,
    texture: Option<egui::TextureHandle>,
    vsend: std::sync::mpsc::Sender<bool>,
}

impl Video {
    pub fn new() -> Self {
        let image = Arc::new(Mutex::new(VideoFrame::new()));
        let (a, b) = std::sync::mpsc::channel();
        let i2 = image.clone();
        std::thread::spawn(move || {
            let mut dev = Device::new(0).expect("Failed to open video device");

            let mut fmt = dev.format().expect("Failed to read format");
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
                let (buf, meta) = stream.next().unwrap();
                if let Ok(mut i) = i2.lock() {
                    i.pixel_data = Some(PixelData::Yuyv(buf.to_vec()).to_rgb());
                    i.mirroring();
                }
                if let Ok(_a) = b.try_recv() {
                    break;
                }
            }
        });
        Self {
            image,
            texture: None,
            vsend: a,
        }
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.vsend.send(true).unwrap();
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
            let mut size = ui.available_size();

            if let Ok(i) = self.image.lock() {
                if let Some(pd) = &i.pixel_data {
                    let zoom = (size.x / (i.width as f32)).min(size.y / (i.height as f32));
                    size = egui::Vec2 {
                        x: i.width as f32 * zoom,
                        y: i.height as f32 * zoom,
                    };
                    let image = egui::ColorImage {
                        size: [i.width as usize, i.height as usize],
                        pixels: pd.get_egui(),
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
                ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                    id: t.id(),
                    size,
                }));
            }
            if let Ok(mut i) = self.image.lock() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut i.hmirror, "H Mirror");
                    ui.checkbox(&mut i.vmirror, "V Mirror");
                });
            }
        });
        None
    }
}
