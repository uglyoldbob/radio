use std::sync::Arc;
use std::sync::Mutex;

use eframe::egui;
use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;
use uobradio_comms::v4l;
use uobradio_comms::video::ControlElement;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

pub enum VideoMessage {
    Quit,
    ControlData { id: u32, value: v4l::control::Value },
}

#[derive(Copy, Clone)]
#[repr(C)]
struct RgbPixel {
    a: [u8; 3],
}

#[derive(Clone)]
pub enum PixelData {
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
            PixelData::Egui(_vec) => todo!(),
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

    pub fn get_egui(&self) -> Vec<egui::Color32> {
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

pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
    pub pixel_data: Option<PixelData>,
    pub hmirror: bool,
    pub vmirror: bool,
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

    pub fn get_jpeg(&self) -> Vec<u8> {
        if let Some(pixels) = &self.pixel_data {
            let rgb = pixels.get_rgb();
            let mut thing = Vec::new();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut thing, 75);
            let _ = encoder.encode(
                &rgb,
                self.width as u32,
                self.height as u32,
                image::ExtendedColorType::Rgb8,
            );
            thing
        } else {
            Vec::new()
        }
    }

    fn mirroring(&mut self) {
        if let Some(pd) = &mut self.pixel_data {
            pd.mirroring(self.width, self.hmirror, self.vmirror);
        }
    }
}

pub struct VideoSource {
    pub image: Arc<Mutex<VideoFrame>>,
    pub vsend: std::sync::mpsc::Sender<VideoMessage>,
    pub controls: Vec<ControlElement>,
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
        let image = Arc::new(Mutex::new(VideoFrame::new()));
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
                    i.pixel_data = Some(PixelData::Yuyv(buf.to_vec()).to_rgb());
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
