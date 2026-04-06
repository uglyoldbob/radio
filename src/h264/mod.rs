use std::sync::mpsc;

#[cfg(feature = "ffmpeg")]
mod ffmpeg;
#[cfg(feature = "imxvpuapi2")]
mod imxvpuapi2;
#[cfg(feature = "v4l2m2m")]
mod v4l2m2m;

/// Internal message to the decode thread
enum DecodeMsg {
    Data(Vec<u8>),
    Shutdown,
}

/// Handle to a background decode thread
struct DecodeThread {
    tx: mpsc::SyncSender<DecodeMsg>,
    rx: mpsc::Receiver<egui::ColorImage>,
    _handle: std::thread::JoinHandle<()>,
}

impl DecodeThread {
    fn spawn(mut decoder: InnerDecoder) -> Self {
        let (tx, msg_rx) = mpsc::sync_channel::<DecodeMsg>(60);
        let (frame_tx, rx) = mpsc::sync_channel::<egui::ColorImage>(2);

        let handle = std::thread::spawn(move || {
            for msg in msg_rx {
                match msg {
                    DecodeMsg::Shutdown => break,
                    DecodeMsg::Data(data) => {
                        for frame in decoder.decode(&data) {
                            let _ = frame_tx.try_send(frame);
                        }
                    }
                }
            }
        });

        Self {
            tx,
            rx,
            _handle: handle,
        }
    }

    fn push(&self, data: &[u8]) {
        let _ = self.tx.send(DecodeMsg::Data(data.to_vec()));
    }

    fn drain_frames(&self) -> Vec<egui::ColorImage> {
        let mut frames = Vec::new();
        while let Ok(f) = self.rx.try_recv() {
            frames.push(f);
        }
        frames
    }
}

impl Drop for DecodeThread {
    fn drop(&mut self) {
        let _ = self.tx.try_send(DecodeMsg::Shutdown);
    }
}

/// The actual decoder logic, living entirely inside the decode thread
enum InnerDecoder {
    Openh264(openh264::decoder::Decoder),
    #[cfg(feature = "ffmpeg")]
    Ffmpeg(ffmpeg::NalDecoder),
    #[cfg(feature = "v4l2m2m")]
    V4l2M2m(v4l2m2m::VpuDecoder),
    #[cfg(feature = "imxvpuapi2")]
    Imxvpuapi2(imxvpuapi2::VpuDecoder),
}

fn nal_type(nal: &[u8]) -> u8 {
    let data = if nal.starts_with(&[0, 0, 0, 1]) {
        &nal[4..]
    } else if nal.starts_with(&[0, 0, 1]) {
        &nal[3..]
    } else {
        nal
    };
    data.first().map(|b| b & 0x1f).unwrap_or(0)
}

impl InnerDecoder {
    fn decode(&mut self, data: &[u8]) -> Vec<egui::ColorImage> {
        let mut frames = Vec::new();
        match self {
            #[cfg(feature = "imxvpuapi2")]
            Self::Imxvpuapi2(v) => {
                for nal in openh264::nal_units(data) {
                    let t = nal_type(&nal);
                    match v.push_nal(nal) {
                        Err(e) => log::error!("imxvpuapi2 push_nal error: {e}"),
                        Ok(decoded_frames) => {
                            if let Some(frame) = decoded_frames.last() {
                                let w = frame.actual_width;
                                let h = frame.actual_height;
                                let pixels = imxvpuapi2::nv12_to_egui(&frame);
                                frames.push(egui::ColorImage {
                                    source_size: [w as f32, h as f32].into(),
                                    size: [w, h],
                                    pixels,
                                });
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "v4l2m2m")]
            Self::V4l2M2m(v) => {
                for nal in openh264::nal_units(data) {
                    match v.push_nal(nal) {
                        Err(e) => log::error!("VPU push_nal error: {:?}", e),
                        Ok(nv12_frames) => {
                            for (nv12, w, h) in nv12_frames {
                                let pixels = v4l2m2m::nv12_to_egui(&nv12, w, h);
                                frames.push(egui::ColorImage {
                                    source_size: [w as f32, h as f32].into(),
                                    size: [w as usize, h as usize],
                                    pixels,
                                });
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "ffmpeg")]
            Self::Ffmpeg(v) => {
                for nal in openh264::nal_units(data) {
                    if let Err(e) = v.push_nal(nal, None, None) {
                        log::error!("Failed to push NAL to ffmpeg decoder: {:?}", e);
                        continue;
                    }
                    loop {
                        match v.next_frame() {
                            Err(e) => {
                                log::error!("Failed to decode video: {:?}", e);
                                break;
                            }
                            Ok(None) => break,
                            Ok(Some(frame)) => match frame.to_rgb24() {
                                Err(e) => log::error!("Failed to convert frame: {:?}", e),
                                Ok(rgb_frame) => {
                                    let w = rgb_frame.width() as usize;
                                    let h = rgb_frame.height() as usize;
                                    let stride = rgb_frame.stride(0);
                                    let src = rgb_frame.data(0);
                                    let mut rgb_raw = Vec::with_capacity(w * h * 3);
                                    for row in 0..h {
                                        let row_start = row * stride;
                                        rgb_raw
                                            .extend_from_slice(&src[row_start..row_start + w * 3]);
                                    }
                                    let pixels = rgb_raw
                                        .chunks_exact(3)
                                        .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
                                        .collect();
                                    frames.push(egui::ColorImage {
                                        source_size: [w as f32, h as f32].into(),
                                        size: [w, h],
                                        pixels,
                                    });
                                }
                            },
                        }
                    }
                }
            }
            Self::Openh264(v) => {
                let mut units = openh264::nal_units(data).peekable();
                while let Some(p) = units.next() {
                    match v.decode(p) {
                        Err(e) => log::error!("Failed to decode video: {:?}", e),
                        Ok(Some(image)) => {
                            use openh264::formats::YUVSource;
                            let mut rgb_raw = vec![0; image.rgb8_len()];
                            image.write_rgb8(&mut rgb_raw);
                            let (w, h) = image.dimensions_uv();
                            let ei = uobradio_comms::video::PixelData::Rgb(rgb_raw);
                            frames.push(egui::ColorImage {
                                source_size: [w as f32, h as f32].into(),
                                size: [w * 2, h * 2],
                                pixels: ei.get_egui(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        frames
    }
}

/// Public-facing decoder — all heavy work happens off the GUI thread
pub struct H264Decoder {
    thread: DecodeThread,
}

impl H264Decoder {
    pub fn new() -> Result<Self, String> {
        let inner = Self::make_inner()?;
        Ok(Self {
            thread: DecodeThread::spawn(inner),
        })
    }

    fn make_inner() -> Result<InnerDecoder, String> {
        #[cfg(feature = "ffmpeg")]
        {
            if let Ok(ffmpeg) =
                ffmpeg::NalDecoder::new(ffmpeg_next::codec::Id::H264, ffmpeg::DecoderConfig::auto())
            {
                log::info!("Got ffmpeg decoder");
                return Ok(InnerDecoder::Ffmpeg(ffmpeg));
            }
        }
        #[cfg(feature = "imxvpuapi2")]
        {
            match imxvpuapi2::VpuDecoder::open() {
                Ok(dec) => {
                    log::info!("imxvpuapi2: Hantro VPU decoder ready");
                    return Ok(InnerDecoder::Imxvpuapi2(dec));
                }
                Err(e) => log::warn!("imxvpuapi2: decoder unavailable: {e}"),
            }
        }
        #[cfg(feature = "v4l2m2m")]
        {
            for dev in &["/dev/video0", "/dev/video1", "/dev/video2"] {
                if let Ok(dec) = v4l2m2m::VpuDecoder::open(dev).map_err(|e| e.to_string()) {
                    log::info!("VPU decoder ready: {}x{}", dec.width, dec.height);
                    return Ok(InnerDecoder::V4l2M2m(dec));
                }
            }
        }
        log::info!("Using openh264");
        Ok(InnerDecoder::Openh264(
            openh264::decoder::Decoder::new().map_err(|_| "openh264 unknown error".to_string())?,
        ))
    }

    /// Push raw H.264 data — returns immediately, never blocks the GUI thread.
    pub fn push(&self, data: &[u8]) {
        self.thread.push(data);
    }

    /// Collect any frames the decode thread has finished — call this from
    /// egui's update() and display whatever comes back.
    pub fn drain_frames(&self) -> Vec<egui::ColorImage> {
        self.thread.drain_frames()
    }
}
