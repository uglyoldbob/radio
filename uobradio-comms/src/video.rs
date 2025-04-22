use std::sync::{Arc, Mutex};

use ffimage::iter::BytesExt;
use ffimage::iter::ColorConvertExt;
use ffimage::iter::PixelsExt;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ControlValue {
    None,
    Int64(i64),
    Bool(bool),
    String(String),
    VecU8(Vec<u8>),
    VecU16(Vec<u16>),
    VecU32(Vec<u32>),
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

#[cfg(target_os = "linux")]
pub struct ControlElement {
    pub id: u32,
    pub name: String,
    data: ControlData,
    pub value: Option<v4l::control::Value>,
}

#[cfg(target_os = "linux")]
impl ControlElement {
    pub fn sendable(&self) -> SendableControlElement {
        SendableControlElement {
            id: self.id,
            name: self.name.clone(),
            data: self.data.clone(),
            value: self.value.as_ref().map(|a| a.into()),
        }
    }

    pub fn new(
        d: &v4l::control::Description,
        value: Option<v4l::control::Value>,
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

#[cfg(target_os="linux")]
pub struct VideoSource {
    pub image: Arc<Mutex<VideoFrame>>,
    pub controls: Vec<ControlElement>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SendableVideoSource {
    pub image: VideoFrame,
    pub controls: Vec<SendableControlElement>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SendableControlElement {
    pub id: u32,
    pub name: String,
    data: ControlData,
    pub value: Option<ControlValue>,
}

#[cfg(target_os = "linux")]
impl SendableControlElement {
    pub fn egui_show(&mut self, ui: &mut egui::Ui) -> bool {
        ui.label(self.name.clone());
        let mut value: Option<v4l::control::Value> = self.value.as_ref().map(|a| a.into());
        match &mut self.data {
            ControlData::Integer {
                val: _,
                min,
                default: _,
                max,
            } => {
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(a) => Some(a),
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
                let mut r = false;
                if let Some(a) = a {
                    r = ui
                        .add(egui::Slider::new(a, *min..=*max).text(self.name.clone()))
                        .changed();
                }
                r
            }
            ControlData::Boolean { val: _, default: _ } => {
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(_) => None,
                        v4l::control::Value::Boolean(b) => Some(b),
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
                let mut r = false;
                if let Some(a) = a {
                    r = ui.checkbox(a, self.name.clone()).changed()
                }
                r
            }
            ControlData::String(_s) => {
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(_) => None,
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(s) => Some(s),
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
                let mut r = false;
                if let Some(a) = a {
                    r = ui.text_edit_singleline(a).changed()
                }
                r
            }
            ControlData::Bitmask(_m) => {
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(i) => Some(i),
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
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
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(a) => Some(a),
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
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
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(a) => Some(a),
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
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
                let a = value
                    .as_mut()
                    .map(|a| match a {
                        v4l::control::Value::None => None,
                        v4l::control::Value::Integer(a) => Some(a),
                        v4l::control::Value::Boolean(_) => None,
                        v4l::control::Value::String(_) => None,
                        v4l::control::Value::CompoundU8(_vec) => None,
                        v4l::control::Value::CompoundU16(_vec) => None,
                        v4l::control::Value::CompoundU32(_vec) => None,
                        v4l::control::Value::CompoundPtr(_vec) => None,
                    })
                    .flatten();
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

pub struct VideoSourceSendable {
    pub image: Arc<Mutex<VideoFrame>>,
    pub controls: Vec<SendableControlElement>,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct RgbPixel {
    a: [u8; 3],
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum PixelData {
    Yuyv(Vec<u8>),
    Rgb(Vec<u8>),
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
        }
    }

    fn get_rgb(&self) -> Vec<u8> {
        match self {
            PixelData::Yuyv(vec) => Self::yuyv_to_rgb(&vec),
            PixelData::Rgb(vec) => vec.clone(),
        }
    }

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
                    .map(|a| RgbPixel {
                        a: [a[0], a[1], a[2]],
                    })
                    .collect();
                Self::general_mirror(width, hflip, vflip, &mut pixels);
                *vec = pixels.iter().flat_map(|a| a.a).collect();
            }
        }
    }
}
