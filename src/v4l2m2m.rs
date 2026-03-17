//! Direct V4L2 M2M H.264 decoder for i.MX8MP VPU.
//!
//! # How V4L2 M2M works
//!
//! The kernel exposes a single `/dev/videoN` device that acts as both a sink
//! (compressed input) and a source (raw output).  The flow is:
//!
//! ```text
//!   Your code                         Kernel / VPU
//!   ─────────────────────────────────────────────────
//!   VIDIOC_S_FMT  OUTPUT  H264       configure input format
//!   VIDIOC_S_FMT  CAPTURE NV12       configure output format
//!   VIDIOC_REQBUFS OUTPUT  N          allocate input buffers
//!   VIDIOC_REQBUFS CAPTURE N          allocate output buffers
//!   VIDIOC_QUERYBUF + mmap            map buffers into user space
//!   VIDIOC_STREAMON OUTPUT            start input queue
//!   VIDIOC_STREAMON CAPTURE           start output queue
//!
//!   loop:
//!     copy NAL data → input buffer
//!     VIDIOC_QBUF  OUTPUT             enqueue filled input buffer
//!     VIDIOC_QBUF  CAPTURE            enqueue empty output buffer
//!     VIDIOC_DQBUF CAPTURE  (poll)    dequeue decoded NV12 frame
//!     NV12 → RGB conversion
//!     VIDIOC_DQBUF OUTPUT             reclaim input buffer
//! ```

use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::io::AsRawFd,
    path::Path,
};

use nix::errno::Errno;

// ============================================================================
// V4L2 constants and structures (subset needed for M2M decoding)
// ============================================================================

// ioctl numbers — architecture-specific but stable on ARM/x86
const VIDIOC_QUERYCAP:   u64 = 0x8068_5600;
const VIDIOC_S_FMT:      u64 = 0xC0D0_5605;
const VIDIOC_G_FMT:      u64 = 0xC0D0_5604;
const VIDIOC_REQBUFS:    u64 = 0xC014_5608;
const VIDIOC_QUERYBUF:   u64 = 0xC058_5609;
const VIDIOC_QBUF:       u64 = 0xC058_560F;
const VIDIOC_DQBUF:      u64 = 0xC058_5611;
const VIDIOC_STREAMON:   u64 = 0x4004_5612;
const VIDIOC_STREAMOFF:  u64 = 0x4004_5613;
const VIDIOC_SUBSCRIBE_EVENT: u64 = 0x4020_5690;
const VIDIOC_DQEVENT:    u64 = 0x8070_5659;
const VIDIOC_DECODER_CMD: u64 = 0xC028_56A8;

// Buffer types
const V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE:  u32 = 10;
const V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE: u32 = 9;

// Memory type
const V4L2_MEMORY_MMAP: u32 = 1;

// Pixel formats (fourcc)
const V4L2_PIX_FMT_H264: u32 = fourcc(b'H', b'2', b'6', b'4');
const V4L2_PIX_FMT_NV12: u32 = fourcc(b'N', b'V', b'1', b'2');

// Capabilities
const V4L2_CAP_VIDEO_M2M_MPLANE: u32 = 0x0000_4000;
const V4L2_CAP_STREAMING:        u32 = 0x0400_0000;

// Buffer flags
const V4L2_BUF_FLAG_LAST:      u32 = 0x0010_0000;
const V4L2_BUF_FLAG_KEYFRAME:  u32 = 0x0000_0008;

// Events
const V4L2_EVENT_SOURCE_CHANGE: u32 = 5;
const V4L2_EVENT_EOS:           u32 = 2;

// Decoder commands
const V4L2_DEC_CMD_STOP:  u32 = 1;
const V4L2_DEC_CMD_START: u32 = 0;

const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

// Number of buffers to allocate on each side.
// More output buffers = more pipelining but more memory.
const NUM_INPUT_BUFS:  usize = 4;
const NUM_OUTPUT_BUFS: usize = 8;

// ============================================================================
// Bindgen-equivalent structs (manually matched to kernel ABI)
// ============================================================================

#[repr(C)]
#[derive(Default)]
struct V4l2Capability {
    driver:       [u8; 16],
    card:         [u8; 32],
    bus_info:     [u8; 32],
    version:      u32,
    capabilities: u32,
    device_caps:  u32,
    reserved:     [u32; 3],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct V4l2PlanePixFormat {
    sizeimage:    u32,
    bytesperline: u32,
    reserved:     [u16; 6],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct V4l2PixFormatMplane {
    width:        u32,
    height:       u32,
    pixelformat:  u32,
    field:        u32,
    colorspace:   u32,
    plane_fmt:    [V4l2PlanePixFormat; 8],
    num_planes:   u8,
    flags:        u8,
    // ycbcr_enc / quantization / xfer_func packed in a union; just pad it
    _enc_quant:   u16,
    reserved:     [u8; 7],
}

#[repr(C)]
union V4l2FmtUnion {
    pix_mp: V4l2PixFormatMplane,
    raw:    [u8; 200],
}

#[repr(C)]
struct V4l2Format {
    buf_type: u32,
    fmt:      V4l2FmtUnion,
}

#[repr(C)]
#[derive(Default)]
struct V4l2RequestBuffers {
    count:    u32,
    buf_type: u32,
    memory:   u32,
    capabilities: u32,
    reserved: [u32; 1],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct V4l2Plane {
    bytesused:   u32,
    length:      u32,
    // union: mem_offset / userptr / fd
    m_mem_offset: u32,
    _m_pad:      [u32; 2],
    data_offset: u32,
    reserved:    [u32; 11],
}

#[repr(C)]
#[derive(Default)]
struct V4l2Buffer {
    index:     u32,
    buf_type:  u32,
    bytesused: u32,
    flags:     u32,
    field:     u32,
    // struct timeval (2 × i64 on 64-bit)
    timestamp: [i64; 2],
    // struct v4l2_timecode
    timecode:  [u32; 5],
    sequence:  u32,
    memory:    u32,
    // union m: for mplane, points to planes array via userspace ptr
    m_planes_ptr: u64,
    length:    u32,
    reserved2: u32,
    // union: request_fd / reserved
    reserved:  u32,
}

#[repr(C)]
#[derive(Default)]
struct V4l2EventSubscription {
    event_type: u32,
    id:         u32,
    flags:      u32,
    reserved:   [u32; 5],
}

#[repr(C)]
union V4l2EventUnion {
    src_change: V4l2EventSrcChange,
    raw: [u8; 64],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct V4l2EventSrcChange {
    changes: u32,
}

#[repr(C)]
struct V4l2Event {
    event_type: u32,
    u:          V4l2EventUnion,
    pending:    u32,
    sequence:   u32,
    timestamp:  [i64; 2],
    id:         u32,
    reserved:   [u32; 8],
}

#[repr(C)]
#[derive(Default)]
struct V4l2DecoderCmd {
    cmd:   u32,
    flags: u32,
    data:  [u64; 4],
}

// ============================================================================
// Mapped buffer
// ============================================================================

struct MmapBuffer {
    /// Userspace address from mmap
    addr: *mut u8,
    /// Length passed to mmap / used for munmap
    len:  usize,
}

impl MmapBuffer {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.addr, self.len) }
    }
    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.addr, self.len) }
    }
}

impl Drop for MmapBuffer {
    fn drop(&mut self) {
        if !self.addr.is_null() {
            unsafe { libc::munmap(self.addr as *mut libc::c_void, self.len); }
        }
    }
}

// SAFETY: we never share the pointer across threads without synchronisation.
unsafe impl Send for MmapBuffer {}

// ============================================================================
// Error type
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum VpuError {
    #[error("open device: {0}")]
    Open(io::Error),
    #[error("ioctl {name}: {err}")]
    Ioctl { name: &'static str, err: Errno },
    #[error("mmap: {0}")]
    Mmap(nix::Error),
    #[error("device is not a V4L2 M2M device")]
    NotM2m,
    #[error("poll: {0}")]
    Poll(io::Error),
    #[error("EOS")]
    Eos,
}

macro_rules! ioctl {
    ($fd:expr, $nr:expr, $name:literal, $arg:expr) => {{
        let ret = unsafe { libc::ioctl($fd, $nr as libc::c_ulong, $arg) };
        if ret < 0 {
            return Err(VpuError::Ioctl {
                name: $name,
                err:  Errno::last(),
            });
        }
        ret
    }};
}

// ============================================================================
// Vpu decoder
// ============================================================================

/// Direct V4L2 M2M decoder targeting the i.MX8MP VPU.
///
/// # Buffer lifecycle
///
/// ```text
///  INPUT  (OUTPUT buf type):   user fills → QBUF → kernel decodes → DQBUF → user refills
///  OUTPUT (CAPTURE buf type):  user enqueues empty → QBUF → kernel fills → DQBUF → user reads
/// ```
/// State of the capture (output) side of the decoder.
#[derive(PartialEq)]
enum CaptureState {
    /// Waiting for SOURCE_CHANGE event — capture buffers not allocated yet.
    Uninitialized,
    /// Capture buffers allocated and streaming.
    Ready,
}

pub struct VpuDecoder {
    fd:           std::os::unix::io::RawFd,
    _file:        File,   // keeps fd alive

    // Mmap'd input (compressed) buffers — allocated in open()
    input_bufs:   Vec<MmapBuffer>,
    input_sizes:  Vec<usize>,

    // Mmap'd output (raw NV12) buffers — allocated after SOURCE_CHANGE
    output_bufs:  Vec<Vec<MmapBuffer>>, // [buf_index][plane_index]
    output_sizes: Vec<Vec<usize>>,

    // Dimensions — valid after SOURCE_CHANGE
    pub width:    u32,
    pub height:   u32,

    // Tracks which input buffers are currently free
    free_input:   Vec<usize>,

    capture_state: CaptureState,
    flushing:      bool,
}

impl VpuDecoder {
    /// Open the VPU H.264 decoder.
    ///
    /// Only the input (OUTPUT) side is set up here.  The capture (CAPTURE)
    /// side cannot be configured until the VPU has parsed the SPS from the
    /// bitstream and fired a V4L2_EVENT_SOURCE_CHANGE, which happens inside
    /// the first few calls to push_nal().
    ///
    /// On i.MX8MP the H.264 node is `/dev/video0` by default.
    pub fn open(device: &str) -> Result<Self, VpuError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(device)
            .map_err(VpuError::Open)?;

        let fd = file.as_raw_fd();

        // Verify it is a M2M streaming device
        let mut cap = V4l2Capability::default();
        ioctl!(fd, VIDIOC_QUERYCAP, "QUERYCAP", &mut cap);
        let caps = cap.capabilities;
        if caps & V4L2_CAP_VIDEO_M2M_MPLANE == 0 || caps & V4L2_CAP_STREAMING == 0 {
            return Err(VpuError::NotM2m);
        }

        let mut dec = Self {
            fd,
            _file:         file,
            input_bufs:    Vec::new(),
            input_sizes:   Vec::new(),
            output_bufs:   Vec::new(),
            output_sizes:  Vec::new(),
            width:          0,
            height:         0,
            free_input:    Vec::new(),
            capture_state: CaptureState::Uninitialized,
            flushing:      false,
        };

        // Step 1: set H.264 input format
        dec.set_input_format()?;

        // Step 2: allocate + mmap input buffers
        dec.alloc_input_buffers()?;

        // Step 3: subscribe to SOURCE_CHANGE and EOS events
        dec.subscribe_events()?;

        // Step 4: start the input queue — we can send NALs now
        let mut t: u32 = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
        ioctl!(dec.fd, VIDIOC_STREAMON, "STREAMON(INPUT)", &mut t);

        log::debug!("VpuDecoder: input queue running, waiting for SOURCE_CHANGE");

        Ok(dec)
    }

    // ------------------------------------------------------------------
    // Initialisation helpers
    // ------------------------------------------------------------------

    fn set_input_format(&self) -> Result<(), VpuError> {
        let mut fmt = V4l2Format {
            buf_type: V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            fmt: V4l2FmtUnion { raw: [0u8; 200] },
        };
        unsafe {
            fmt.fmt.pix_mp.pixelformat                = V4L2_PIX_FMT_H264;
            fmt.fmt.pix_mp.width                      = 0; // VPU reads from SPS
            fmt.fmt.pix_mp.height                     = 0;
            fmt.fmt.pix_mp.num_planes                 = 1;
            fmt.fmt.pix_mp.plane_fmt[0].sizeimage     = 1 << 20; // 1 MiB per input buf
        }
        ioctl!(self.fd, VIDIOC_S_FMT, "S_FMT(INPUT)", &mut fmt);
        Ok(())
    }

    fn alloc_input_buffers(&mut self) -> Result<(), VpuError> {
        let mut req = V4l2RequestBuffers {
            count:    NUM_INPUT_BUFS as u32,
            buf_type: V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            memory:   V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_REQBUFS, "REQBUFS(INPUT)", &mut req);

        for i in 0..req.count as usize {
            let plane = self.querybuf_plane(V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE, i, 0)?;
            let buf   = self.mmap_plane(plane.length as usize, plane.m_mem_offset)?;
            self.input_sizes.push(plane.length as usize);
            self.input_bufs.push(buf);
            self.free_input.push(i);
        }
        Ok(())
    }

    /// Called when V4L2_EVENT_SOURCE_CHANGE is received.
    /// Reads the negotiated dimensions, allocates capture buffers, and starts
    /// the capture queue.  Safe to call multiple times (e.g. mid-stream
    /// resolution change).
    fn handle_source_change(&mut self) -> Result<(), VpuError> {
        log::info!("VpuDecoder: SOURCE_CHANGE — (re)initialising capture side");

        // If we had capture buffers from a previous resolution, free them
        if self.capture_state == CaptureState::Ready {
            let mut t: u32 = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
            unsafe { libc::ioctl(self.fd, VIDIOC_STREAMOFF as libc::c_ulong, &mut t); }
            let mut req = V4l2RequestBuffers {
                count:    0, // free all
                buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                memory:   V4L2_MEMORY_MMAP,
                ..Default::default()
            };
            unsafe { libc::ioctl(self.fd, VIDIOC_REQBUFS as libc::c_ulong, &mut req); }
            self.output_bufs.clear();
            self.output_sizes.clear();
        }

        // Read dimensions negotiated by the VPU from the SPS
        let mut fmt = V4l2Format {
            buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            fmt: V4l2FmtUnion { raw: [0u8; 200] },
        };
        ioctl!(self.fd, VIDIOC_G_FMT, "G_FMT(CAPTURE)", &mut fmt);
        unsafe {
            self.width  = fmt.fmt.pix_mp.width;
            self.height = fmt.fmt.pix_mp.height;
            // Ensure NV12
            fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_NV12;
            fmt.fmt.pix_mp.num_planes  = 1;
        }
        ioctl!(self.fd, VIDIOC_S_FMT, "S_FMT(CAPTURE)", &mut fmt);
        // Re-read after S_FMT in case the driver adjusted the size
        ioctl!(self.fd, VIDIOC_G_FMT, "G_FMT(CAPTURE)", &mut fmt);
        unsafe {
            self.width  = fmt.fmt.pix_mp.width;
            self.height = fmt.fmt.pix_mp.height;
        }
        log::info!("VpuDecoder: stream is {}x{}", self.width, self.height);

        // Allocate capture buffers now that we know the frame size
        let mut req = V4l2RequestBuffers {
            count:    NUM_OUTPUT_BUFS as u32,
            buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            memory:   V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_REQBUFS, "REQBUFS(CAPTURE)", &mut req);

        for i in 0..req.count as usize {
            let plane = self.querybuf_plane(V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE, i, 0)?;
            let buf   = self.mmap_plane(plane.length as usize, plane.m_mem_offset)?;
            self.output_sizes.push(vec![plane.length as usize]);
            self.output_bufs.push(vec![buf]);
        }

        // Start the capture queue
        let mut t: u32 = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
        ioctl!(self.fd, VIDIOC_STREAMON, "STREAMON(CAPTURE)", &mut t);

        // Pre-enqueue all capture buffers
        for i in 0..self.output_bufs.len() {
            self.qbuf_capture(i, 0)?;
        }

        self.capture_state = CaptureState::Ready;
        Ok(())
    }

    fn subscribe_events(&self) -> Result<(), VpuError> {
        for event_type in [V4L2_EVENT_SOURCE_CHANGE, V4L2_EVENT_EOS] {
            let mut sub = V4l2EventSubscription {
                event_type,
                ..Default::default()
            };
            ioctl!(self.fd, VIDIOC_SUBSCRIBE_EVENT, "SUBSCRIBE_EVENT", &mut sub);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Push one NAL unit and return all decoded NV12 frames that are ready.
    ///
    /// Each returned frame is `(NV12 data, width, height)`.
    /// Convert to RGB with [`nv12_to_rgb`].
    pub fn push_nal(&mut self, nal: &[u8]) -> Result<Vec<(Vec<u8>, u32, u32)>, VpuError> {
        self.queue_input(nal)?;
        // Process any pending events (SOURCE_CHANGE, EOS) before draining frames.
        // SOURCE_CHANGE must be handled before trying to dequeue capture buffers.
        self.drain_events()?;
        if self.capture_state == CaptureState::Uninitialized {
            // SOURCE_CHANGE not yet received — no frames can be ready yet
            return Ok(Vec::new());
        }
        self.drain_output()
    }

    /// Drain all pending V4L2 events, acting on SOURCE_CHANGE.
    fn drain_events(&mut self) -> Result<(), VpuError> {
        loop {
            let mut event: V4l2Event = unsafe { std::mem::zeroed() };
            let ret = unsafe {
                libc::ioctl(self.fd, VIDIOC_DQEVENT as libc::c_ulong, &mut event)
            };
            if ret < 0 {
                // ENOENT means no more events — normal exit
                if Errno::last() == Errno::ENOENT { break; }
                // Any other error is unexpected but non-fatal; stop draining
                break;
            }
            match event.event_type {
                V4L2_EVENT_SOURCE_CHANGE => {
                    self.handle_source_change()?;
                }
                V4L2_EVENT_EOS => {
                    log::info!("VpuDecoder: EOS event");
                    self.flushing = true;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Signal end-of-stream and drain remaining frames.
    pub fn flush(&mut self) -> Result<Vec<(Vec<u8>, u32, u32)>, VpuError> {
        if !self.flushing {
            self.flushing = true;
            let mut cmd = V4l2DecoderCmd {
                cmd:   V4L2_DEC_CMD_STOP,
                ..Default::default()
            };
            ioctl!(self.fd, VIDIOC_DECODER_CMD, "DECODER_CMD(STOP)", &mut cmd);
        }
        self.drain_output()
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn queue_input(&mut self, data: &[u8]) -> Result<(), VpuError> {
        // Wait for a free input buffer (busy-poll with a limit)
        if self.free_input.is_empty() {
            self.reclaim_input()?;
        }

        let idx = self.free_input.pop()
            .expect("no free input buffers after reclaim");

        let buf = &mut self.input_bufs[idx];
        let len = data.len().min(self.input_sizes[idx]);
        buf.as_slice_mut()[..len].copy_from_slice(&data[..len]);

        self.qbuf_output(idx, len)?;
        Ok(())
    }

    fn reclaim_input(&mut self) -> Result<(), VpuError> {
        let mut plane = V4l2Plane::default();
        let mut buf = V4l2Buffer {
            buf_type: V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            memory:   V4L2_MEMORY_MMAP,
            m_planes_ptr: &mut plane as *mut V4l2Plane as u64,
            length:   1,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_DQBUF, "DQBUF(INPUT)", &mut buf);
        self.free_input.push(buf.index as usize);
        Ok(())
    }

    fn drain_output(&mut self) -> Result<Vec<(Vec<u8>, u32, u32)>, VpuError> {
        let mut out = Vec::new();

        loop {
            // Non-blocking poll: check if a capture buffer is ready
            if !self.poll_capture(0)? {
                break;
            }

            let mut plane = V4l2Plane::default();
            let mut buf = V4l2Buffer {
                buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                memory:   V4L2_MEMORY_MMAP,
                m_planes_ptr: &mut plane as *mut V4l2Plane as u64,
                length:   1,
                ..Default::default()
            };

            let ret = unsafe {
                libc::ioctl(self.fd, VIDIOC_DQBUF as libc::c_ulong, &mut buf)
            };
            if ret < 0 {
                let e = Errno::last();
                if e == Errno::EAGAIN { break; }
                return Err(VpuError::Ioctl { name: "DQBUF(CAPTURE)", err: e });
            }

            let idx       = buf.index as usize;
            let bytesused = plane.bytesused as usize;
            let is_last   = buf.flags & V4L2_BUF_FLAG_LAST != 0;

            // Copy NV12 data out before re-queuing the buffer
            let frame_data = self.output_bufs[idx][0].as_slice()[..bytesused].to_vec();
            out.push((frame_data, self.width, self.height));

            // Re-enqueue the output buffer for the VPU to fill again
            self.qbuf_capture(idx, 0)?;

            if is_last { break; }
        }

        Ok(out)
    }

    fn qbuf_output(&self, index: usize, bytesused: usize) -> Result<(), VpuError> {
        let mut plane = V4l2Plane {
            bytesused: bytesused as u32,
            length:    self.input_sizes[index] as u32,
            ..Default::default()
        };
        let mut buf = V4l2Buffer {
            index:    index as u32,
            buf_type: V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            memory:   V4L2_MEMORY_MMAP,
            m_planes_ptr: &mut plane as *mut V4l2Plane as u64,
            length:   1,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_QBUF, "QBUF(OUTPUT)", &mut buf);
        Ok(())
    }

    fn qbuf_capture(&self, index: usize, plane_idx: usize) -> Result<(), VpuError> {
        let mut plane = V4l2Plane {
            length: self.output_sizes[index][plane_idx] as u32,
            ..Default::default()
        };
        let mut buf = V4l2Buffer {
            index:    index as u32,
            buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            memory:   V4L2_MEMORY_MMAP,
            m_planes_ptr: &mut plane as *mut V4l2Plane as u64,
            length:   1,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_QBUF, "QBUF(CAPTURE)", &mut buf);
        Ok(())
    }

    fn querybuf_plane(
        &self,
        buf_type: u32,
        index:    usize,
        plane:    usize,
    ) -> Result<V4l2Plane, VpuError> {
        let mut pl = V4l2Plane::default();
        let mut buf = V4l2Buffer {
            index:    index as u32,
            buf_type,
            memory:   V4L2_MEMORY_MMAP,
            m_planes_ptr: &mut pl as *mut V4l2Plane as u64,
            length:   (plane + 1) as u32,
            ..Default::default()
        };
        ioctl!(self.fd, VIDIOC_QUERYBUF, "QUERYBUF", &mut buf);
        Ok(pl)
    }

    fn mmap_plane(&self, length: usize, offset: u32) -> Result<MmapBuffer, VpuError> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.fd,
                offset as libc::off_t,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(VpuError::Mmap(nix::Error::last()));
        }
        Ok(MmapBuffer {
            addr: ptr as *mut u8,
            len:  length,
        })
    }

    fn poll_capture(&self, timeout_ms: i32) -> Result<bool, VpuError> {
        let mut pfd = libc::pollfd {
            fd:      self.fd,
            events:  libc::POLLIN,
            revents: 0,
        };
        let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ret < 0 {
            return Err(VpuError::Poll(io::Error::last_os_error()));
        }
        Ok(ret > 0 && pfd.revents & libc::POLLIN != 0)
    }
}

impl Drop for VpuDecoder {
    fn drop(&mut self) {
        unsafe {
            let mut t: u32 = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
            libc::ioctl(self.fd, VIDIOC_STREAMOFF as libc::c_ulong, &mut t);
            t = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
            libc::ioctl(self.fd, VIDIOC_STREAMOFF as libc::c_ulong, &mut t);
        }
    }
}

// ============================================================================
// NV12 → RGB conversion
// ============================================================================

/// Convert a planar NV12 buffer to packed RGB24.
///
/// NV12 layout:
/// ```text
///   [ Y plane: width × height bytes          ]
///   [ UV plane: width × height/2 bytes       ]  (interleaved U,V)
/// ```
pub fn nv12_to_rgb(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width  as usize;
    let h = height as usize;
    let y_size  = w * h;
    let uv_base = y_size;

    let mut rgb = vec![0u8; w * h * 3];

    for row in 0..h {
        for col in 0..w {
            let y  = data[row * w + col] as i32;
            // UV is subsampled 2×2: both pixels in same pair share one UV
            let uv_row  = row / 2;
            let uv_col  = (col / 2) * 2;
            let u = data[uv_base + uv_row * w + uv_col]     as i32 - 128;
            let v = data[uv_base + uv_row * w + uv_col + 1] as i32 - 128;

            // BT.601 limited range
            let r = (y + 1403 * v / 1000).clamp(0, 255) as u8;
            let g = (y - 344  * u / 1000 - 714 * v / 1000).clamp(0, 255) as u8;
            let b = (y + 1770 * u / 1000).clamp(0, 255) as u8;

            let off = (row * w + col) * 3;
            rgb[off]     = r;
            rgb[off + 1] = g;
            rgb[off + 2] = b;
        }
    }

    rgb
}

/// Convert NV12 directly to a vec of `egui::Color32`.
pub fn nv12_to_egui(data: &[u8], width: u32, height: u32) -> Vec<egui::Color32> {
    let w = width  as usize;
    let h = height as usize;
    let y_size  = w * h;
    let uv_base = y_size;
    let mut pixels = Vec::with_capacity(w * h);

    for row in 0..h {
        for col in 0..w {
            let y  = data[row * w + col] as i32;
            let uv_row = row / 2;
            let uv_col = (col / 2) * 2;
            let u = data[uv_base + uv_row * w + uv_col]     as i32 - 128;
            let v = data[uv_base + uv_row * w + uv_col + 1] as i32 - 128;

            let r = (y + 1403 * v / 1000).clamp(0, 255) as u8;
            let g = (y - 344  * u / 1000 - 714 * v / 1000).clamp(0, 255) as u8;
            let b = (y + 1770 * u / 1000).clamp(0, 255) as u8;

            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }

    pixels
}