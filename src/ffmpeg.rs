//! Hardware-accelerated video decoder built on top of [`ffmpeg-next`].
//!
//! Two operating modes:
//!
//! **Container mode** — FFmpeg demuxes a file or URL, then decodes:
//! ```rust,no_run
//! use ffmpeg_hw_decoder::Decoder;
//! let mut dec = Decoder::open_auto("input.mp4").unwrap();
//! while let Some(frame) = dec.next_frame().unwrap() {
//!     println!("{}x{}", frame.width(), frame.height());
//! }
//! ```
//!
//! **NAL mode** — caller owns the transport, pushes raw NAL packets:
//! ```rust,no_run
//! use ffmpeg_hw_decoder::{NalDecoder, DecoderConfig};
//! use ffmpeg_next::codec::Id;
//!
//! let mut dec = NalDecoder::new(Id::H264, DecoderConfig::auto()).unwrap();
//!
//! // Feed Annex-B or AVCC NAL data from any source (camera, socket, …)
//! for nal in incoming_nals() {
//!     dec.push_nal(&nal.data, nal.pts, nal.dts).unwrap();
//!     while let Some(frame) = dec.next_frame().unwrap() {
//!         process(frame);
//!     }
//! }
//! // Flush at end-of-stream
//! dec.flush().unwrap();
//! while let Some(frame) = dec.next_frame().unwrap() {
//!     process(frame);
//! }
//! # fn incoming_nals() -> Vec<Nal> { vec![] }
//! # fn process(_: ffmpeg_hw_decoder::SoftwareFrame) {}
//! # struct Nal { data: Vec<u8>, pts: Option<i64>, dts: Option<i64> }
//! ```
//!
//! # Probe order
//!
//! | Priority | Back-end  | Typical hardware                        |
//! |----------|-----------|-----------------------------------------|
//! | 1        | v4l2m2m   | i.MX8MP VPU, RPi, Rockchip, Allwinner  |
//! | 2        | vaapi     | Intel iGPU, AMD, etnaviv on i.MX8       |
//! | 3        | cuda      | NVIDIA (NVDEC)                          |
//! | 4        | qsv       | Intel Quick Sync                        |
//! | 5        | drm       | Generic DRM prime                       |
//! | 6        | vulkan    | FFmpeg >= 6.0 Vulkan decode             |
//! | 7        | vdpau     | Legacy NVIDIA / Nouveau                 |
//! | 8        | software  | libavcodec CPU fallback                 |
//!
//! # i.MX8MP VPU device layout
//!
//! | Codec     | Default device |
//! |-----------|----------------|
//! | H.264     | /dev/video0    |
//! | HEVC      | /dev/video1    |
//! | VP8       | /dev/video2    |
//! | VP9       | /dev/video3    |
//! | MPEG-2/4  | /dev/video4    |

// ============================================================================
// Imports
// ============================================================================

use std::ffi::{c_int, CString};

use clap::ValueEnum;
use ffmpeg_next::{
    codec::codec,
    ffi,
    format::{self, Pixel},
    frame::Video as VideoFrame,
    media::Type as MediaType,
    software::scaling::{context::Context as SwsContext, flag::Flags},
    Packet,
};
use thiserror::Error;

// ============================================================================
// Error
// ============================================================================

#[derive(Debug, Error)]
pub enum DecoderError {
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),

    #[error("Hardware acceleration unavailable for device type: {0}")]
    HwAccelUnavailable(String),

    #[error("Failed to create hardware device context: {0}")]
    HwDeviceCreate(String),

    #[error("Failed to set hardware frames context")]
    HwFramesContext,

    #[error("No video stream found in input")]
    NoVideoStream,

    #[error("Codec not found: {0}")]
    CodecNotFound(String),

    #[error("Failed to transfer hardware frame to system memory")]
    FrameTransfer,

    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedPixelFormat(Pixel),

    #[error("End of stream")]
    Eof,
}

pub type Result<T> = std::result::Result<T, DecoderError>;

// ============================================================================
// Hardware acceleration – device types
// ============================================================================

/// Supported hardware acceleration back-ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwDeviceType {
    /// V4L2 memory-to-memory — covers i.MX8MP VPU, RPi, Rockchip, Allwinner
    V4l2M2m,
    /// VA-API (Intel iGPU, AMD, etnaviv/imx-gpu-viv on i.MX8)
    Vaapi,
    /// NVIDIA NVDEC via CUDA
    Cuda,
    /// Intel Quick Sync Video
    Qsv,
    /// DRM PRIME zero-copy buffers
    Drm,
    /// Vulkan video decode (FFmpeg >= 6.0)
    Vulkan,
    /// VDPAU (legacy NVIDIA / Nouveau)
    Vdpau,
}

impl HwDeviceType {
    pub fn as_av_hw_device_type(self) -> ffi::AVHWDeviceType {
        match self {
            // V4L2M2M = 13, defined in FFmpeg >= 4.0
            // May be missing from bindgen output if FFmpeg was built without --enable-v4l2-m2m
            HwDeviceType::V4l2M2m => unsafe {
                std::mem::transmute::<u32, ffi::AVHWDeviceType>(13u32)
            },
            HwDeviceType::Vaapi => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            HwDeviceType::Cuda => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
            HwDeviceType::Qsv => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV,
            HwDeviceType::Drm => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
            HwDeviceType::Vulkan => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN,
            HwDeviceType::Vdpau => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VDPAU,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            HwDeviceType::V4l2M2m => "v4l2m2m",
            HwDeviceType::Vaapi => "vaapi",
            HwDeviceType::Cuda => "cuda",
            HwDeviceType::Qsv => "qsv",
            HwDeviceType::Drm => "drm",
            HwDeviceType::Vulkan => "vulkan",
            HwDeviceType::Vdpau => "vdpau",
        }
    }

    pub fn default_device(self) -> Option<&'static str> {
        match self {
            HwDeviceType::V4l2M2m => Some("/dev/video0"),
            HwDeviceType::Vaapi => Some("/dev/dri/renderD128"),
            HwDeviceType::Drm => Some("/dev/dri/card0"),
            _ => None,
        }
    }

    pub fn probe_order() -> &'static [HwDeviceType] {
        &[
            HwDeviceType::V4l2M2m,
            HwDeviceType::Vaapi,
            HwDeviceType::Cuda,
            HwDeviceType::Qsv,
            HwDeviceType::Drm,
            HwDeviceType::Vulkan,
            HwDeviceType::Vdpau,
        ]
    }
}

// ============================================================================
// HwBackend — user-facing selector (CLI / public API)
// ============================================================================

/// User-facing hardware back-end selector.
///
/// Convert to a [`DecoderConfig`] with [`HwBackend::to_config`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HwBackend {
    /// Probe all back-ends automatically (recommended)
    Auto,
    /// V4L2 M2M (i.MX8MP VPU, RPi, Rockchip, Allwinner)
    V4l2m2m,
    /// VA-API (Intel iGPU, AMD, etnaviv)
    Vaapi,
    /// NVIDIA NVDEC via CUDA
    Cuda,
    /// Intel Quick Sync Video
    Qsv,
    /// DRM PRIME zero-copy
    Drm,
    /// Vulkan video decode (FFmpeg >= 6.0)
    Vulkan,
    /// VDPAU (legacy NVIDIA / Nouveau)
    Vdpau,
    /// Disable hardware acceleration, use software decoding only
    Software,
}

impl HwBackend {
    pub fn to_hw_device_type(self) -> Option<HwDeviceType> {
        match self {
            HwBackend::Auto | HwBackend::Software => None,
            HwBackend::V4l2m2m => Some(HwDeviceType::V4l2M2m),
            HwBackend::Vaapi => Some(HwDeviceType::Vaapi),
            HwBackend::Cuda => Some(HwDeviceType::Cuda),
            HwBackend::Qsv => Some(HwDeviceType::Qsv),
            HwBackend::Drm => Some(HwDeviceType::Drm),
            HwBackend::Vulkan => Some(HwDeviceType::Vulkan),
            HwBackend::Vdpau => Some(HwDeviceType::Vdpau),
        }
    }

    pub fn is_software(self) -> bool {
        self == HwBackend::Software
    }

    pub fn to_config(
        self,
        device_path: Option<String>,
        no_fallback: bool,
        low_latency: bool,
    ) -> DecoderConfig {
        DecoderConfig {
            hw_device_type: self.to_hw_device_type(),
            device_path,
            fallback_to_software: !no_fallback && !self.is_software(),
            low_latency,
            ..DecoderConfig::auto()
        }
    }
}

// ============================================================================
// i.MX8MP codec → V4L2 device mapping
// ============================================================================

/// V4L2 M2M device node for `codec_id` on an i.MX8MP.
///
/// Returns `None` for codecs not accelerated by the VPU.
pub fn imx8mp_v4l2_device(codec_id: ffmpeg_next::codec::Id) -> Option<&'static str> {
    match codec_id {
        ffmpeg_next::codec::Id::H264 => Some("/dev/video0"),
        ffmpeg_next::codec::Id::HEVC => Some("/dev/video1"),
        ffmpeg_next::codec::Id::VP8 => Some("/dev/video2"),
        ffmpeg_next::codec::Id::VP9 => Some("/dev/video3"),
        ffmpeg_next::codec::Id::MPEG2VIDEO | ffmpeg_next::codec::Id::MPEG4 => Some("/dev/video4"),
        _ => None,
    }
}

// ============================================================================
// Hardware device context
// ============================================================================

/// Owned FFmpeg hardware device context (`AVBufferRef *`).
pub struct HwDeviceContext {
    ptr: *mut ffi::AVBufferRef,
    pub device_type: HwDeviceType,
}

unsafe impl Send for HwDeviceContext {}
unsafe impl Sync for HwDeviceContext {}

impl HwDeviceContext {
    pub fn new(device_type: HwDeviceType, device: Option<&str>) -> Result<Self> {
        let device_path = device
            .or_else(|| device_type.default_device())
            .unwrap_or("");

        let c_device = if device_path.is_empty() {
            None
        } else {
            Some(
                CString::new(device_path)
                    .map_err(|_| DecoderError::HwDeviceCreate(device_path.to_owned()))?,
            )
        };

        let mut ptr: *mut ffi::AVBufferRef = std::ptr::null_mut();
        let ret = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut ptr,
                device_type.as_av_hw_device_type(),
                c_device
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(std::ptr::null()),
                std::ptr::null_mut(),
                0,
            )
        };

        if ret < 0 || ptr.is_null() {
            return Err(DecoderError::HwDeviceCreate(format!(
                "{} (device={:?}, err={})",
                device_type.name(),
                device,
                ret
            )));
        }

        log::debug!("hw device: {} @ {:?}", device_type.name(), device);
        Ok(Self { ptr, device_type })
    }

    fn ref_ptr(&self) -> *mut ffi::AVBufferRef {
        unsafe { ffi::av_buffer_ref(self.ptr) }
    }
}

impl Drop for HwDeviceContext {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { ffi::av_buffer_unref(&mut self.ptr) };
        }
    }
}

/// Pixel formats `codec` can produce for `device_type` via hw device ctx.
pub fn hw_pixel_formats_for_codec(codec: &codec::Codec, device_type: HwDeviceType) -> Vec<Pixel> {
    let mut out = Vec::new();
    let av_type = device_type.as_av_hw_device_type();
    let mut i = 0i32;
    loop {
        let cfg = unsafe { ffi::avcodec_get_hw_config(codec.as_ptr(), i) };
        if cfg.is_null() {
            break;
        }
        let cfg = unsafe { &*cfg };
        if cfg.device_type == av_type
            && (cfg.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32) != 0
        {
            let pix = Pixel::from(cfg.pix_fmt);
            if pix != Pixel::None {
                out.push(pix);
            }
        }
        i += 1;
    }
    out
}

// ============================================================================
// SoftwareFrame
// ============================================================================

/// A decoded video frame guaranteed to be in CPU-accessible memory.
pub struct SoftwareFrame {
    inner: VideoFrame,
}

impl SoftwareFrame {
    /// Download from hw surface or wrap an already-software frame.
    pub fn from_hw_frame(hw_frame: &VideoFrame) -> Result<Self> {
        if is_hardware_pixel_format(hw_frame.format()) {
            let mut sw = VideoFrame::empty();
            let ret =
                unsafe { ffi::av_hwframe_transfer_data(sw.as_mut_ptr(), hw_frame.as_ptr(), 0) };
            if ret < 0 {
                return Err(DecoderError::FrameTransfer);
            }
            unsafe {
                ffi::av_frame_copy_props(sw.as_mut_ptr(), hw_frame.as_ptr());
            }
            log::trace!(
                "hw transfer {:?}->{:?} {}x{}",
                hw_frame.format(),
                sw.format(),
                sw.width(),
                sw.height()
            );
            Ok(Self { inner: sw })
        } else {
            let mut sw = VideoFrame::empty();
            if unsafe { ffi::av_frame_ref(sw.as_mut_ptr(), hw_frame.as_ptr()) } < 0 {
                return Err(DecoderError::FrameTransfer);
            }
            Ok(Self { inner: sw })
        }
    }

    pub fn width(&self) -> u32 {
        self.inner.width()
    }
    pub fn height(&self) -> u32 {
        self.inner.height()
    }
    pub fn format(&self) -> Pixel {
        self.inner.format()
    }
    pub fn pts(&self) -> Option<i64> {
        self.inner.pts()
    }
    pub fn as_video_frame(&self) -> &VideoFrame {
        &self.inner
    }
    pub fn plane_data(&self, i: usize) -> &[u8] {
        self.inner.data(i)
    }
    pub fn linesize(&self, i: usize) -> usize {
        self.inner.stride(i)
    }

    /// Convert to packed RGB24 via libswscale.
    pub fn to_rgb24(&self) -> Result<VideoFrame> {
        let src = self.inner.format();
        let w = self.inner.width();
        let h = self.inner.height();
        let mut sws = SwsContext::get(src, w, h, Pixel::RGB24, w, h, Flags::BILINEAR)
            .map_err(|_| DecoderError::UnsupportedPixelFormat(src))?;
        let mut dst = VideoFrame::new(Pixel::RGB24, w, h);
        sws.run(&self.inner, &mut dst)
            .map_err(|_| DecoderError::UnsupportedPixelFormat(src))?;
        Ok(dst)
    }
}

/// `true` if `pixel` has `AV_PIX_FMT_FLAG_HWACCEL`.
pub fn is_hardware_pixel_format(pixel: Pixel) -> bool {
    let desc = unsafe { ffi::av_pix_fmt_desc_get(pixel.into()) };
    if desc.is_null() {
        return false;
    }
    (unsafe { (*desc).flags } & ffi::AV_PIX_FMT_FLAG_HWACCEL as u64) != 0
}

// ============================================================================
// DecoderConfig
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct DecoderConfig {
    /// Specific back-end to try first; `None` = auto-probe.
    pub hw_device_type: Option<HwDeviceType>,
    /// Override device node (e.g. `/dev/video2`).
    pub device_path: Option<String>,
    /// Fall back to software if all hw attempts fail. Default: `true`.
    pub fallback_to_software: bool,
    /// Codec thread count for software decoding (0 = FFmpeg decides).
    pub thread_count: u32,
    /// Disable b-frame reordering for low-latency / live pipelines.
    pub low_latency: bool,
}

impl DecoderConfig {
    pub fn auto() -> Self {
        Self {
            fallback_to_software: true,
            ..Default::default()
        }
    }
}

// ============================================================================
// get_format callback (thread-local state)
// ============================================================================

mod get_format_state {
    use ffmpeg_next::{ffi, format::Pixel};
    use std::cell::Cell;
    thread_local! {
        static DESIRED: Cell<ffi::AVPixelFormat> =
            Cell::new(ffi::AVPixelFormat::AV_PIX_FMT_NONE);
    }
    pub fn set(fmt: Pixel) {
        DESIRED.with(|c| c.set(fmt.into()));
    }
    pub fn get() -> ffi::AVPixelFormat {
        DESIRED.with(|c| c.get())
    }
}

extern "C" fn get_format(
    _ctx: *mut ffi::AVCodecContext,
    fmt_list: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let desired = get_format_state::get();
    let mut i = 0;
    loop {
        let fmt = unsafe { *fmt_list.add(i) };
        if fmt == ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            break;
        }
        if fmt == desired {
            return fmt;
        }
        i += 1;
    }
    unsafe { *fmt_list }
}

// ============================================================================
// Shared hw-probe logic (used by both Decoder and NalDecoder)
// ============================================================================

fn probe_hw(
    codec: &codec::Codec,
    codec_id: ffmpeg_next::codec::Id,
    codec_ctx: &mut ffmpeg_next::codec::context::Context,
    config: &DecoderConfig,
) -> (Option<HwDeviceContext>, Option<Pixel>) {
    let candidates: Vec<(HwDeviceType, Option<String>)> = if let Some(dt) = config.hw_device_type {
        let node = config
            .device_path
            .clone()
            .or_else(|| codec_device_node(dt, codec_id).map(str::to_owned));
        vec![(dt, node)]
    } else {
        HwDeviceType::probe_order()
            .iter()
            .map(|&dt| {
                let node = config
                    .device_path
                    .clone()
                    .or_else(|| codec_device_node(dt, codec_id).map(str::to_owned));
                (dt, node)
            })
            .collect()
    };

    for (dt, node) in candidates {
        let node_str = node.as_deref();

        let hw_ctx = match HwDeviceContext::new(dt, node_str) {
            Ok(ctx) => ctx,
            Err(e) => {
                log::debug!("skip {} {:?}: {}", dt.name(), node_str, e);
                continue;
            }
        };

        let hw_fmts = hw_pixel_formats_for_codec(codec, dt);
        let hw_fmt = match hw_fmts.first().copied() {
            Some(f) => f,
            None => {
                log::debug!("skip {}: no hw configs for {:?}", dt.name(), codec_id);
                continue;
            }
        };

        log::info!("hw: {} @ {:?}  fmt={:?}", dt.name(), node_str, hw_fmt);
        get_format_state::set(hw_fmt);
        unsafe {
            let p = codec_ctx.as_mut_ptr();
            (*p).hw_device_ctx = hw_ctx.ref_ptr();
            (*p).get_format = Some(get_format);
        }
        return (Some(hw_ctx), Some(hw_fmt));
    }

    if config.fallback_to_software {
        log::info!("no hw back-end available, using software decoding");
    }
    (None, None)
}

/// Best device node for `(device_type, codec_id)`, using i.MX8MP layout for V4L2 M2M.
fn codec_device_node(dt: HwDeviceType, codec_id: ffmpeg_next::codec::Id) -> Option<&'static str> {
    match dt {
        HwDeviceType::V4l2M2m => imx8mp_v4l2_device(codec_id).or_else(|| dt.default_device()),
        _ => dt.default_device(),
    }
}

/// Apply thread count and low-latency flags to a raw `AVCodecContext`.
fn apply_codec_flags(codec_ctx: &mut ffmpeg_next::codec::context::Context, config: &DecoderConfig) {
    if config.thread_count > 0 {
        unsafe {
            (*codec_ctx.as_mut_ptr()).thread_count = config.thread_count as c_int;
        }
    }
    if config.low_latency {
        unsafe {
            (*codec_ctx.as_mut_ptr()).flags |= ffi::AV_CODEC_FLAG_LOW_DELAY as c_int;
            (*codec_ctx.as_mut_ptr()).flags2 |= ffi::AV_CODEC_FLAG2_FAST as c_int;
        }
    }
}

/// Drain all available frames from the codec into `out`.
fn drain_frames(
    decoder: &mut ffmpeg_next::codec::decoder::Video,
    out: &mut Vec<SoftwareFrame>,
) -> Result<()> {
    loop {
        let mut hw_frame = VideoFrame::empty();
        match decoder.receive_frame(&mut hw_frame) {
            Ok(()) => out.push(SoftwareFrame::from_hw_frame(&hw_frame)?),
            Err(ffmpeg_next::Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                break
            }
            Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(DecoderError::Ffmpeg(e)),
        }
    }
    Ok(())
}

// ============================================================================
// Decoder — container / URL mode
// ============================================================================

/// Hardware-accelerated decoder that demuxes a container (file, RTSP, HLS, …).
///
/// Use [`NalDecoder`] instead when you own the transport and push raw NAL data.
pub struct Decoder {
    input: format::context::Input,
    video_stream_index: usize,
    decoder: ffmpeg_next::codec::decoder::Video,
    _hw_ctx: Option<HwDeviceContext>,
    pub is_hardware: bool,
    hw_pixel_format: Option<Pixel>,
}

impl Decoder {
    /// Open with fully automatic hardware detection.
    pub fn open_auto(path: &str) -> Result<Self> {
        Self::open(path, DecoderConfig::auto())
    }

    /// Open with explicit config.
    pub fn open(path: &str, config: DecoderConfig) -> Result<Self> {
        ffmpeg_next::init().map_err(DecoderError::Ffmpeg)?;

        let input = format::input(&path).map_err(DecoderError::Ffmpeg)?;

        let stream = input
            .streams()
            .best(MediaType::Video)
            .ok_or(DecoderError::NoVideoStream)?;
        let video_stream_index = stream.index();

        let codec_params = stream.parameters();
        let codec_id = codec_params.id();

        let codec = ffmpeg_next::codec::decoder::find(codec_id)
            .ok_or_else(|| DecoderError::CodecNotFound(format!("{:?}", codec_id)))?;

        log::info!("container codec: {:?}", codec_id);

        let mut codec_ctx = ffmpeg_next::codec::context::Context::from_parameters(codec_params)
            .map_err(DecoderError::Ffmpeg)?;

        apply_codec_flags(&mut codec_ctx, &config);

        let (hw_ctx, hw_pixel_format) = probe_hw(&codec, codec_id, &mut codec_ctx, &config);
        let is_hardware = hw_ctx.is_some();
        let decoder = codec_ctx.decoder().video().map_err(DecoderError::Ffmpeg)?;

        Ok(Self {
            input,
            video_stream_index,
            decoder,
            _hw_ctx: hw_ctx,
            is_hardware,
            hw_pixel_format,
        })
    }

    /// Decode the next frame from the container.  Returns `Ok(None)` at EOS.
    pub fn next_frame(&mut self) -> Result<Option<SoftwareFrame>> {
        loop {
            let mut hw = VideoFrame::empty();
            match self.decoder.receive_frame(&mut hw) {
                Ok(()) => return Ok(Some(SoftwareFrame::from_hw_frame(&hw)?)),
                Err(ffmpeg_next::Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                }
                Err(ffmpeg_next::Error::Eof) => return Ok(None),
                Err(e) => return Err(DecoderError::Ffmpeg(e)),
            }

            // Need more data — pull the next video packet from the demuxer.
            let mut sent = false;
            for (stream, packet) in self.input.packets() {
                if stream.index() == self.video_stream_index {
                    self.decoder
                        .send_packet(&packet)
                        .map_err(DecoderError::Ffmpeg)?;
                    sent = true;
                    break;
                }
            }
            if !sent {
                // Demuxer exhausted — flush remaining frames.
                self.decoder.send_eof().map_err(DecoderError::Ffmpeg)?;
                let mut hw = VideoFrame::empty();
                return match self.decoder.receive_frame(&mut hw) {
                    Ok(()) => Ok(Some(SoftwareFrame::from_hw_frame(&hw)?)),
                    Err(ffmpeg_next::Error::Other { errno })
                        if errno == ffmpeg_next::error::EAGAIN =>
                    {
                        Ok(None)
                    }
                    Err(ffmpeg_next::Error::Eof) => Ok(None),
                    Err(e) => Err(DecoderError::Ffmpeg(e)),
                };
            }
        }
    }

    /// Iterator over all decoded frames.
    pub fn frames(&mut self) -> FrameIter<'_> {
        FrameIter { decoder: self }
    }

    pub fn width(&self) -> u32 {
        self.decoder.width()
    }
    pub fn height(&self) -> u32 {
        self.decoder.height()
    }
    pub fn format(&self) -> Pixel {
        self.decoder.format()
    }
    pub fn hw_pixel_format(&self) -> Option<Pixel> {
        self.hw_pixel_format
    }
}

// ============================================================================
// NalDecoder — raw NAL packet mode
// ============================================================================

/// Hardware-accelerated decoder for raw NAL packet streams.
///
/// The caller is responsible for transport and framing.  Each call to
/// [`push_nal`](NalDecoder::push_nal) sends one access unit (one or more NAL
/// units that together form a single picture) directly into the codec,
/// bypassing FFmpeg's demuxer entirely.
///
/// # Annex-B vs AVCC
///
/// - **Annex-B** (start codes `00 00 00 01` or `00 00 01`): pass the raw bytes
///   directly; FFmpeg's H.264/HEVC parsers handle them natively.
/// - **AVCC / length-prefixed**: either convert to Annex-B first, or set
///   `extradata` (SPS/PPS in AVCC format) via [`NalDecoder::new_with_extradata`]
///   so FFmpeg can interpret length prefixes correctly.
///
/// # Example
///
/// ```rust,no_run
/// use ffmpeg_hw_decoder::{NalDecoder, DecoderConfig};
/// use ffmpeg_next::codec::Id;
///
/// let mut dec = NalDecoder::new(Id::H264, DecoderConfig::auto()).unwrap();
///
/// let nal_data: &[u8] = &[0x00, 0x00, 0x00, 0x01, /* … */];
/// dec.push_nal(nal_data, Some(0), None).unwrap();
///
/// while let Some(frame) = dec.next_frame().unwrap() {
///     println!("{}x{} pts={:?}", frame.width(), frame.height(), frame.pts());
/// }
/// ```
pub struct NalDecoder {
    decoder: ffmpeg_next::codec::decoder::Video,
    _hw_ctx: Option<HwDeviceContext>,
    pub is_hardware: bool,
    hw_pixel_format: Option<Pixel>,
    /// Buffered decoded frames waiting to be returned by `next_frame`.
    pending: Vec<SoftwareFrame>,
    flushed: bool,
}

impl NalDecoder {
    /// Create a `NalDecoder` for `codec_id` with automatic hardware detection.
    pub fn new(codec_id: ffmpeg_next::codec::Id, config: DecoderConfig) -> Result<Self> {
        Self::new_with_extradata(codec_id, &[], config)
    }

    /// Create a `NalDecoder` and supply codec `extradata` (e.g. SPS/PPS in
    /// AVCC/HEVC `DecoderConfigurationRecord` format, or a codec private blob).
    ///
    /// Pass an empty slice when using Annex-B streams — FFmpeg will parse
    /// parameter sets from the stream itself.
    pub fn new_with_extradata(
        codec_id: ffmpeg_next::codec::Id,
        extradata: &[u8],
        config: DecoderConfig,
    ) -> Result<Self> {
        ffmpeg_next::init().map_err(DecoderError::Ffmpeg)?;

        let codec = ffmpeg_next::codec::decoder::find(codec_id)
            .ok_or_else(|| DecoderError::CodecNotFound(format!("{:?}", codec_id)))?;

        log::info!("NalDecoder codec: {:?}", codec_id);

        // Build a codec context without container parameters.
        let mut codec_ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);

        // Inject extradata before opening the codec, if provided.
        if !extradata.is_empty() {
            unsafe {
                let ctx = codec_ctx.as_mut_ptr();
                let buf = ffi::av_mallocz(
                    (extradata.len() + ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize) as _,
                ) as *mut u8;
                if buf.is_null() {
                    return Err(DecoderError::Ffmpeg(ffmpeg_next::Error::from(
                        -12, /* ENOMEM */
                    )));
                }
                std::ptr::copy_nonoverlapping(extradata.as_ptr(), buf, extradata.len());
                (*ctx).extradata = buf;
                (*ctx).extradata_size = extradata.len() as c_int;
            }
        }

        apply_codec_flags(&mut codec_ctx, &config);

        let (hw_ctx, hw_pixel_format) = probe_hw(&codec, codec_id, &mut codec_ctx, &config);
        let is_hardware = hw_ctx.is_some();

        let decoder = codec_ctx.decoder().video().map_err(DecoderError::Ffmpeg)?;

        Ok(Self {
            decoder,
            _hw_ctx: hw_ctx,
            is_hardware,
            hw_pixel_format,
            pending: Vec::new(),
            flushed: false,
        })
    }

    /// Push one access unit of raw NAL data into the decoder.
    ///
    /// `pts` and `dts` are in the caller's time base (nanoseconds, stream
    /// ticks, or `None` to let FFmpeg infer them).
    ///
    /// After calling `push_nal`, drain decoded frames by calling
    /// [`next_frame`](Self::next_frame) in a loop until it returns `Ok(None)`.
    pub fn push_nal(&mut self, data: &[u8], pts: Option<i64>, dts: Option<i64>) -> Result<()> {
        if self.flushed {
            return Err(DecoderError::Eof);
        }

        // Build an AVPacket that borrows `data` (no copy — FFmpeg ref-counts it).
        let mut packet = Packet::copy(data);

        if let Some(pts) = pts {
            packet.set_pts(Some(pts));
        }
        if let Some(dts) = dts {
            packet.set_dts(Some(dts));
        }

        self.decoder
            .send_packet(&packet)
            .map_err(DecoderError::Ffmpeg)?;

        // Eagerly drain whatever the codec has ready.
        drain_frames(&mut self.decoder, &mut self.pending)
    }

    /// Flush the decoder after the last NAL has been pushed.
    ///
    /// This signals end-of-stream to the codec so buffered frames are
    /// released.  Call [`next_frame`](Self::next_frame) after flushing to
    /// collect those final frames.
    pub fn flush(&mut self) -> Result<()> {
        if self.flushed {
            return Ok(());
        }
        self.flushed = true;
        self.decoder.send_eof().map_err(DecoderError::Ffmpeg)?;
        drain_frames(&mut self.decoder, &mut self.pending)
    }

    /// Return the next decoded frame, or `Ok(None)` when none is buffered.
    ///
    /// Call this in a loop after each [`push_nal`](Self::push_nal) and after
    /// [`flush`](Self::flush).
    pub fn next_frame(&mut self) -> Result<Option<SoftwareFrame>> {
        if !self.pending.is_empty() {
            return Ok(Some(self.pending.remove(0)));
        }
        // Try the codec once more in case pending was empty but more is ready.
        drain_frames(&mut self.decoder, &mut self.pending)?;
        Ok(if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        })
    }

    pub fn width(&self) -> u32 {
        self.decoder.width()
    }
    pub fn height(&self) -> u32 {
        self.decoder.height()
    }
    pub fn format(&self) -> Pixel {
        self.decoder.format()
    }
    pub fn hw_pixel_format(&self) -> Option<Pixel> {
        self.hw_pixel_format
    }
}

// ============================================================================
// FrameIter (for Decoder / container mode)
// ============================================================================

/// Iterator returned by [`Decoder::frames`].
pub struct FrameIter<'a> {
    decoder: &'a mut Decoder,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Result<SoftwareFrame>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.decoder.next_frame() {
            Ok(Some(f)) => Some(Ok(f)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
