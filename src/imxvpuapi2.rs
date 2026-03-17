//! Hardware H.264 decoder for the i.MX8MP using `libimxvpuapi2` (Hantro VPU).
//!
//! # Architecture
//!
//! `libimxvpuapi2` wraps the proprietary Hantro G1/G2 userspace library.
//! The decode loop is event-driven: you push one encoded NAL unit at a time
//! with [`imx_vpu_api_dec_push_encoded_frame`] and then spin
//! [`imx_vpu_api_dec_decode`] until it signals that it needs more input data.
//! Each iteration may produce a fully-decoded frame, request more DMA
//! framebuffers, or report new stream parameters.
//!
//! # DMA memory
//!
//! The VPU requires all buffers (stream ring-buffer, framebuffer pool, optional
//! separate output buffer) to be physically contiguous DMA memory, allocated
//! through `libimxdmabuffer`.
//!
//! # Struct layout notes
//!
//! All `#[repr(C)]` structs below are manually matched to the C ABI on
//! 64-bit ARM (aarch64 / i.MX8MP).  Fields are documented with their byte
//! offset so they can be cross-checked against the upstream header
//! `imxvpuapi2/imxvpuapi2.h`.

#![allow(dead_code, non_camel_case_types)]

use std::os::raw::{c_int, c_uint, c_void};

// ============================================================================
// Constants
// ============================================================================

// ---- ImxVpuApiDecReturnCodes -----------------------------------------------
const IMX_VPU_API_DEC_RETURN_CODE_OK: u32 = 0;

// ---- ImxVpuApiDecOutputCodes -----------------------------------------------
const IMX_VPU_API_DEC_OUTPUT_CODE_NO_OUTPUT_YET_AVAILABLE: u32 = 0;
const IMX_VPU_API_DEC_OUTPUT_CODE_EOS: u32 = 1;
const IMX_VPU_API_DEC_OUTPUT_CODE_NEW_STREAM_INFO_AVAILABLE: u32 = 2;
const IMX_VPU_API_DEC_OUTPUT_CODE_NEED_ADDITIONAL_FRAMEBUFFER: u32 = 3;
const IMX_VPU_API_DEC_OUTPUT_CODE_DECODED_FRAME_AVAILABLE: u32 = 4;
const IMX_VPU_API_DEC_OUTPUT_CODE_MORE_INPUT_DATA_NEEDED: u32 = 5;
const IMX_VPU_API_DEC_OUTPUT_CODE_FRAME_SKIPPED: u32 = 6;
const IMX_VPU_API_DEC_OUTPUT_CODE_VIDEO_PARAMETERS_CHANGED: u32 = 7;

// ---- ImxVpuApiDecGlobalInfoFlags -------------------------------------------
/// The codec can decode (not just encode).
const IMX_VPU_API_DEC_GLOBAL_INFO_FLAG_HAS_DECODER: u32 = 1 << 0;
/// Decoded frames are taken from the decoder's internal DMA buffer pool;
/// the caller must return them when done.
const IMX_VPU_API_DEC_GLOBAL_INFO_FLAG_DECODED_FRAMES_ARE_FROM_BUFFER_POOL: u32 = 1 << 3;

// ---- ImxVpuApiDecOpenParamsFlags -------------------------------------------
/// Allow the decoder to reorder frames (required for B-frame support in H.264).
const IMX_VPU_API_DEC_OPEN_PARAMS_FLAG_ENABLE_FRAME_REORDERING: u32 = 1 << 0;
/// Request semi-planar (NV12) output instead of fully-planar (I420).
const IMX_VPU_API_DEC_OPEN_PARAMS_FLAG_USE_SEMI_PLANAR_COLOR_FORMAT: u32 = 1 << 5;

// ---- ImxVpuApiCompressionFormat --------------------------------------------
const IMX_VPU_API_COMPRESSION_FORMAT_H264: u32 = 5;

// ---- ImxDmaBufferMappingFlags ----------------------------------------------
const IMX_DMA_BUFFER_MAPPING_FLAG_READ: u32 = 1 << 0;

// Reserved block size used in several structs for forward ABI compatibility.
const IMX_VPU_API_RESERVED_SIZE: usize = 64;

// ============================================================================
// Opaque C types
// ============================================================================

/// Opaque VPU decoder handle.
#[repr(C)]
pub struct ImxVpuApiDecoder {
    _opaque: [u8; 0],
}

/// Opaque DMA buffer handle (physically-contiguous memory).
#[repr(C)]
pub struct ImxDmaBuffer {
    _opaque: [u8; 0],
}

/// Opaque DMA buffer allocator handle.
#[repr(C)]
pub struct ImxDmaBufferAllocator {
    _opaque: [u8; 0],
}

// ============================================================================
// C structs (manually laid out for aarch64)
// ============================================================================

/// Metrics describing the layout of one framebuffer in memory.
///
/// Byte offsets (aarch64):
/// ```text
///  0  aligned_frame_width
///  8  aligned_frame_height
/// 16  actual_frame_width
/// 24  actual_frame_height
/// 32  y_stride
/// 40  uv_stride
/// 48  y_size
/// 56  uv_size
/// 64  y_offset
/// 72  u_offset
/// 80  v_offset
/// 88  reserved[64]
/// total: 152
/// ```
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImxVpuApiFramebufferMetrics {
    aligned_frame_width: usize,
    aligned_frame_height: usize,
    actual_frame_width: usize,
    actual_frame_height: usize,
    y_stride: usize,
    uv_stride: usize,
    y_size: usize,
    uv_size: usize,
    /// Byte offset of the Y plane from the start of the DMA buffer.
    y_offset: usize,
    /// Byte offset of the interleaved UV plane (semi-planar) or U plane (planar).
    u_offset: usize,
    /// Byte offset of the V plane (planar only; unused for NV12).
    v_offset: usize,
    _reserved: [u8; IMX_VPU_API_RESERVED_SIZE],
}

/// HDR mastering display metadata embedded in H.265 streams.
/// 14 × u32 = 56 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImxVpuApiDecHDRMetadata {
    red_primary_x: u32,
    red_primary_y: u32,
    green_primary_x: u32,
    green_primary_y: u32,
    blue_primary_x: u32,
    blue_primary_y: u32,
    white_point_x: u32,
    white_point_y: u32,
    xy_range: [u32; 2],
    min_mastering_luminance: u32,
    max_mastering_luminance: u32,
    max_content_light_level: u32,
    max_frame_average_light_level: u32,
}

/// Colour primaries / transfer / matrix from H.265 VUI.  3 × u32 = 12 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImxVpuApiDecColorDescription {
    color_primaries: u32,
    transfer_characteristics: u32,
    matrix_coefficients: u32,
}

/// Chroma sample location from H.265 VUI.  2 × u32 = 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImxVpuApiDecLocationOfChromaInfo {
    chroma_sample_loc_type_top_field: u32,
    chroma_sample_loc_type_bottom_field: u32,
}

/// Information about the current stream, returned by
/// [`imx_vpu_api_dec_get_stream_info`] after the decoder signals
/// `IMX_VPU_API_DEC_OUTPUT_CODE_NEW_STREAM_INFO_AVAILABLE`.
///
/// Byte offsets (aarch64):
/// ```text
///   0  min_fb_pool_framebuffer_size
///   8  min_output_framebuffer_size
///  16  fb_pool_framebuffer_alignment
///  24  output_framebuffer_alignment
///  32  decoded_frame_framebuffer_metrics  (152 bytes)
/// 184  has_crop_rectangle  (i32, 4 bytes)
/// 188  _pad0  (4 bytes, aligns next size_t to 8)
/// 192  crop_left
/// 200  crop_top
/// 208  crop_width
/// 216  crop_height
/// 224  frame_rate_numerator  (u32)
/// 228  frame_rate_denominator  (u32)
/// 232  min_num_required_framebuffers
/// 240  color_format  (u32)
/// 244  video_full_range_flag  (u32)
/// 248  hdr_metadata  (56 bytes)
/// 304  color_description  (12 bytes)
/// 316  location_of_chroma_info  (8 bytes)
/// 324  flags  (u32)
/// 328  reserved[64]
/// total: 392
/// ```
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImxVpuApiDecStreamInfo {
    min_fb_pool_framebuffer_size: usize,
    min_output_framebuffer_size: usize,
    fb_pool_framebuffer_alignment: usize,
    output_framebuffer_alignment: usize,
    decoded_frame_framebuffer_metrics: ImxVpuApiFramebufferMetrics,
    has_crop_rectangle: c_int,
    _pad0: [u8; 4],
    crop_left: usize,
    crop_top: usize,
    crop_width: usize,
    crop_height: usize,
    frame_rate_numerator: c_uint,
    frame_rate_denominator: c_uint,
    min_num_required_framebuffers: usize,
    color_format: u32,
    video_full_range_flag: u32,
    hdr_metadata: ImxVpuApiDecHDRMetadata,
    color_description: ImxVpuApiDecColorDescription,
    location_of_chroma_info: ImxVpuApiDecLocationOfChromaInfo,
    flags: u32,
    _reserved: [u8; IMX_VPU_API_RESERVED_SIZE],
}

/// Global, static capabilities of the underlying VPU decoder hardware.
///
/// Byte offsets (aarch64):
/// ```text
///  0  flags  (u32)
///  4  hardware_type  (u32)
///  8  min_required_stream_buffer_size
/// 16  required_stream_buffer_physaddr_alignment
/// 24  required_stream_buffer_size_alignment
/// 32  supported_compression_formats  (ptr)
/// 40  num_supported_compression_formats
/// 48  reserved[64]
/// total: 112
/// ```
#[repr(C)]
struct ImxVpuApiDecGlobalInfo {
    flags: u32,
    hardware_type: u32,
    min_required_stream_buffer_size: usize,
    required_stream_buffer_physaddr_alignment: usize,
    required_stream_buffer_size_alignment: usize,
    supported_compression_formats: *const u32,
    num_supported_compression_formats: usize,
    _reserved: [u8; IMX_VPU_API_RESERVED_SIZE],
}

/// Parameters passed to [`imx_vpu_api_dec_open`].
///
/// Byte offsets (aarch64):
/// ```text
///  0  compression_format  (u32)
///  4  flags  (u32)
///  8  frame_width
/// 16  frame_height
/// 24  extra_header_data  (ptr)
/// 32  extra_header_data_size
/// 40  suggested_color_format  (u32)
/// 44  reserved[60]   (= IMX_VPU_API_RESERVED_SIZE - sizeof(u32))
/// total: 104
/// ```
#[repr(C)]
struct ImxVpuApiDecOpenParams {
    compression_format: u32,
    flags: u32,
    frame_width: usize,
    frame_height: usize,
    extra_header_data: *const u8,
    extra_header_data_size: usize,
    suggested_color_format: u32,
    _reserved: [u8; IMX_VPU_API_RESERVED_SIZE - std::mem::size_of::<u32>()],
}

/// An encoded frame pushed into the decoder.
///
/// Byte offsets (aarch64):
/// ```text
///  0  data  (ptr)
///  8  data_size
/// 16  has_header  (i32)
/// 20  frame_type  (u32)
/// 24  context  (ptr)
/// 32  pts  (u64)
/// 40  dts  (u64)
/// total: 48
/// ```
#[repr(C)]
struct ImxVpuApiEncodedFrame {
    data: *mut u8,
    data_size: usize,
    has_header: c_int,
    frame_type: u32,
    context: *mut c_void,
    pts: u64,
    dts: u64,
}

/// A decoded raw frame returned by the decoder.
///
/// Byte offsets (aarch64):
/// ```text
///  0  fb_dma_buffer  (ptr)
///  8  fb_context  (ptr)
/// 16  frame_types[2]  (2 × u32)
/// 24  interlacing_mode  (u32)
/// 28  _pad  (4 bytes, aligns next ptr to 8)
/// 32  context  (ptr)
/// 40  pts  (u64)
/// 48  dts  (u64)
/// total: 56
/// ```
#[repr(C)]
#[derive(Default)]
struct ImxVpuApiRawFrame {
    fb_dma_buffer: *mut ImxDmaBuffer,
    fb_context: *mut c_void,
    frame_types: [u32; 2],
    interlacing_mode: u32,
    _pad: [u8; 4],
    context: *mut c_void,
    pts: u64,
    dts: u64,
}

impl Default for ImxVpuApiRawFrame {
    fn default() -> Self {
        Self {
            fb_dma_buffer: std::ptr::null_mut(),
            fb_context: std::ptr::null_mut(),
            frame_types: [0; 2],
            interlacing_mode: 0,
            _pad: [0; 4],
            context: std::ptr::null_mut(),
            pts: 0,
            dts: 0,
        }
    }
}

// ============================================================================
// FFI declarations
// ============================================================================

extern "C" {
    // ---- libimxdmabuffer ---------------------------------------------------

    /// Create a new DMA buffer allocator using the default backend
    /// (typically the ION / DMA-heap allocator on i.MX8MP).
    fn imx_dma_buffer_allocator_new(error_code: *mut c_int) -> *mut ImxDmaBufferAllocator;

    /// Destroy a previously created allocator.  All DMA buffers allocated
    /// through it must have been deallocated first.
    fn imx_dma_buffer_allocator_destroy(allocator: *mut ImxDmaBufferAllocator);

    /// Allocate a new DMA buffer of `size` bytes with the given physical-address
    /// `alignment`.  Returns `NULL` on failure and sets `*error_code`.
    fn imx_dma_buffer_allocate(
        allocator: *mut ImxDmaBufferAllocator,
        size: usize,
        alignment: usize,
        error_code: *mut c_int,
    ) -> *mut ImxDmaBuffer;

    /// Free a DMA buffer.  It must not be currently mapped.
    fn imx_dma_buffer_deallocate(dma_buffer: *mut ImxDmaBuffer);

    /// Map a DMA buffer into the calling process's virtual address space.
    /// `mapping_flags` is a bitwise-OR of `IMX_DMA_BUFFER_MAPPING_FLAG_*`.
    /// Returns a pointer to the mapped region, or `NULL` on failure.
    fn imx_dma_buffer_map(
        dma_buffer: *mut ImxDmaBuffer,
        mapping_flags: u32,
        error_code: *mut c_int,
    ) -> *mut u8;

    /// Unmap a previously mapped DMA buffer.
    fn imx_dma_buffer_unmap(dma_buffer: *mut ImxDmaBuffer);

    // ---- libimxvpuapi2 – logging -------------------------------------------

    /// Set the minimum log level that the library will emit.
    fn imx_vpu_api_set_logging_threshold(threshold: u32);

    // ---- libimxvpuapi2 – decoder -------------------------------------------

    /// Return a pointer to the global, static decoder capabilities.
    /// The returned pointer is never NULL and must not be freed.
    fn imx_vpu_api_dec_get_global_info() -> *const ImxVpuApiDecGlobalInfo;

    /// Open a new decoder instance.  On success `*decoder` is set.
    fn imx_vpu_api_dec_open(
        decoder: *mut *mut ImxVpuApiDecoder,
        open_params: *mut ImxVpuApiDecOpenParams,
        stream_buffer: *mut ImxDmaBuffer,
    ) -> u32;

    /// Close and free a decoder instance.
    fn imx_vpu_api_dec_close(decoder: *mut ImxVpuApiDecoder);

    /// Retrieve stream information after the decoder signals
    /// `NEW_STREAM_INFO_AVAILABLE`.  The returned pointer refers to internal
    /// decoder state and must not be freed.
    fn imx_vpu_api_dec_get_stream_info(
        decoder: *mut ImxVpuApiDecoder,
    ) -> *const ImxVpuApiDecStreamInfo;

    /// Add DMA framebuffers to the decoder's pool.
    fn imx_vpu_api_dec_add_framebuffers_to_pool(
        decoder: *mut ImxVpuApiDecoder,
        fb_dma_buffers: *mut *mut ImxDmaBuffer,
        fb_contexts: *mut *mut c_void,
        num_framebuffers: usize,
    ) -> u32;

    /// Specify the DMA buffer that the next decoded frame shall be written into.
    /// Only relevant when `DECODED_FRAMES_ARE_FROM_BUFFER_POOL` is **not** set.
    fn imx_vpu_api_dec_set_output_frame_dma_buffer(
        decoder: *mut ImxVpuApiDecoder,
        output_frame_dma_buffer: *mut ImxDmaBuffer,
        fb_context: *mut c_void,
    );

    /// Push one encoded frame (NAL unit or access unit) into the decoder.
    fn imx_vpu_api_dec_push_encoded_frame(
        decoder: *mut ImxVpuApiDecoder,
        encoded_frame: *mut ImxVpuApiEncodedFrame,
    ) -> u32;

    /// Advance the decode state machine by one step.
    /// `*output_code` describes what happened.
    fn imx_vpu_api_dec_decode(
        decoder: *mut ImxVpuApiDecoder,
        output_code: *mut u32,
    ) -> u32;

    /// Retrieve a fully decoded frame after the decoder signals
    /// `DECODED_FRAME_AVAILABLE`.  This **must** be called before the next
    /// `imx_vpu_api_dec_decode` call.
    fn imx_vpu_api_dec_get_decoded_frame(
        decoder: *mut ImxVpuApiDecoder,
        decoded_frame: *mut ImxVpuApiRawFrame,
    ) -> u32;

    /// Return a framebuffer to the decoder's pool so it can be reused.
    /// Only needed when `DECODED_FRAMES_ARE_FROM_BUFFER_POOL` is set.
    fn imx_vpu_api_dec_return_framebuffer_to_decoder(
        decoder: *mut ImxVpuApiDecoder,
        fb_dma_buffer: *mut ImxDmaBuffer,
    );

    /// Enable drain mode: the decoder will flush all remaining decoded frames
    /// without expecting further encoded input.
    fn imx_vpu_api_dec_enable_drain_mode(decoder: *mut ImxVpuApiDecoder);

    /// Flush the decoder, discarding all queued input and output.
    fn imx_vpu_api_dec_flush(decoder: *mut ImxVpuApiDecoder);

    /// Human-readable description of a decoder return code (for logging).
    fn imx_vpu_api_dec_return_code_string(code: u32) -> *const std::os::raw::c_char;

    /// Human-readable description of a decoder output code (for logging).
    fn imx_vpu_api_dec_output_code_string(code: u32) -> *const std::os::raw::c_char;
}

// ============================================================================
// Safe helpers
// ============================================================================

/// Convert a C return-code pointer to a `&str` for log messages.
///
/// # Safety
/// The pointer must come from one of the `imx_vpu_api_dec_*_string` functions,
/// which return static string literals.
unsafe fn ret_code_str(code: u32) -> &'static str {
    let ptr = imx_vpu_api_dec_return_code_string(code);
    if ptr.is_null() {
        return "<null>";
    }
    std::ffi::CStr::from_ptr(ptr)
        .to_str()
        .unwrap_or("<invalid utf8>")
}

/// Same as [`ret_code_str`] but for output codes.
unsafe fn output_code_str(code: u32) -> &'static str {
    let ptr = imx_vpu_api_dec_output_code_string(code);
    if ptr.is_null() {
        return "<null>";
    }
    std::ffi::CStr::from_ptr(ptr)
        .to_str()
        .unwrap_or("<invalid utf8>")
}

// ============================================================================
// Public error type
// ============================================================================

/// Errors produced by the imxvpuapi2 decoder.
#[derive(Debug)]
pub enum VpuError {
    /// The VPU hardware does not support decoding.
    NoDecoder,
    /// Could not create the DMA buffer allocator.
    AllocatorCreate(c_int),
    /// A DMA buffer allocation failed.
    DmaAlloc { size: usize, err: c_int },
    /// `imx_vpu_api_dec_open` returned a non-OK code.
    Open(String),
    /// `imx_vpu_api_dec_push_encoded_frame` returned a non-OK code.
    Push(String),
    /// `imx_vpu_api_dec_decode` returned a non-OK code.
    Decode(String),
    /// `imx_vpu_api_dec_get_decoded_frame` returned a non-OK code.
    GetFrame(String),
    /// `imx_vpu_api_dec_add_framebuffers_to_pool` returned a non-OK code.
    AddFramebuffers(String),
    /// The stream info pointer returned by the library was null.
    NullStreamInfo,
    /// The decoded DMA buffer virtual mapping returned null.
    MapFailed(c_int),
}

impl std::fmt::Display for VpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDecoder => write!(f, "VPU hardware does not support decoding"),
            Self::AllocatorCreate(e) => write!(f, "DMA allocator creation failed (errno {e})"),
            Self::DmaAlloc { size, err } => {
                write!(f, "DMA alloc of {size} bytes failed (errno {err})")
            }
            Self::Open(s) => write!(f, "imx_vpu_api_dec_open: {s}"),
            Self::Push(s) => write!(f, "imx_vpu_api_dec_push_encoded_frame: {s}"),
            Self::Decode(s) => write!(f, "imx_vpu_api_dec_decode: {s}"),
            Self::GetFrame(s) => write!(f, "imx_vpu_api_dec_get_decoded_frame: {s}"),
            Self::AddFramebuffers(s) => {
                write!(f, "imx_vpu_api_dec_add_framebuffers_to_pool: {s}")
            }
            Self::NullStreamInfo => write!(f, "imx_vpu_api_dec_get_stream_info returned NULL"),
            Self::MapFailed(e) => write!(f, "imx_dma_buffer_map failed (errno {e})"),
        }
    }
}

// ============================================================================
// Public output type
// ============================================================================

/// A single decoded video frame, ready for colour-space conversion.
///
/// The raw bytes are a copy of the DMA buffer contents.  The Y and interleaved
/// UV data may be padded/strided, so always use the offset and stride fields
/// when accessing pixel data.  Pass this struct to [`nv12_to_egui`].
pub struct DecodedFrame {
    /// Raw bytes copied from the VPU's DMA framebuffer.
    pub data: Vec<u8>,
    /// Width of the displayable picture in pixels (no alignment padding).
    pub actual_width: usize,
    /// Height of the displayable picture in pixels (no alignment padding).
    pub actual_height: usize,
    /// Byte offset within `data` where the Y (luma) plane begins.
    pub y_offset: usize,
    /// Byte offset within `data` where the interleaved UV (chroma) plane begins.
    pub u_offset: usize,
    /// Row stride of the Y plane in bytes (≥ `actual_width`).
    pub y_stride: usize,
    /// Row stride of the UV plane in bytes (≥ `actual_width`).
    pub uv_stride: usize,
}

// ============================================================================
// Public conversion function
// ============================================================================

/// Convert a stride-aware NV12 [`DecodedFrame`] into a flat `Vec<egui::Color32>`.
///
/// Uses BT.601 limited-range coefficients, matching the existing `v4l2m2m`
/// conversion.
pub fn nv12_to_egui(frame: &DecodedFrame) -> Vec<egui::Color32> {
    let w = frame.actual_width;
    let h = frame.actual_height;
    let data = &frame.data;
    let mut pixels = Vec::with_capacity(w * h);

    for row in 0..h {
        for col in 0..w {
            // Y sample: one per pixel
            let y = data[frame.y_offset + row * frame.y_stride + col] as i32;
            // UV samples: one pair per 2×2 pixel block
            let uv_row = row / 2;
            let uv_col = (col / 2) * 2;
            let u =
                data[frame.u_offset + uv_row * frame.uv_stride + uv_col] as i32 - 128;
            let v =
                data[frame.u_offset + uv_row * frame.uv_stride + uv_col + 1] as i32 - 128;

            // BT.601 limited-range YCbCr → RGB
            let r = (y + 1403 * v / 1000).clamp(0, 255) as u8;
            let g = (y - 344 * u / 1000 - 714 * v / 1000).clamp(0, 255) as u8;
            let b = (y + 1770 * u / 1000).clamp(0, 255) as u8;
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }

    pixels
}

// ============================================================================
// VpuDecoder state
// ============================================================================

/// Snapshot of the metrics we care about from `ImxVpuApiDecStreamInfo`.
/// Copied out of the C struct so we don't hold a raw pointer to decoder internals.
#[derive(Clone, Copy)]
struct StreamMetrics {
    actual_width: usize,
    actual_height: usize,
    y_offset: usize,
    u_offset: usize,
    y_stride: usize,
    uv_stride: usize,
    /// Minimum size (bytes) for each DMA buffer added to the framebuffer pool.
    min_fb_pool_size: usize,
    /// Required physical-address alignment for framebuffer pool DMA buffers.
    fb_pool_alignment: usize,
    /// Minimum number of framebuffers the decoder needs in its pool.
    min_num_fb: usize,
    /// Minimum size (bytes) for the separate output DMA buffer.
    /// Only relevant when `frames_from_pool` is `false`.
    min_output_size: usize,
    /// Required physical-address alignment for the output DMA buffer.
    output_alignment: usize,
}

/// Safe wrapper around an `ImxVpuApiDecoder` + its supporting resources.
///
/// # Thread safety
/// `VpuDecoder` is `Send` (the raw pointers are owned exclusively by this
/// struct and are never shared), but it is not `Sync`.
pub struct VpuDecoder {
    /// The libimxvpuapi2 decoder handle.
    decoder: *mut ImxVpuApiDecoder,
    /// DMA buffer allocator (backed by ION / DMA-heap on i.MX8MP).
    allocator: *mut ImxDmaBufferAllocator,
    /// Ring-buffer used internally by the VPU; may be NULL if the global info
    /// says `min_required_stream_buffer_size == 0`.
    stream_buffer: *mut ImxDmaBuffer,
    /// When true, decoded frames reside in the decoder's own buffer pool and
    /// must be returned with `imx_vpu_api_dec_return_framebuffer_to_decoder`.
    frames_from_pool: bool,
    /// Cached stream metrics; `None` until the first
    /// `NEW_STREAM_INFO_AVAILABLE` output code is received.
    stream_metrics: Option<StreamMetrics>,
    /// DMA buffers that make up the decoder's framebuffer pool.
    /// Each entry is `(*mut ImxDmaBuffer, allocated_size_bytes)`.
    fb_pool: Vec<(*mut ImxDmaBuffer, usize)>,
    /// Separate output DMA buffer used when `frames_from_pool == false`.
    output_dmabuf: *mut ImxDmaBuffer,
    /// Allocated byte-size of `output_dmabuf`; 0 when the buffer is NULL.
    output_dmabuf_size: usize,
    /// Monotonically increasing counter used as a frame context handle.
    frame_counter: usize,
}

// SAFETY: VpuDecoder owns all raw pointers exclusively and is never shared
// across threads simultaneously.
unsafe impl Send for VpuDecoder {}

// ============================================================================
// VpuDecoder – constructor
// ============================================================================

impl VpuDecoder {
    /// Open the i.MX8MP Hantro VPU and prepare it for H.264 decoding.
    ///
    /// Allocates the DMA stream buffer, configures H.264 with frame-reordering
    /// and semi-planar (NV12) output, and opens the decoder.  The framebuffer
    /// pool is populated lazily on the first `NEW_STREAM_INFO_AVAILABLE` event.
    pub fn open() -> Result<Self, VpuError> {
        // ---- Silence the library's own log output; we use our own logger ----
        // IMX_VPU_API_LOG_LEVEL_WARNING = 1
        unsafe { imx_vpu_api_set_logging_threshold(1) };

        // ---- Global decoder capabilities -----------------------------------
        let global_info = unsafe { imx_vpu_api_dec_get_global_info() };
        assert!(!global_info.is_null(), "imx_vpu_api_dec_get_global_info returned NULL");
        let flags = unsafe { (*global_info).flags };

        if flags & IMX_VPU_API_DEC_GLOBAL_INFO_FLAG_HAS_DECODER == 0 {
            return Err(VpuError::NoDecoder);
        }

        let frames_from_pool =
            (flags & IMX_VPU_API_DEC_GLOBAL_INFO_FLAG_DECODED_FRAMES_ARE_FROM_BUFFER_POOL) != 0;

        log::debug!(
            "imxvpuapi2: frames_from_pool={frames_from_pool} \
             min_stream_buf={}",
            unsafe { (*global_info).min_required_stream_buffer_size }
        );

        // ---- DMA allocator -------------------------------------------------
        let mut alloc_err: c_int = 0;
        let allocator = unsafe { imx_dma_buffer_allocator_new(&mut alloc_err) };
        if allocator.is_null() {
            return Err(VpuError::AllocatorCreate(alloc_err));
        }

        // ---- Stream buffer (may be zero-sized) -----------------------------
        let stream_buf_size =
            unsafe { (*global_info).min_required_stream_buffer_size };
        let stream_buf_align =
            unsafe { (*global_info).required_stream_buffer_physaddr_alignment };

        let stream_buffer = if stream_buf_size > 0 {
            let mut err: c_int = 0;
            let buf = unsafe {
                imx_dma_buffer_allocate(allocator, stream_buf_size, stream_buf_align, &mut err)
            };
            if buf.is_null() {
                unsafe { imx_dma_buffer_allocator_destroy(allocator) };
                return Err(VpuError::DmaAlloc { size: stream_buf_size, err });
            }
            buf
        } else {
            std::ptr::null_mut()
        };

        // ---- Open params ---------------------------------------------------
        let mut open_params = ImxVpuApiDecOpenParams {
            compression_format: IMX_VPU_API_COMPRESSION_FORMAT_H264,
            flags: IMX_VPU_API_DEC_OPEN_PARAMS_FLAG_ENABLE_FRAME_REORDERING
                | IMX_VPU_API_DEC_OPEN_PARAMS_FLAG_USE_SEMI_PLANAR_COLOR_FORMAT,
            frame_width: 0,
            frame_height: 0,
            extra_header_data: std::ptr::null(),
            extra_header_data_size: 0,
            suggested_color_format: 0,
            _reserved: [0u8; IMX_VPU_API_RESERVED_SIZE - std::mem::size_of::<u32>()],
        };

        // ---- Open the decoder ----------------------------------------------
        let mut decoder: *mut ImxVpuApiDecoder = std::ptr::null_mut();
        let ret = unsafe {
            imx_vpu_api_dec_open(&mut decoder, &mut open_params, stream_buffer)
        };

        if ret != IMX_VPU_API_DEC_RETURN_CODE_OK {
            let msg = unsafe { ret_code_str(ret) }.to_owned();
            if !stream_buffer.is_null() {
                unsafe { imx_dma_buffer_deallocate(stream_buffer) };
            }
            unsafe { imx_dma_buffer_allocator_destroy(allocator) };
            return Err(VpuError::Open(msg));
        }

        log::info!("imxvpuapi2: decoder opened (frames_from_pool={frames_from_pool})");

        Ok(Self {
            decoder,
            allocator,
            stream_buffer,
            frames_from_pool,
            stream_metrics: None,
            fb_pool: Vec::new(),
            output_dmabuf: std::ptr::null_mut(),
            output_dmabuf_size: 0,
            frame_counter: 0,
        })
    }
}

// ============================================================================
// VpuDecoder – public decode interface
// ============================================================================

impl VpuDecoder {
    /// Push one H.264 NAL unit (byte-stream format, with start code) into the
    /// decoder and run the decode state machine until it requests more input.
    ///
    /// Returns zero or more fully decoded frames.  Each frame must be converted
    /// to `egui::ColorImage` by calling [`nv12_to_egui`].
    pub fn push_nal(&mut self, nal: &[u8]) -> Result<Vec<DecodedFrame>, VpuError> {
        let mut frames = Vec::new();

        // ---- Push encoded data ---------------------------------------------
        let mut encoded = ImxVpuApiEncodedFrame {
            data: nal.as_ptr() as *mut u8,
            data_size: nal.len(),
            has_header: 0,
            frame_type: 0,
            context: self.frame_counter as *mut c_void,
            pts: 0,
            dts: 0,
        };
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let ret = unsafe {
            imx_vpu_api_dec_push_encoded_frame(self.decoder, &mut encoded)
        };
        if ret != IMX_VPU_API_DEC_RETURN_CODE_OK {
            let msg = unsafe { ret_code_str(ret) }.to_owned();
            return Err(VpuError::Push(msg));
        }

        // If we already have stream info and are NOT using the pool, set the
        // output buffer before the first decode call.
        if !self.frames_from_pool {
            self.maybe_set_output_buffer();
        }

        // ---- Decode loop ---------------------------------------------------
        loop {
            let mut output_code: u32 = 0;
            let ret =
                unsafe { imx_vpu_api_dec_decode(self.decoder, &mut output_code) };

            if ret != IMX_VPU_API_DEC_RETURN_CODE_OK {
                let msg = unsafe { ret_code_str(ret) }.to_owned();
                return Err(VpuError::Decode(msg));
            }

            log::trace!(
                "imxvpuapi2: output_code={}",
                unsafe { output_code_str(output_code) }
            );

            match output_code {
                IMX_VPU_API_DEC_OUTPUT_CODE_NO_OUTPUT_YET_AVAILABLE => {
                    // Continue spinning – the decoder needs another step.
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_MORE_INPUT_DATA_NEEDED => {
                    // The decoder consumed the data we pushed; need the next NAL.
                    break;
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_EOS => {
                    log::debug!("imxvpuapi2: EOS reported");
                    break;
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_NEW_STREAM_INFO_AVAILABLE => {
                    self.handle_new_stream_info()?;
                    // After allocating pool buffers, tell the decoder about the
                    // output buffer if we're not using the pool.
                    if !self.frames_from_pool {
                        self.maybe_set_output_buffer();
                    }
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_NEED_ADDITIONAL_FRAMEBUFFER => {
                    self.add_framebuffers(1)?;
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_DECODED_FRAME_AVAILABLE => {
                    match self.retrieve_decoded_frame() {
                        Ok(frame) => frames.push(frame),
                        Err(e) => {
                            log::error!("imxvpuapi2: retrieve_decoded_frame: {e}");
                        }
                    }
                    // After retrieval, re-arm the output buffer for the next frame.
                    if !self.frames_from_pool {
                        self.maybe_set_output_buffer();
                    }
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_FRAME_SKIPPED => {
                    log::trace!("imxvpuapi2: frame skipped by decoder");
                }

                IMX_VPU_API_DEC_OUTPUT_CODE_VIDEO_PARAMETERS_CHANGED => {
                    // Stream parameters changed mid-stream (e.g. resolution switch).
                    // We flush and let the next push_nal restart the stream.
                    log::warn!(
                        "imxvpuapi2: video parameters changed – flushing decoder"
                    );
                    unsafe { imx_vpu_api_dec_flush(self.decoder) };
                    self.free_fb_pool();
                    self.stream_metrics = None;
                    break;
                }

                unknown => {
                    log::warn!("imxvpuapi2: unknown output code {unknown}");
                    break;
                }
            }
        }

        Ok(frames)
    }
}

// ============================================================================
// VpuDecoder – private helpers
// ============================================================================

impl VpuDecoder {
    /// Called when the decoder signals `NEW_STREAM_INFO_AVAILABLE`.
    ///
    /// 1. Reads stream info from the decoder.
    /// 2. Frees any previously allocated pool buffers (stream param change).
    /// 3. Allocates the required number of pool framebuffers and registers them.
    /// 4. If `frames_from_pool` is false, allocates the separate output buffer.
    fn handle_new_stream_info(&mut self) -> Result<(), VpuError> {
        let raw = unsafe { imx_vpu_api_dec_get_stream_info(self.decoder) };
        if raw.is_null() {
            return Err(VpuError::NullStreamInfo);
        }

        // SAFETY: the pointer is valid until the next decode call that produces
        // NEW_STREAM_INFO_AVAILABLE; we copy everything we need immediately.
        let info = unsafe { &*raw };
        let m = &info.decoded_frame_framebuffer_metrics;

        let metrics = StreamMetrics {
            actual_width: m.actual_frame_width,
            actual_height: m.actual_frame_height,
            y_offset: m.y_offset,
            u_offset: m.u_offset,
            y_stride: m.y_stride,
            uv_stride: m.uv_stride,
            min_fb_pool_size: info.min_fb_pool_framebuffer_size,
            fb_pool_alignment: info.fb_pool_framebuffer_alignment,
            min_num_fb: info.min_num_required_framebuffers,
            min_output_size: info.min_output_framebuffer_size,
            output_alignment: info.output_framebuffer_alignment,
        };

        log::info!(
            "imxvpuapi2: new stream info – {}×{} y_stride={} uv_stride={} \
             y_off={} u_off={} pool_fb_size={} min_fb={}",
            metrics.actual_width,
            metrics.actual_height,
            metrics.y_stride,
            metrics.uv_stride,
            metrics.y_offset,
            metrics.u_offset,
            metrics.min_fb_pool_size,
            metrics.min_num_fb,
        );

        // Free any stale pool from a previous stream (e.g. resolution change).
        self.free_fb_pool();

        self.stream_metrics = Some(metrics);

        // Allocate and register framebuffer pool entries.
        if metrics.min_num_fb > 0 {
            self.add_framebuffers(metrics.min_num_fb)?;
        }

        // If decoded frames land in a separate output buffer, allocate it now.
        if !self.frames_from_pool {
            self.allocate_output_buffer()?;
        }

        Ok(())
    }

    /// Allocate `count` new DMA buffers and add them to the decoder's pool.
    fn add_framebuffers(&mut self, count: usize) -> Result<(), VpuError> {
        let metrics = match &self.stream_metrics {
            Some(m) => *m,
            None => {
                log::error!("imxvpuapi2: add_framebuffers called before stream info");
                return Ok(());
            }
        };

        let size = metrics.min_fb_pool_size;
        let align = metrics.fb_pool_alignment;

        let mut new_bufs: Vec<*mut ImxDmaBuffer> = Vec::with_capacity(count);
        for _ in 0..count {
            let mut err: c_int = 0;
            let buf = unsafe {
                imx_dma_buffer_allocate(self.allocator, size, align, &mut err)
            };
            if buf.is_null() {
                // Clean up the ones we already allocated this round.
                for b in &new_bufs {
                    unsafe { imx_dma_buffer_deallocate(*b) };
                }
                return Err(VpuError::DmaAlloc { size, err });
            }
            new_bufs.push(buf);
        }

        // Register with the decoder (fb_contexts may be NULL).
        let ret = unsafe {
            imx_vpu_api_dec_add_framebuffers_to_pool(
                self.decoder,
                new_bufs.as_mut_ptr(),
                std::ptr::null_mut(),
                count,
            )
        };

        if ret != IMX_VPU_API_DEC_RETURN_CODE_OK {
            let msg = unsafe { ret_code_str(ret) }.to_owned();
            for b in &new_bufs {
                unsafe { imx_dma_buffer_deallocate(*b) };
            }
            return Err(VpuError::AddFramebuffers(msg));
        }

        // Take ownership of the new buffers.
        for b in new_bufs {
            self.fb_pool.push((b, size));
        }

        log::debug!(
            "imxvpuapi2: fb pool now has {} buffer(s) (added {count})",
            self.fb_pool.len()
        );

        Ok(())
    }

    /// Allocate (or reallocate) the single output DMA buffer used when
    /// `frames_from_pool` is `false`.
    fn allocate_output_buffer(&mut self) -> Result<(), VpuError> {
        let metrics = match &self.stream_metrics {
            Some(m) => *m,
            None => return Ok(()),
        };

        // Free the old output buffer if present.
        if !self.output_dmabuf.is_null() {
            unsafe { imx_dma_buffer_deallocate(self.output_dmabuf) };
            self.output_dmabuf = std::ptr::null_mut();
            self.output_dmabuf_size = 0;
        }

        let size = metrics.min_output_size;
        let align = metrics.output_alignment;
        let mut err: c_int = 0;
        let buf = unsafe {
            imx_dma_buffer_allocate(self.allocator, size, align, &mut err)
        };
        if buf.is_null() {
            return Err(VpuError::DmaAlloc { size, err });
        }

        self.output_dmabuf = buf;
        self.output_dmabuf_size = size;
        Ok(())
    }

    /// Call `imx_vpu_api_dec_set_output_frame_dma_buffer` with the current
    /// output buffer.  A no-op if the buffer has not been allocated yet.
    fn maybe_set_output_buffer(&self) {
        if !self.output_dmabuf.is_null() {
            unsafe {
                imx_vpu_api_dec_set_output_frame_dma_buffer(
                    self.decoder,
                    self.output_dmabuf,
                    std::ptr::null_mut(),
                );
            }
        }
    }

    /// Retrieve one decoded frame from the VPU, copy its pixel data out of the
    /// DMA buffer, and (when using the pool) immediately return the buffer so
    /// the decoder can reuse it.
    fn retrieve_decoded_frame(&mut self) -> Result<DecodedFrame, VpuError> {
        let mut raw_frame = ImxVpuApiRawFrame::default();
        let ret = unsafe {
            imx_vpu_api_dec_get_decoded_frame(self.decoder, &mut raw_frame)
        };
        if ret != IMX_VPU_API_DEC_RETURN_CODE_OK {
            let msg = unsafe { ret_code_str(ret) }.to_owned();
            return Err(VpuError::GetFrame(msg));
        }

        let metrics = self.stream_metrics.expect(
            "stream metrics must be set before DECODED_FRAME_AVAILABLE is emitted",
        );

        // Map the DMA buffer for CPU read access.
        let dmabuf = raw_frame.fb_dma_buffer;
        let mut map_err: c_int = 0;
        let vaddr = unsafe {
            imx_dma_buffer_map(dmabuf, IMX_DMA_BUFFER_MAPPING_FLAG_READ, &mut map_err)
        };
        if vaddr.is_null() {
            // Return the buffer to the pool before bailing out.
            if self.frames_from_pool {
                unsafe { imx_vpu_api_dec_return_framebuffer_to_decoder(self.decoder, dmabuf) };
            }
            return Err(VpuError::MapFailed(map_err));
        }

        // The total byte-span we need covers from the start of the DMA buffer
        // up to the end of the UV plane.  We compute a conservative upper bound
        // using the pool framebuffer size so we never read out-of-bounds.
        let copy_size = if self.frames_from_pool {
            metrics.min_fb_pool_size
        } else {
            metrics.min_output_size
        };

        // SAFETY: `vaddr` is a valid mapping of `copy_size` bytes, kept alive
        // until `imx_dma_buffer_unmap`.
        let data = unsafe { std::slice::from_raw_parts(vaddr, copy_size) }.to_vec();

        unsafe { imx_dma_buffer_unmap(dmabuf) };

        // Return the buffer to the pool so the VPU can decode into it again.
        if self.frames_from_pool {
            unsafe {
                imx_vpu_api_dec_return_framebuffer_to_decoder(self.decoder, dmabuf)
            };
        }

        Ok(DecodedFrame {
            data,
            actual_width: metrics.actual_width,
            actual_height: metrics.actual_height,
            y_offset: metrics.y_offset,
            u_offset: metrics.u_offset,
            y_stride: metrics.y_stride,
            uv_stride: metrics.uv_stride,
        })
    }

    /// Deallocate all framebuffers in the pool.
    ///
    /// The decoder **must** have been closed or flushed before calling this,
    /// otherwise the VPU might still be writing into the buffers.
    fn free_fb_pool(&mut self) {
        for (buf, _size) in self.fb_pool.drain(..) {
            unsafe { imx_dma_buffer_deallocate(buf) };
        }
    }
}

// ============================================================================
// VpuDecoder – Drop
// ============================================================================

impl Drop for VpuDecoder {
    fn drop(&mut self) {
        // 1. Close the decoder first so it stops using all DMA buffers.
        if !self.decoder.is_null() {
            unsafe { imx_vpu_api_dec_close(self.decoder) };
            self.decoder = std::ptr::null_mut();
        }

        // 2. Free the framebuffer pool (decoder is closed so it's safe).
        self.free_fb_pool();

        // 3. Free the separate output buffer (if any).
        if !self.output_dmabuf.is_null() {
            unsafe { imx_dma_buffer_deallocate(self.output_dmabuf) };
            self.output_dmabuf = std::ptr::null_mut();
        }

        // 4. Free the stream ring-buffer.
        if !self.stream_buffer.is_null() {
            unsafe { imx_dma_buffer_deallocate(self.stream_buffer) };
            self.stream_buffer = std::ptr::null_mut();
        }

        // 5. Destroy the allocator.
        if !self.allocator.is_null() {
            unsafe { imx_dma_buffer_allocator_destroy(self.allocator) };
            self.allocator = std::ptr::null_mut();
        }
    }
}
