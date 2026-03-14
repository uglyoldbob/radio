//! Hardware-accelerated video decoder built on top of [`ffmpeg-next`].
//!
//! # i.MX8 VPU support
//!
//! The i.MX8 family exposes its Video Processing Unit through two paths
//! depending on the BSP / kernel version you are running:
//!
//! | Path        | FFmpeg hw_type | Notes                               |
//! |-------------|----------------|-------------------------------------|
//! | V4L2 M2M    | `v4l2m2m`      | Preferred on mainline kernel ≥ 5.10 |
//! | DRM / VAAPI | `drm` / `vaapi`| Available via imx-gpu-viv or etnaviv|
//!
//! [`HwDeviceType::Imx8Vpu`] resolves to `v4l2m2m` at runtime so callers do
//! not need to know the underlying FFmpeg name.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use ffmpeg_hw_decoder::{Decoder, DecoderConfig, HwDeviceType};
//!
//! let config = DecoderConfig {
//!     hw_device_type: Some(HwDeviceType::Imx8Vpu),
//!     device_path: None,      // uses default /dev/video0
//!     thread_count: 1,        // VPU handles threading internally
//!     ..Default::default()
//! };
//!
//! let mut decoder = Decoder::open("input.mp4", config).unwrap();
//!
//! while let Some(frame) = decoder.next_frame().unwrap() {
//!     println!("pts={:?}  {}x{}", frame.pts(), frame.width(), frame.height());
//! }
//! ```

// ============================================================================
// Imports
// ============================================================================

use std::ffi::{c_int, CString};

use ffmpeg_next::{
    codec,
    ffi,
    format::{self, Pixel},
    frame::Video as VideoFrame,
    media::Type as MediaType,
    software::scaling::{context::Context as SwsContext, flag::Flags},
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

/// All supported hardware acceleration back-ends, including i.MX8-specific ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwDeviceType {
    /// i.MX8 VPU via V4L2 memory-to-memory (resolves to `v4l2m2m`)
    Imx8Vpu,
    /// Generic V4L2 M2M (Raspberry Pi, Allwinner, Rockchip, …)
    V4l2M2m,
    /// VA-API (Intel, AMD on Linux, some ARM SoCs via Mesa)
    Vaapi,
    /// NVIDIA NVDEC via CUDA
    Cuda,
    /// Intel Quick Sync
    Qsv,
    /// DRM prime buffers (zero-copy display pipeline)
    Drm,
    /// Vulkan compute / decode (FFmpeg ≥ 6.0)
    Vulkan,
    /// VDPAU (legacy NVIDIA / Nouveau)
    Vdpau,
}

impl HwDeviceType {
    /// Return the FFmpeg `AVHWDeviceType` constant for this back-end.
    pub fn as_av_hw_device_type(self) -> ffi::AVHWDeviceType {
        match self {
            // v4l2m2m covers the i.MX8 VPU on mainline kernels
            HwDeviceType::Imx8Vpu | HwDeviceType::V4l2M2m => {
                ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_V4L2M2M
            }
            HwDeviceType::Vaapi  => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            HwDeviceType::Cuda   => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
            HwDeviceType::Qsv    => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV,
            HwDeviceType::Drm    => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
            HwDeviceType::Vulkan => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN,
            HwDeviceType::Vdpau  => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VDPAU,
        }
    }

    /// Human-readable name used in log messages.
    pub fn name(self) -> &'static str {
        match self {
            HwDeviceType::Imx8Vpu => "imx8-vpu (v4l2m2m)",
            HwDeviceType::V4l2M2m => "v4l2m2m",
            HwDeviceType::Vaapi   => "vaapi",
            HwDeviceType::Cuda    => "cuda",
            HwDeviceType::Qsv     => "qsv",
            HwDeviceType::Drm     => "drm",
            HwDeviceType::Vulkan  => "vulkan",
            HwDeviceType::Vdpau   => "vdpau",
        }
    }

    /// Default device node path used when none is specified by the caller.
    ///
    /// On i.MX8 the VPU exposes itself as `/dev/video0` (or a higher index).
    /// Override with [`DecoderConfig::device_path`] if your board maps it differently.
    pub fn default_device(self) -> Option<&'static str> {
        match self {
            HwDeviceType::Imx8Vpu | HwDeviceType::V4l2M2m => Some("/dev/video0"),
            HwDeviceType::Vaapi => Some("/dev/dri/renderD128"),
            HwDeviceType::Drm   => Some("/dev/dri/card0"),
            _                   => None,
        }
    }

    /// Back-ends in preference order used by auto-detection.
    pub fn all_by_preference() -> &'static [HwDeviceType] {
        &[
            HwDeviceType::Imx8Vpu,
            HwDeviceType::Vaapi,
            HwDeviceType::Cuda,
            HwDeviceType::Qsv,
            HwDeviceType::V4l2M2m,
            HwDeviceType::Drm,
            HwDeviceType::Vulkan,
            HwDeviceType::Vdpau,
        ]
    }
}

// ============================================================================
// Hardware acceleration – device context
// ============================================================================

/// An owned FFmpeg hardware device context (`AVBufferRef *`).
///
/// The context is reference-counted by FFmpeg; we hold one reference and
/// release it on drop via `av_buffer_unref`.
pub struct HwDeviceContext {
    /// Raw pointer to the `AVBufferRef` wrapping an `AVHWDeviceContext`.
    ptr: *mut ffi::AVBufferRef,
    pub device_type: HwDeviceType,
}

// SAFETY: The pointer is only ever accessed from one thread at a time.
unsafe impl Send for HwDeviceContext {}
unsafe impl Sync for HwDeviceContext {}

impl HwDeviceContext {
    /// Create a hardware device context for the given back-end.
    ///
    /// `device` overrides the default device node (e.g. `/dev/video1`).
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

        let mut hw_device_ctx: *mut ffi::AVBufferRef = std::ptr::null_mut();

        let ret = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                device_type.as_av_hw_device_type(),
                c_device
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(std::ptr::null()),
                std::ptr::null_mut(), // opts
                0,                    // flags
            )
        };

        if ret < 0 || hw_device_ctx.is_null() {
            return Err(DecoderError::HwDeviceCreate(format!(
                "{} (device={:?}, ffmpeg_err={})",
                device_type.name(),
                device,
                ret
            )));
        }

        log::debug!(
            "Created hw device context: {} ({})",
            device_type.name(),
            device_path
        );

        Ok(Self { ptr: hw_device_ctx, device_type })
    }

    /// Try every back-end in preference order; return the first that succeeds.
    pub fn auto_detect(device: Option<&str>) -> Result<Self> {
        for &dt in HwDeviceType::all_by_preference() {
            match Self::new(dt, device) {
                Ok(ctx) => {
                    log::info!("Hardware acceleration: {} selected", dt.name());
                    return Ok(ctx);
                }
                Err(e) => log::debug!("hw back-end {} unavailable: {}", dt.name(), e),
            }
        }
        Err(DecoderError::HwAccelUnavailable(
            "no supported hw back-end found".into(),
        ))
    }

    /// Return a new reference to the underlying `AVBufferRef` suitable for
    /// handing to an `AVCodecContext`.  The reference count is incremented.
    fn ref_ptr(&self) -> *mut ffi::AVBufferRef {
        // SAFETY: ptr is valid for the lifetime of self.
        unsafe { ffi::av_buffer_ref(self.ptr) }
    }
}

impl Drop for HwDeviceContext {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: we own exactly one reference.
            unsafe { ffi::av_buffer_unref(&mut self.ptr) };
        }
    }
}

/// Query which pixel formats `codec` advertises as hardware-accelerated for
/// `device_type`.  Returns an empty `Vec` if the codec has no hw configs.
pub fn hw_pixel_formats_for_codec(
    codec: &ffmpeg_next::codec::Codec,
    device_type: HwDeviceType,
) -> Vec<Pixel> {
    let mut formats = Vec::new();
    let av_type = device_type.as_av_hw_device_type();
    let mut i = 0i32;

    loop {
        // SAFETY: simple index lookup; returns null at end-of-list.
        let cfg = unsafe { ffi::av_codec_get_hw_config(codec.as_ptr(), i) };
        if cfg.is_null() {
            break;
        }
        let cfg = unsafe { &*cfg };
        if cfg.device_type == av_type
            && (cfg.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32) != 0
        {
            let pix_fmt = Pixel::from(cfg.pix_fmt);
            if pix_fmt != Pixel::None {
                formats.push(pix_fmt);
            }
        }
        i += 1;
    }
    formats
}

// ============================================================================
// Frame – hardware → system memory transfer
// ============================================================================

/// A decoded video frame that is guaranteed to live in system (CPU-accessible)
/// memory.
///
/// Hardware back-ends keep decoded surfaces in device memory.  Before the CPU
/// can read pixel data the frame must be *transferred* (downloaded).
/// [`SoftwareFrame::from_hw_frame`] performs that transfer; if the frame is
/// already in system memory it is returned zero-copy.
pub struct SoftwareFrame {
    inner: VideoFrame,
}

impl SoftwareFrame {
    /// Transfer a hardware-resident `VideoFrame` to system memory.
    ///
    /// If the frame is already in system memory it is returned as-is
    /// (increments the FFmpeg refcount only).
    pub fn from_hw_frame(hw_frame: &VideoFrame) -> Result<Self> {
        if is_hardware_pixel_format(hw_frame.format()) {
            let mut sw_frame = VideoFrame::empty();

            let ret = unsafe {
                ffi::av_hwframe_transfer_data(sw_frame.as_mut_ptr(), hw_frame.as_ptr(), 0)
            };
            if ret < 0 {
                return Err(DecoderError::FrameTransfer);
            }

            // Copy metadata (pts, dts, duration, …) from hw frame to sw frame.
            unsafe {
                ffi::av_frame_copy_props(sw_frame.as_mut_ptr(), hw_frame.as_ptr());
            }

            log::trace!(
                "Transferred hw frame ({:?}) → sw frame ({:?})  {}x{}",
                hw_frame.format(),
                sw_frame.format(),
                sw_frame.width(),
                sw_frame.height(),
            );

            Ok(Self { inner: sw_frame })
        } else {
            // Already in system memory – add one reference.
            let mut sw_frame = VideoFrame::empty();
            let ret = unsafe { ffi::av_frame_ref(sw_frame.as_mut_ptr(), hw_frame.as_ptr()) };
            if ret < 0 {
                return Err(DecoderError::FrameTransfer);
            }
            Ok(Self { inner: sw_frame })
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 { self.inner.width() }

    /// Height in pixels.
    pub fn height(&self) -> u32 { self.inner.height() }

    /// Pixel format of the transferred frame (e.g. `NV12`, `YUV420P`).
    pub fn format(&self) -> Pixel { self.inner.format() }

    /// Presentation timestamp in the stream's time base.
    pub fn pts(&self) -> Option<i64> { self.inner.pts() }

    /// Immutable access to the underlying `ffmpeg_next` frame.
    pub fn as_video_frame(&self) -> &VideoFrame { &self.inner }

    /// Raw planar data for plane `index`.
    pub fn plane_data(&self, index: usize) -> &[u8] { self.inner.data(index) }

    /// Stride (linesize) for plane `index`.
    pub fn linesize(&self, index: usize) -> usize { self.inner.stride(index) }

    /// Convert this frame to RGB24 using libswscale.
    ///
    /// Useful for saving PNGs or feeding into an image-processing pipeline.
    pub fn to_rgb24(&self) -> Result<VideoFrame> {
        let src_fmt = self.inner.format();
        let dst_fmt = Pixel::RGB24;
        let w = self.inner.width();
        let h = self.inner.height();

        let mut sws = SwsContext::get(src_fmt, w, h, dst_fmt, w, h, Flags::BILINEAR)
            .map_err(|_| DecoderError::UnsupportedPixelFormat(src_fmt))?;

        let mut dst = VideoFrame::new(dst_fmt, w, h);
        sws.run(&self.inner, &mut dst)
            .map_err(|_| DecoderError::UnsupportedPixelFormat(src_fmt))?;

        Ok(dst)
    }
}

/// Return `true` if `pixel` carries the `AV_PIX_FMT_FLAG_HWACCEL` flag.
pub fn is_hardware_pixel_format(pixel: Pixel) -> bool {
    // SAFETY: returns a pointer into a static libavutil table; null for unknown formats.
    let desc = unsafe { ffi::av_pix_fmt_desc_get(pixel.into()) };
    if desc.is_null() {
        return false;
    }
    (unsafe { (*desc).flags } & ffi::AV_PIX_FMT_FLAG_HWACCEL as u64) != 0
}

// ============================================================================
// Decoder configuration
// ============================================================================

/// Configuration for [`Decoder::open`].
#[derive(Debug, Clone)]
pub struct DecoderConfig {
    /// Which hardware back-end to use.  `None` triggers auto-detection.
    pub hw_device_type: Option<HwDeviceType>,

    /// Override the device node (e.g. `/dev/video1` or `/dev/dri/renderD129`).
    /// `None` → use [`HwDeviceType::default_device`].
    pub device_path: Option<String>,

    /// Fall back to software decoding if every hardware back-end fails.
    /// Default: `true`.
    pub fallback_to_software: bool,

    /// Number of codec threads for software decoding.
    /// For hardware decoders the VPU/GPU manages concurrency internally.
    /// Default: `0` (let FFmpeg decide).
    pub thread_count: u32,

    /// Disable b-frame reordering for low-latency / live-stream pipelines.
    /// Default: `false`.
    pub low_latency: bool,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            hw_device_type: None,
            device_path: None,
            fallback_to_software: true,
            thread_count: 0,
            low_latency: false,
        }
    }
}

// ============================================================================
// get_format callback (thread-local state)
// ============================================================================

/// FFmpeg calls `get_format` during `avcodec_open2` to negotiate the pixel
/// format.  Because it is a plain C callback we carry the desired format via
/// thread-local storage rather than a closure.
mod get_format_state {
    use std::cell::Cell;
    use ffmpeg_next::{ffi, format::Pixel};

    thread_local! {
        static DESIRED_HW_FMT: Cell<ffi::AVPixelFormat> =
            Cell::new(ffi::AVPixelFormat::AV_PIX_FMT_NONE);
    }

    pub fn set(fmt: Pixel) {
        DESIRED_HW_FMT.with(|c| c.set(fmt.into()));
    }

    pub fn get() -> ffi::AVPixelFormat {
        DESIRED_HW_FMT.with(|c| c.get())
    }
}

/// C callback installed on `AVCodecContext::get_format`.
///
/// Walks the null-terminated candidate list FFmpeg provides and returns the
/// desired hardware format if present, otherwise the first software format.
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
    // Desired hw format not offered; fall back to first candidate (software).
    unsafe { *fmt_list }
}

// ============================================================================
// Decoder
// ============================================================================

/// A hardware-accelerated (or software-fallback) video decoder.
///
/// Owns the demuxer, codec context, and (optionally) the hardware device
/// context.  Frames are decoded on demand via [`Decoder::next_frame`] or the
/// [`Decoder::frames`] iterator.
pub struct Decoder {
    input: format::context::Input,
    video_stream_index: usize,
    decoder: codec::decoder::Video,
    _hw_ctx: Option<HwDeviceContext>,
    /// `true` when a hardware back-end was successfully initialised.
    pub is_hardware: bool,
    hw_pixel_format: Option<Pixel>,
}

impl Decoder {
    /// Open a media file or URL and initialise the decoder according to `config`.
    pub fn open(path: &str, config: DecoderConfig) -> Result<Self> {
        ffmpeg_next::init().map_err(DecoderError::Ffmpeg)?;

        let input = format::input(&path).map_err(DecoderError::Ffmpeg)?;

        let stream = input
            .streams()
            .best(MediaType::Video)
            .ok_or(DecoderError::NoVideoStream)?;
        let video_stream_index = stream.index();

        let codec_params = stream.parameters();
        let codec_id    = codec_params.id();

        let codec = codec::decoder::find(codec_id)
            .ok_or_else(|| DecoderError::CodecNotFound(format!("{:?}", codec_id)))?;

        log::debug!("Codec: {:?}", codec_id);

        let mut codec_ctx =
            codec::context::Context::from_parameters(codec_params)
                .map_err(DecoderError::Ffmpeg)?;

        // Thread count (mainly useful for sw fallback).
        if config.thread_count > 0 {
            unsafe {
                (*codec_ctx.as_mut_ptr()).thread_count = config.thread_count as c_int;
            }
        }

        // Low-latency: disable b-frame delay.
        if config.low_latency {
            unsafe {
                (*codec_ctx.as_mut_ptr()).flags  |= ffi::AV_CODEC_FLAG_LOW_DELAY as c_int;
                (*codec_ctx.as_mut_ptr()).flags2 |= ffi::AV_CODEC_FLAG2_FAST as c_int;
            }
        }

        let (hw_ctx, hw_pixel_format) = Self::try_setup_hw(&codec, &mut codec_ctx, &config);
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

    /// Decode and return the next video frame, downloading from the GPU if needed.
    ///
    /// Returns `Ok(None)` at end-of-stream.
    pub fn next_frame(&mut self) -> Result<Option<SoftwareFrame>> {
        loop {
            match self.receive_frame() {
                Ok(Some(f))            => return Ok(Some(f)),
                Ok(None)               => {} // need more packets
                Err(DecoderError::Eof) => return Ok(None),
                Err(e)                 => return Err(e),
            }

            match self.send_next_packet()? {
                PacketResult::Sent => {}
                PacketResult::Eof  => {
                    self.decoder.send_eof().map_err(DecoderError::Ffmpeg)?;
                    loop {
                        match self.receive_frame() {
                            Ok(Some(f))                           => return Ok(Some(f)),
                            Ok(None) | Err(DecoderError::Eof)    => return Ok(None),
                            Err(e)                               => return Err(e),
                        }
                    }
                }
            }
        }
    }

    /// Return an iterator over all decoded frames.
    pub fn frames(&mut self) -> FrameIter<'_> {
        FrameIter { decoder: self }
    }

    /// Width of the video stream in pixels.
    pub fn width(&self) -> u32 { self.decoder.width() }

    /// Height of the video stream in pixels.
    pub fn height(&self) -> u32 { self.decoder.height() }

    /// Pixel format reported by the codec context.
    pub fn format(&self) -> Pixel { self.decoder.format() }

    /// Negotiated hardware pixel format, if hardware decoding is active.
    pub fn hw_pixel_format(&self) -> Option<Pixel> { self.hw_pixel_format }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Attempt to create a hardware device context and wire it into `codec_ctx`.
    ///
    /// Returns `(None, None)` on any failure so the caller can proceed with
    /// software decoding.
    fn try_setup_hw(
        codec: &codec::Codec,
        codec_ctx: &mut codec::context::Context,
        config: &DecoderConfig,
    ) -> (Option<HwDeviceContext>, Option<Pixel>) {
        let device_path = config.device_path.as_deref();

        let hw_ctx = if let Some(dt) = config.hw_device_type {
            match HwDeviceContext::new(dt, device_path) {
                Ok(ctx) => {
                    log::info!("Using hw back-end: {}", dt.name());
                    Some(ctx)
                }
                Err(e) => {
                    log::warn!("Requested hw back-end failed: {}; trying auto-detect", e);
                    if config.fallback_to_software {
                        HwDeviceContext::auto_detect(device_path).ok()
                    } else {
                        None
                    }
                }
            }
        } else {
            match HwDeviceContext::auto_detect(device_path) {
                Ok(ctx) => Some(ctx),
                Err(e) => {
                    log::warn!("No hw back-end found ({}), using software", e);
                    None
                }
            }
        };

        let hw_ctx = match hw_ctx {
            Some(c) => c,
            None    => return (None, None),
        };

        let hw_fmts = hw_pixel_formats_for_codec(codec, hw_ctx.device_type);
        let hw_fmt  = match hw_fmts.first().copied() {
            Some(f) => f,
            None => {
                log::warn!(
                    "Codec {:?} has no hw configs for {}; falling back to software",
                    codec.id(),
                    hw_ctx.device_type.name()
                );
                return (None, None);
            }
        };

        log::debug!("Negotiated hw pixel format: {:?}", hw_fmt);
        get_format_state::set(hw_fmt);

        unsafe {
            let ctx_ptr = codec_ctx.as_mut_ptr();
            (*ctx_ptr).hw_device_ctx = hw_ctx.ref_ptr();   // transfer one ref
            (*ctx_ptr).get_format    = Some(get_format);   // install callback
        }

        (Some(hw_ctx), Some(hw_fmt))
    }

    /// Pull one decoded frame from the codec.  Returns `Ok(None)` when the
    /// codec needs more packets (`EAGAIN`).
    fn receive_frame(&mut self) -> Result<Option<SoftwareFrame>> {
        let mut hw_frame = VideoFrame::empty();
        match self.decoder.receive_frame(&mut hw_frame) {
            Ok(()) => Ok(Some(SoftwareFrame::from_hw_frame(&hw_frame)?)),
            Err(ffmpeg_next::Error::Other { errno })
                if errno == ffmpeg_next::error::EAGAIN => Ok(None),
            Err(ffmpeg_next::Error::Eof) => Err(DecoderError::Eof),
            Err(e) => Err(DecoderError::Ffmpeg(e)),
        }
    }

    /// Read the next video packet from the demuxer and send it to the codec.
    fn send_next_packet(&mut self) -> Result<PacketResult> {
        for (stream, packet) in self.input.packets() {
            if stream.index() == self.video_stream_index {
                self.decoder.send_packet(&packet).map_err(DecoderError::Ffmpeg)?;
                return Ok(PacketResult::Sent);
            }
        }
        Ok(PacketResult::Eof)
    }
}

// ============================================================================
// FrameIter
// ============================================================================

enum PacketResult { Sent, Eof }

/// Iterator returned by [`Decoder::frames`].
pub struct FrameIter<'a> {
    decoder: &'a mut Decoder,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Result<SoftwareFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.decoder.next_frame() {
            Ok(Some(f)) => Some(Ok(f)),
            Ok(None)    => None,
            Err(e)      => Some(Err(e)),
        }
    }
}