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
//! dec.push_nal(&nal_data, Some(pts), None).unwrap();
//! while let Some(frame) = dec.next_frame().unwrap() { process(frame); }
//! dec.flush().unwrap();
//! while let Some(frame) = dec.next_frame().unwrap() { process(frame); }
//! # fn process(_: ffmpeg_hw_decoder::SoftwareFrame) {}
//! # let (nal_data, pts) = (vec![0u8], 0i64);
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
    codec, ffi,
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

// FFmpeg 4.4.1 on i.MX8MP: VPU is accessed via named codec, not hw device type.
// Use "h264_v4l2m2m" instead of the generic h264 decoder + hw device context.
pub fn v4l2m2m_codec_name(codec_id: codec::Id) -> Option<&'static str> {
    match codec_id {
        codec::Id::H264 => Some("h264_v4l2m2m"),
        codec::Id::HEVC => Some("hevc_v4l2m2m"),
        codec::Id::VP8 => Some("vp8_v4l2m2m"),
        codec::Id::VP9 => Some("vp9_v4l2m2m"),
        codec::Id::MPEG4 => Some("mpeg4_v4l2m2m"),
        codec::Id::MPEG2VIDEO => Some("mpeg2_v4l2m2m"),
        _ => None,
    }
}

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
            // AV_HWDEVICE_TYPE_V4L2M2M = 13.  Some bindgen outputs omit it when
            // FFmpeg was built without --enable-v4l2-m2m; transmute is safe
            // because AVHWDeviceType is #[repr(u32)] and the value is stable.
            HwDeviceType::V4l2M2m => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE,
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

pub fn imx8mp_v4l2_device(codec_id: codec::Id) -> Option<&'static str> {
    match codec_id {
        codec::Id::H264 => Some("/dev/video0"),
        codec::Id::HEVC => Some("/dev/video1"),
        codec::Id::VP8 => Some("/dev/video2"),
        codec::Id::VP9 => Some("/dev/video3"),
        codec::Id::MPEG2VIDEO | codec::Id::MPEG4 => Some("/dev/video4"),
        _ => None,
    }
}

// ============================================================================
// Hardware device context
// ============================================================================

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

        log::info!("hw device: {} @ {:?}", device_type.name(), device);
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

pub fn hw_pixel_formats_for_codec(
    codec: &ffmpeg_next::codec::codec::Codec,
    device_type: HwDeviceType,
) -> Vec<Pixel> {
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

pub struct SoftwareFrame {
    inner: VideoFrame,
}

impl SoftwareFrame {
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
    pub hw_device_type: Option<HwDeviceType>,
    pub device_path: Option<String>,
    pub fallback_to_software: bool,
    pub thread_count: u32,
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
// get_format callback
//
// Bug fix vs previous version: we store the desired pixel format in the
// AVCodecContext's `opaque` pointer rather than thread-local storage.
// TLS breaks when two NalDecoder instances are opened on different threads
// because they share the same Cell and clobber each other's desired format.
// Using opaque ties the state to the specific codec context instance.
// ============================================================================

/// Per-context state stored in `AVCodecContext::opaque`.
struct GetFormatState {
    desired: ffi::AVPixelFormat,
}

extern "C" fn get_format(
    ctx: *mut ffi::AVCodecContext,
    fmt_list: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    // Read the desired format from the opaque pointer we set before open.
    let desired = unsafe {
        let state = (*ctx).opaque as *const GetFormatState;
        if state.is_null() {
            ffi::AVPixelFormat::AV_PIX_FMT_NONE
        } else {
            (*state).desired
        }
    };

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
    // Desired hw format not offered — fall back to first (software) format.
    unsafe { *fmt_list }
}

// ============================================================================
// Shared hw-probe logic
//
// Returns (hw_ctx, hw_pixel_format, codec_already_opened).
//
// When hw succeeds, avcodec_open2 is called INSIDE probe_hw under a silenced
// log level to suppress "Invalid/Failed setup for format X" noise — those
// messages are emitted at AV_LOG_ERROR for every non-matching format during
// get_format negotiation and are not real errors.
//
// When codec_already_opened == false, the caller must still open the codec
// (software fallback path via codec_ctx.decoder().video()).
// ============================================================================

fn probe_hw(
    codec: &ffmpeg_next::codec::codec::Codec,
    codec_id: codec::Id,
    codec_ctx: &mut codec::context::Context,
    config: &DecoderConfig,
) -> (Option<HwDeviceContext>, Option<Pixel>, bool) {
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

    // Try v4l2m2m named codec (FFmpeg 4.x — no AVHWDeviceType for V4L2M2M)
    if let Some(codec_name) = v4l2m2m_codec_name(codec_id) {
        if let Some(v4l2_codec) = codec::decoder::find_by_name(codec_name) {
            log::info!("probe: trying named codec {}", codec_name);

            // Replace the codec context with one built for the v4l2m2m codec
            let mut v4l2_ctx = codec::context::Context::new_with_codec(v4l2_codec);
            apply_codec_flags(&mut v4l2_ctx, config);

            let open_ret = unsafe {
                let saved = ffi::av_log_get_level();
                ffi::av_log_set_level(8);
                let r = ffi::avcodec_open2(
                    v4l2_ctx.as_mut_ptr(),
                    v4l2_codec.as_ptr(),
                    std::ptr::null_mut(),
                );
                ffi::av_log_set_level(saved);
                r
            };

            if open_ret == 0 {
                log::info!("probe: {} opened successfully", codec_name);
                // Swap the caller's codec_ctx for the v4l2m2m one
                *codec_ctx = v4l2_ctx;
                return (None, None, true); // no hw_ctx needed, already open
            } else {
                log::info!("probe: {} failed ({}), continuing", codec_name, open_ret);
            }
        }
    }

    for (dt, node) in candidates {
        let node_str = node.as_deref();

        // 1. Open the hw device (cheap, no codec involvement).
        let hw_ctx = match HwDeviceContext::new(dt, node_str) {
            Ok(ctx) => ctx,
            Err(e) => {
                log::info!("skip {} {:?}: {}", dt.name(), node_str, e);
                continue;
            }
        };

        // 2. Check the static codec hw-config table so we skip completely
        //    before touching the codec context when there's no chance of success.
        let hw_fmts = hw_pixel_formats_for_codec(codec, dt);
        let hw_fmt = match hw_fmts.first().copied() {
            Some(f) => f,
            None => {
                log::info!("skip {}: no hw configs for {:?}", dt.name(), codec_id);
                continue;
            }
        };

        // 3. Wire device ctx + get_format callback using opaque for per-instance state.
        //    Heap-allocate the state; it lives until we drop it after open (success or fail).
        let state = Box::new(GetFormatState {
            desired: hw_fmt.into(),
        });
        let state_ptr = Box::into_raw(state);

        unsafe {
            let p = codec_ctx.as_mut_ptr();
            (*p).hw_device_ctx = hw_ctx.ref_ptr();
            (*p).get_format = Some(get_format);
            (*p).opaque = state_ptr as *mut std::ffi::c_void;
        }

        // 4. Attempt avcodec_open2.  Silence AV_LOG_ERROR messages during this
        //    call: FFmpeg emits "Invalid/Failed setup for format X" for every
        //    hw format that doesn't match our device type — these are normal
        //    negotiation side-effects, not real errors.
        //    AV_LOG_FATAL = 8, AV_LOG_ERROR = 16.
        let open_ret = unsafe {
            let saved = ffi::av_log_get_level();
            ffi::av_log_set_level(8); // suppress everything below FATAL
            let r =
                ffi::avcodec_open2(codec_ctx.as_mut_ptr(), codec.as_ptr(), std::ptr::null_mut());
            ffi::av_log_set_level(saved);
            r
        };

        // Reclaim the state box — it's no longer needed after open.
        unsafe {
            drop(Box::from_raw(state_ptr));
        }

        if open_ret < 0 {
            // This back-end was rejected (device exists but profile unsupported etc).
            // Detach the hw ctx ref we gave to the codec context before we drop
            // hw_ctx, otherwise the refcount goes negative.
            log::info!("skip {}: avcodec_open2 failed ({})", dt.name(), open_ret);
            unsafe {
                let p = codec_ctx.as_mut_ptr();
                if !(*p).hw_device_ctx.is_null() {
                    ffi::av_buffer_unref(&mut (*p).hw_device_ctx);
                }
                (*p).get_format = None;
                (*p).opaque = std::ptr::null_mut();
            }
            continue;
        }

        log::info!("hw: {} @ {:?}  fmt={:?}", dt.name(), node_str, hw_fmt);
        return (Some(hw_ctx), Some(hw_fmt), true);
    }

    if config.fallback_to_software {
        log::info!("no hw back-end available, using software decoding");
    }
    (None, None, false)
}

fn codec_device_node(dt: HwDeviceType, codec_id: codec::Id) -> Option<&'static str> {
    match dt {
        HwDeviceType::V4l2M2m => imx8mp_v4l2_device(codec_id).or_else(|| dt.default_device()),
        _ => dt.default_device(),
    }
}

fn apply_codec_flags(codec_ctx: &mut codec::context::Context, config: &DecoderConfig) {
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

fn drain_frames(decoder: &mut codec::decoder::Video, out: &mut Vec<SoftwareFrame>) -> Result<()> {
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
// open_codec_ctx — shared finalisation after probe_hw
//
// If probe_hw already opened the codec (hw path, already_opened == true),
// we reinterpret the Context as a decoder::Video without calling open2 again.
// If it didn't (sw path), we call decoder().video() which calls open2.
// ============================================================================

fn finish_open(
    codec_ctx: codec::context::Context,
    already_opened: bool,
) -> std::result::Result<codec::decoder::Video, ffmpeg_next::Error> {
    if already_opened {
        // SAFETY: codec::context::Context and codec::decoder::Video are both
        // #[repr(transparent)] wrappers over the same NonNull<AVCodecContext>.
        // The codec is already open so reinterpreting the wrapper is valid.
        Ok(unsafe {
            std::mem::transmute::<codec::context::Context, codec::decoder::Video>(codec_ctx)
        })
    } else {
        codec_ctx.decoder().video()
    }
}

// ============================================================================
// Decoder — container / URL mode
// ============================================================================

pub struct Decoder {
    input: format::context::Input,
    video_stream_index: usize,
    decoder: codec::decoder::Video,
    _hw_ctx: Option<HwDeviceContext>,
    pub is_hardware: bool,
    hw_pixel_format: Option<Pixel>,
}

impl Decoder {
    pub fn open_auto(path: &str) -> Result<Self> {
        Self::open(path, DecoderConfig::auto())
    }

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

        let codec = codec::decoder::find(codec_id)
            .ok_or_else(|| DecoderError::CodecNotFound(format!("{:?}", codec_id)))?;

        log::info!("container codec: {:?}", codec_id);

        let mut codec_ctx =
            codec::context::Context::from_parameters(codec_params).map_err(DecoderError::Ffmpeg)?;

        apply_codec_flags(&mut codec_ctx, &config);

        let (hw_ctx, hw_pixel_format, already_opened) =
            probe_hw(&codec, codec_id, &mut codec_ctx, &config);
        let is_hardware = hw_ctx.is_some();

        let decoder = finish_open(codec_ctx, already_opened).map_err(DecoderError::Ffmpeg)?;

        Ok(Self {
            input,
            video_stream_index,
            decoder,
            _hw_ctx: hw_ctx,
            is_hardware,
            hw_pixel_format,
        })
    }

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
/// The caller owns transport and framing. Each [`push_nal`](Self::push_nal)
/// sends one access unit directly to the codec, bypassing FFmpeg's demuxer.
///
/// **Annex-B** (start codes `00 00 00 01`): pass bytes directly.  
/// **AVCC / length-prefixed**: use [`new_with_extradata`](Self::new_with_extradata)
/// with the `DecoderConfigurationRecord` so FFmpeg can parse length prefixes.
pub struct NalDecoder {
    decoder: codec::decoder::Video,
    _hw_ctx: Option<HwDeviceContext>,
    pub is_hardware: bool,
    hw_pixel_format: Option<Pixel>,
    pending: Vec<SoftwareFrame>,
    flushed: bool,
}

impl NalDecoder {
    pub fn new(codec_id: codec::Id, config: DecoderConfig) -> Result<Self> {
        Self::new_with_extradata(codec_id, &[], config)
    }

    pub fn new_with_extradata(
        codec_id: codec::Id,
        extradata: &[u8],
        config: DecoderConfig,
    ) -> Result<Self> {
        ffmpeg_next::init().map_err(DecoderError::Ffmpeg)?;

        let codec = codec::decoder::find(codec_id)
            .ok_or_else(|| DecoderError::CodecNotFound(format!("{:?}", codec_id)))?;

        log::info!("NalDecoder codec: {:?}", codec_id);

        let mut codec_ctx = codec::context::Context::new_with_codec(codec);

        if !extradata.is_empty() {
            unsafe {
                let ctx = codec_ctx.as_mut_ptr();
                let buf = ffi::av_mallocz(
                    (extradata.len() + ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize) as _,
                ) as *mut u8;
                if buf.is_null() {
                    return Err(DecoderError::Ffmpeg(ffmpeg_next::Error::from(-12)));
                }
                std::ptr::copy_nonoverlapping(extradata.as_ptr(), buf, extradata.len());
                (*ctx).extradata = buf;
                (*ctx).extradata_size = extradata.len() as c_int;
            }
        }

        apply_codec_flags(&mut codec_ctx, &config);

        let (hw_ctx, hw_pixel_format, already_opened) =
            probe_hw(&codec, codec_id, &mut codec_ctx, &config);
        let is_hardware = hw_ctx.is_some();

        let decoder = finish_open(codec_ctx, already_opened).map_err(DecoderError::Ffmpeg)?;

        Ok(Self {
            decoder,
            _hw_ctx: hw_ctx,
            is_hardware,
            hw_pixel_format,
            pending: Vec::new(),
            flushed: false,
        })
    }

    /// Push one access unit of raw NAL data.
    ///
    /// After calling this, drain with [`next_frame`](Self::next_frame) until
    /// it returns `Ok(None)` before pushing the next unit.
    pub fn push_nal(&mut self, data: &[u8], pts: Option<i64>, dts: Option<i64>) -> Result<()> {
        if self.flushed {
            return Err(DecoderError::Eof);
        }

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
        drain_frames(&mut self.decoder, &mut self.pending)
    }

    /// Signal end-of-stream and flush buffered frames.
    pub fn flush(&mut self) -> Result<()> {
        if self.flushed {
            return Ok(());
        }
        self.flushed = true;
        self.decoder.send_eof().map_err(DecoderError::Ffmpeg)?;
        drain_frames(&mut self.decoder, &mut self.pending)
    }

    /// Return the next buffered decoded frame, or `Ok(None)` if none ready.
    pub fn next_frame(&mut self) -> Result<Option<SoftwareFrame>> {
        if self.pending.is_empty() {
            drain_frames(&mut self.decoder, &mut self.pending)?;
        }
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
// FrameIter
// ============================================================================

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
