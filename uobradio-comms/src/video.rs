//! Video handling code

use std::sync::{Arc, Mutex};

use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;


/// Represents a color pixel with rgb components
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct RgbPixel {
    colors: [u8; 3],
}

impl RgbPixel {
    /// Build from r g and b, making it fully non-transparent
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            colors: [r, g, b],
        }
    }
    /// Build from a solid gray channel
    pub const fn from_gray(g: u8) -> Self {
        Self {
            colors: [g, g, g],
        }
    }
}

/// A generic pixel based image
#[derive(Debug, Clone)]
pub struct PixelImage<T> {
    /// The actual pixels of the image
    pixels: Vec<T>,
    /// The width of the image in pixels.
    pub width: u16,
    /// The height of the image in pixels.
    pub height: u16,
}

impl PixelImage<RgbPixel> {
    /// Construct from raw image data of the specified dimensions
    pub fn from_raw(width: u16, height: u16, data: &[u8]) -> Self {
        let pixels: Vec<RgbPixel> = data.iter().map(|p| RgbPixel::from_gray(*p)).collect();
        Self {
            pixels,
            width,
            height,
        }
    }

    /// Build from jpeg using the image crate
    pub fn from_jpeg_image(data: &[u8]) -> Option<Self> {
        let data2 = std::io::Cursor::new(data);
        let reader = image::ImageReader::with_format(data2, image::ImageFormat::Jpeg);
        let image = reader.decode().ok()?;
        let img = image.into_rgb8();
        let w = img.width();
        let h = img.height();
        let b = img.as_raw().clone();
        let pixels: Vec<RgbPixel> = b
            .chunks_exact(3)
            .map(|p| RgbPixel::from_rgb(p[0], p[1], p[2]))
            .collect();
        Some(Self {
            pixels,
            width: w as u16,
            height: h as u16,
        })
    }

    /// Build a new image of the specified dimensions
    pub fn new(w: u16, h: u16) -> Self {
        let cap = w as usize * h as usize;
        let m = vec![RgbPixel { colors: [0; 3] }; cap];
        Self {
            pixels: m,
            width: w,
            height: h,
        }
    }
}

impl From<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>> for PixelImage<RgbPixel> {
    fn from(value: image::ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> Self {
        let p = value.pixels().map(|p| RgbPixel{ colors: p.0 }).collect();
        Self {
            pixels: p,
            width: value.width() as u16,
            height: value.height() as u16,
        }
    }
}

impl From<PixelImage<RgbPixel>> for egui::ColorImage {
    fn from(value: PixelImage<RgbPixel>) -> Self {
        let pixels = value
            .pixels
            .iter()
            .map(|p| egui::Color32::from_rgb(p.colors[0], p.colors[1], p.colors[2]))
            .collect();
        Self {
            size: [value.width as usize, value.height as usize],
            pixels,
        }
    }
}

/// A value for a video control
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ControlValue {
    /// Nothing
    None,
    /// An integer
    Int64(i64),
    /// A bool
    Bool(bool),
    /// A string
    String(String),
    /// The CompoundU8 type from v4l2
    VecU8(Vec<u8>),
    /// The CompoundU16 type from v4l2
    VecU16(Vec<u16>),
    /// The CompoundU32 type from v4l2
    VecU32(Vec<u32>),
    /// The Ptr type from v4l2
    Ptr(Vec<u8>),
}

#[cfg(target_os = "linux")]
impl From<ControlValue> for v4l::control::Value {
    fn from(value: ControlValue) -> Self {
        match value {
            ControlValue::None => Self::None,
            ControlValue::Int64(v) => Self::Integer(v),
            ControlValue::Bool(v) => Self::Boolean(v),
            ControlValue::String(v) => Self::String(v),
            ControlValue::VecU8(items) => Self::CompoundU8(items),
            ControlValue::VecU16(items) => Self::CompoundU16(items),
            ControlValue::VecU32(items) => Self::CompoundU32(items),
            ControlValue::Ptr(items) => Self::CompoundPtr(items),
        }
    }
}

#[cfg(target_os = "linux")]
impl From<&ControlValue> for v4l::control::Value {
    fn from(value: &ControlValue) -> Self {
        match value {
            ControlValue::None => Self::None,
            ControlValue::Int64(v) => Self::Integer(*v),
            ControlValue::Bool(v) => Self::Boolean(*v),
            ControlValue::String(v) => Self::String(v.clone()),
            ControlValue::VecU8(items) => Self::CompoundU8(items.clone()),
            ControlValue::VecU16(items) => Self::CompoundU16(items.clone()),
            ControlValue::VecU32(items) => Self::CompoundU32(items.clone()),
            ControlValue::Ptr(items) => Self::CompoundPtr(items.clone()),
        }
    }
}

#[cfg(target_os = "linux")]
impl From<&v4l::control::Value> for ControlValue {
    fn from(value: &v4l::control::Value) -> Self {
        match value {
            v4l::control::Value::None => Self::None,
            v4l::control::Value::Integer(v) => Self::Int64(*v),
            v4l::control::Value::Boolean(v) => Self::Bool(*v),
            v4l::control::Value::String(v) => Self::String(v.clone()),
            v4l::control::Value::CompoundU8(items) => Self::VecU8(items.clone()),
            v4l::control::Value::CompoundU16(items) => Self::VecU16(items.clone()),
            v4l::control::Value::CompoundU32(items) => Self::VecU32(items.clone()),
            v4l::control::Value::CompoundPtr(items) => Self::Ptr(items.clone()),
        }
    }
}

#[cfg(target_os = "linux")]
impl From<v4l::control::Value> for ControlValue {
    fn from(value: v4l::control::Value) -> Self {
        match value {
            v4l::control::Value::None => Self::None,
            v4l::control::Value::Integer(v) => Self::Int64(v),
            v4l::control::Value::Boolean(v) => Self::Bool(v),
            v4l::control::Value::String(v) => Self::String(v),
            v4l::control::Value::CompoundU8(items) => Self::VecU8(items),
            v4l::control::Value::CompoundU16(items) => Self::VecU16(items),
            v4l::control::Value::CompoundU32(items) => Self::VecU32(items),
            v4l::control::Value::CompoundPtr(items) => Self::Ptr(items),
        }
    }
}

/// Represents a single frame of video
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VideoFrame {
    /// The with of the frame in pixels
    pub width: u16,
    /// The height of the image in pixels
    pub height: u16,
    /// The optional pixel data for the frame
    pub pixel_data: Option<PixelData>,
    /// Should the image be hroizontally mirrored
    pub hmirror: bool,
    /// Should the image ve vertically mirrored
    pub vmirror: bool,
}

impl From<PixelImage<RgbPixel>> for VideoFrame {
    fn from(value: PixelImage<RgbPixel>) -> Self {
        let p: Vec<u8> = value.pixels.iter().flat_map(|p| [p.colors[0], p.colors[1], p.colors[2]]).collect();
        let pixels: PixelData = PixelData::Rgb(p);
        Self {
            width: value.width,
            height: value.height,
            pixel_data: Some(pixels),
            hmirror: false,
            vmirror: false,
        }
    }
}

impl VideoFrame {
    /// Construct an empty video frame
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixel_data: None,
            hmirror: false,
            vmirror: false,
        }
    }

    /// Build a jpeg with the video frame. Currently quality is hard-coded to 75 percent
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

    /// Run the horizontal and vertical mirroring for the image. Should probably only do this once per image.
    pub fn mirroring(&mut self) {
        if let Some(pd) = &mut self.pixel_data {
            pd.mirroring(self.width, self.hmirror, self.vmirror);
        }
    }
}


#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

/// A control element for a video source
#[cfg(target_os = "linux")]
pub struct ControlElement {
    /// The id of the control
    pub id: u32,
    /// The user-visible name of the control
    pub name: String,
    /// Specifies how the control can be manipulated
    data: ControlData,
    /// The value for the control element
    pub value: v4l::control::Value,
}

#[cfg(target_os = "linux")]
impl ControlElement {
    /// Build a sendable version of the control element
    pub fn sendable(&self) -> SendableControlElement {
        SendableControlElement {
            id: self.id,
            name: self.name.clone(),
            data: self.data.clone(),
            value: (&self.value).into(),
        }
    }

    /// Construct a new self, with the given description and value
    pub fn new(
        d: &v4l::control::Description,
        value: v4l::control::Value,
    ) -> Result<Self, String> {
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
            v4l::control::Type::CtrlClass => {
                Err(format!("Unsupported control CtrlClass {}", d.name))
            }
            v4l::control::Type::String => Ok(ControlData::String("dummy".to_string())),
            v4l::control::Type::Bitmask => Ok(ControlData::Bitmask(d.default as u64)),
            v4l::control::Type::IntegerMenu => {
                Err(format!("Unsupported control IntegerMenu {}", d.name))
            }
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
            value,
        })
    }
}

/// A plain video source
#[cfg(target_os="linux")]
pub struct VideoSource {
    /// The latext image for the video source
    pub image: Arc<Mutex<VideoFrame>>,
    /// The controls for the video source
    pub controls: Vec<ControlElement>,
}

#[cfg(target_os="linux")]
impl VideoSource {
    /// Get a sendable version of the video source
    pub fn sendable(&self) -> Option<SendableVideoSource> {
        let img = self.image.lock().ok()?;
        Some(SendableVideoSource { image: Some(img.clone()), controls: self.controls.iter().map(|a| a.into()).collect() })
    }
}

/// A video source that can be sent over a network or other channel
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SendableVideoSource {
    /// The newest image for the video source
    pub image: Option<VideoFrame>,
    /// The controls for the video source
    pub controls: Vec<SendableControlElement>,
}

impl SendableVideoSource {
    /// construct a new self
    pub fn new() -> Self {
        Self {
            image: None,
            controls: Vec::new(),
        }
    }
}

/// A control element that can be sent across the network or a channel
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SendableControlElement {
    /// The id for the control
    pub id: u32,
    /// The user visible name of the control
    pub name: String,
    /// The details for how the value can be set
    data: ControlData,
    /// The current value for the control
    pub value: ControlValue,
}

#[cfg(target_os="linux")]
impl From<&ControlElement> for SendableControlElement {
    fn from(value: &ControlElement) -> Self {
        Self {
            id: value.id,
            name: value.name.clone(),
            data: value.data.clone(),
            value: (&value.value).into(),
        }
    }
}

#[cfg(target_os = "linux")]
impl SendableControlElement {
    /// Show the control element on a egui form
    pub fn egui_show(&mut self, ui: &mut egui::Ui) -> bool {
        ui.label(self.name.clone());
        let value = &mut self.value;
        match &mut self.data {
            ControlData::Integer {
                val: _,
                min,
                default: _,
                max,
            } => {
                let a = if let ControlValue::Int64(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui
                        .add(egui::Slider::new(a, *min..=*max).text(self.name.clone()))
                        .changed();
                }
                r
            }
            ControlData::Boolean { val: _, default: _ } => {
                let a = if let ControlValue::Bool(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui.checkbox(a, self.name.clone()).changed()
                }
                r
            }
            ControlData::String(_s) => {
                let a = if let ControlValue::String(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui.text_edit_singleline(a).changed()
                }
                r
            }
            ControlData::Bitmask(_m) => {
                let a = if let ControlValue::Int64(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let r = false;
                if let Some(a) = a {
                    ui.label(format!("{:X}", a));
                }
                r
            }
            ControlData::U8 {
                val: _,
                min,
                default: _,
                max,
            } => {
                let a = if let ControlValue::Int64(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui
                        .add(
                            egui::Slider::new(a, (*min as i64)..=(*max as i64))
                                .text(self.name.clone()),
                        )
                        .changed()
                }
                r
            }
            ControlData::U16 {
                val: _,
                min,
                default: _,
                max,
            } => {
                let a = if let ControlValue::Int64(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui
                        .add(
                            egui::Slider::new(a, (*min as i64)..=(*max as i64))
                                .text(self.name.clone()),
                        )
                        .changed()
                }
                r
            }
            ControlData::U32 {
                val: _,
                min,
                default: _,
                max,
            } => {
                let a = if let ControlValue::Int64(i) = value {
                    Some(i)
                }
                else {
                    None
                };
                let mut r = false;
                if let Some(a) = a {
                    r = ui
                        .add(
                            egui::Slider::new(a, (*min as i64)..=(*max as i64))
                                .text(self.name.clone()),
                        )
                        .changed()
                }
                r
            }
        }
    }
}

/// An image in either yuyv or rgb format
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum PixelData {
    /// The image is represented with yuyv 4:2:2 data
    Yuyv(Vec<u8>),
    /// The image is represented with rgb data, 8 bits per channel, no alpha channel
    Rgb(Vec<u8>),
}

impl PixelData {
    /// Convert the given yuyv data to rgb
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

    /// Convert the given rgb data to a compatible egui format
    fn rgb_to_egui(vec: &[u8]) -> Vec<egui::Color32> {
        vec.chunks_exact(3)
            .map(|i| egui::Color32::from_rgb(i[0], i[1], i[2]))
            .collect()
    }

    /// Convert the pixel data to rgb, if required
    pub fn to_rgb(self) -> Self {
        match self {
            PixelData::Yuyv(vec) => PixelData::Rgb(Self::yuyv_to_rgb(&vec)),
            PixelData::Rgb(vec) => PixelData::Rgb(vec),
        }
    }

    fn get_rgb(&self) -> Vec<u8> {
        match self {
            PixelData::Yuyv(vec) => Self::yuyv_to_rgb(&vec),
            PixelData::Rgb(vec) => vec.clone(),
        }
    }

    /// Get an egui compatible image
    pub fn get_egui(&self) -> Vec<egui::Color32> {
        match self {
            PixelData::Yuyv(vec) => {
                let a = Self::yuyv_to_rgb(&vec);
                Self::rgb_to_egui(&a)
            }
            PixelData::Rgb(vec) => Self::rgb_to_egui(&vec),
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
                    .map(|a| RgbPixel::from_rgb(a[0], a[1], a[2]))
                    .collect();
                Self::general_mirror(width, hflip, vflip, &mut pixels);
                *vec = pixels.iter().flat_map(|a| a.colors).collect();
            }
        }
    }
}
