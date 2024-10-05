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

#[derive(Debug)]
enum ControlData {
    Integer {
        val: i64,
        min: i64,
        default: i64,
        max: i64,
    },
    Boolean {
        val: bool,
        default: bool,
    },
    String(String),
    Bitmask(u64),
    U8 {
        val: u8,
        min: u8,
        default: u8,
        max: u8,
    },
    U16 {
        val: u16,
        min: u16,
        default: u16,
        max: u16,
    },
    U32 {
        val: u32,
        min: u32,
        default: u32,
        max: u32,
    },
}

#[derive(Debug)]
pub struct ControlElement {
    id: u32,
    name: String,
    data: ControlData,
}

impl ControlElement {
    fn new(d: &v4l::control::Description) -> Result<Self, String> {
        let cd = match d.typ {
            v4l::control::Type::Integer => Ok(ControlData::Integer {
                val: d.default,
                min: d.minimum,
                max: d.maximum,
                default: d.default,
            }),
            v4l::control::Type::Boolean => Ok(ControlData::Boolean {
                val: d.default != 0,
                default: d.default != 0,
            }),
            v4l::control::Type::Menu => Err(format!("Unsupported control Menu {}", d.name)),
            v4l::control::Type::Button => Err(format!("Unsupported control Button {}", d.name)),
            v4l::control::Type::Integer64 => Ok(ControlData::Integer {
                val: d.default,
                min: d.minimum,
                max: d.maximum,
                default: d.default,
            }),
            v4l::control::Type::CtrlClass => Err(format!("Unsupported control CtrlClass {}", d.name)),
            v4l::control::Type::String => Ok(ControlData::String("dummy".to_string())),
            v4l::control::Type::Bitmask => Ok(ControlData::Bitmask(d.default as u64)),
            v4l::control::Type::IntegerMenu => Err(format!("Unsupported control IntegerMenu {}", d.name)),
            v4l::control::Type::U8 => Ok(ControlData::U8 {
                val: d.default as u8,
                min: d.minimum as u8,
                max: d.maximum as u8,
                default: d.default as u8,
            }),
            v4l::control::Type::U16 => Ok(ControlData::U16 {
                val: d.default as u16,
                min: d.minimum as u16,
                max: d.maximum as u16,
                default: d.default as u16,
            }),
            v4l::control::Type::U32 => Ok(ControlData::U32 {
                val: d.default as u32,
                min: d.minimum as u32,
                max: d.maximum as u32,
                default: d.default as u32,
            }),
            v4l::control::Type::Area => Err(format!("Unsupported control Area {}", d.name)),
        };
        Ok(Self {
            id: d.id,
            name: d.name.clone(),
            data: cd?,
        })
    }

    pub fn egui_show(&mut self, ui: &mut egui::Ui) -> bool {
        ui.label(self.name.clone());
        match &mut self.data {
            ControlData::Integer { val, min, default, max } => {
                ui.add(egui::Slider::new(val, *min..=*max).text(self.name.clone())).changed()
            },
            ControlData::Boolean { val, default } => {
                ui.checkbox(val, self.name.clone()).changed()
            }
            ControlData::String(s) => {
                ui.text_edit_singleline(s).changed()
            }
            ControlData::Bitmask(m) => {
                ui.label(format!("{:X}", m)).changed()
            }
            ControlData::U8 { val, min, default, max } => {
                ui.add(egui::Slider::new(val, *min..=*max).text(self.name.clone())).changed()
            }
            ControlData::U16 { val, min, default, max } => {
                ui.add(egui::Slider::new(val, *min..=*max).text(self.name.clone())).changed()
            }
            ControlData::U32 { val, min, default, max } => {
                ui.add(egui::Slider::new(val, *min..=*max).text(self.name.clone())).changed()
            }
        }
    }
}

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

pub struct VideoSource {
    pub image: Arc<Mutex<VideoFrame>>,
    pub texture: Option<egui::TextureHandle>,
    pub vsend: std::sync::mpsc::Sender<bool>,
    pub controls: Vec<ControlElement>,
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.vsend.send(true).unwrap();
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
        let controls : Vec<ControlElement> = dev.query_controls().unwrap().iter().filter_map(|c| ControlElement::new(c).ok()).collect();
        std::thread::spawn(move || {
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
        VideoSource {image,
            texture: None,
            vsend: a,
            controls,
        }
    }


    pub fn new() -> Self {
        Self {
            which_video: 0,
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
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label("This is the video page");
                let mut size = ui.available_size();
                let vsrc = &mut common.video_sources[self.which_video];
                if ui.button("Debug controls").clicked() {
                    for c in &vsrc.controls {
                        println!("Control: {:?}", c);
                    }
                }
                if let Ok(i) = vsrc.image.lock() {
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
                        if let None = vsrc.texture {
                            vsrc.texture =
                                Some(ctx.load_texture("camera0", image, egui::TextureOptions::LINEAR));
                        } else if let Some(t) = &mut vsrc.texture {
                            t.set_partial([0, 0], image, egui::TextureOptions::LINEAR);
                        }
                    }
                }
                if let Some(t) = &vsrc.texture {
                    ui.add(egui::Image::from_texture(egui::load::SizedTexture {
                        id: t.id(),
                        size,
                    }));
                }
                if let Ok(mut i) = vsrc.image.lock() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut i.hmirror, "H Mirror");
                        ui.checkbox(&mut i.vmirror, "V Mirror");
                    });
                }
            });
        });
        None
    }
}
