use super::{
    BgraFrame, GIF_FPS, GIF_MAX_EDGE, GifSampling, PERMISSION_MSG, RecordOutcome, RecordingInfo,
    WakeFn,
};
use crate::capture_output::{CaptureOutput, CaptureSize, CropRect};
use bevy::prelude::Entity;
use block2::RcBlock;
use crossbeam_channel::Sender;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVFileTypeMPEG4,
    AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264,
    AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
use objc2_screen_capture_kit::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput,
    SCStreamOutputType,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vmux_input::RecordStartResponse;

use bevy::winit::WINIT_WINDOWS;
use objc2_app_kit::NSView;
use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const PIXEL_FORMAT_BGRA: u32 = 0x4247_5241;

unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

struct SendCell<T>(T);
unsafe impl<T> Send for SendCell<T> {}
unsafe impl<T> Sync for SendCell<T> {}

type GifMsg = (Vec<u8>, u32, u32);

struct EncodeState {
    writer: SendCell<Retained<AVAssetWriter>>,
    input: SendCell<Retained<AVAssetWriterInput>>,
    adaptor: SendCell<Retained<AVAssetWriterInputPixelBufferAdaptor>>,
    started_session: bool,
    start: Instant,
    last_gif_ms: Option<u64>,
    gif_tx: Option<Sender<GifMsg>>,
    appended: u64,
    paused: bool,
    pause_anchor: Option<CMTime>,
    pts_offset: CMTime,
}

struct RecordingState {
    stream: Mutex<Option<SendCell<Retained<SCStream>>>>,
    encode: Mutex<EncodeState>,
    gif_join: Mutex<Option<JoinHandle<()>>>,
    _queue: SendCell<DispatchRetained<DispatchQueue>>,
    _delegate: Mutex<Option<SendCell<Retained<StreamOutput>>>>,
    temp_mp4: PathBuf,
    temp_gif: Option<PathBuf>,
    gif: bool,
    default_dir: PathBuf,
    deadline: Instant,
    out: Mutex<FinalizeTarget>,
    tx: Sender<RecordOutcome>,
    wake: Option<WakeFn>,
}

#[derive(Default, Clone)]
struct FinalizeTarget {
    dir: Option<String>,
    name: Option<String>,
    request_id: Option<[u8; 16]>,
    finalizing: bool,
}

#[derive(Default)]
pub(crate) struct CaptureRuntime {
    active: Option<Arc<RecordingState>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VmuxStreamOutput"]
    #[ivars = Weak<RecordingState>]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            let Some(state) = self.ivars().upgrade() else {
                return;
            };
            state.handle_sample(sample);
        }
    }
);

impl StreamOutput {
    fn new(state: Weak<RecordingState>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(state);
        unsafe { msg_send![super(this), init] }
    }
}

impl RecordingState {
    fn handle_sample(&self, sample: &CMSampleBuffer) {
        let Some(image_buffer) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let pts = unsafe { sample.presentation_time_stamp() };
        let mut enc = self.encode.lock().unwrap();

        if enc.paused {
            if enc.pause_anchor.is_none() {
                enc.pause_anchor = Some(pts);
            }
            return;
        }
        if let Some(anchor) = enc.pause_anchor.take() {
            let gap = unsafe { pts.subtract(anchor) };
            enc.pts_offset = unsafe { enc.pts_offset.add(gap) };
        }
        let adj = unsafe { pts.subtract(enc.pts_offset) };

        if !enc.started_session {
            unsafe { enc.writer.0.startSessionAtSourceTime(adj) };
            enc.started_session = true;
        }
        if unsafe { enc.input.0.isReadyForMoreMediaData() } {
            let appended = unsafe {
                enc.adaptor
                    .0
                    .appendPixelBuffer_withPresentationTime(&image_buffer, adj)
            };
            if appended {
                enc.appended += 1;
            }
        }

        if let Some(gif_tx) = enc.gif_tx.clone() {
            let elapsed_ms = enc.start.elapsed().as_millis() as u64;
            if GifSampling::new(elapsed_ms, enc.last_gif_ms, GIF_FPS).should_sample() {
                enc.last_gif_ms = Some(elapsed_ms);
                if let Some(frame) = CaptureFrame::rgba(&image_buffer) {
                    let _ = gif_tx.try_send(frame);
                }
            }
        }
    }
}

struct CaptureFrame;

impl CaptureFrame {
    fn rgba(pixel_buffer: &CVPixelBuffer) -> Option<(Vec<u8>, u32, u32)> {
        unsafe {
            CVPixelBufferLockBaseAddress(pixel_buffer, CVPixelBufferLockFlags::ReadOnly);
            let width = CVPixelBufferGetWidth(pixel_buffer) as u32;
            let height = CVPixelBufferGetHeight(pixel_buffer) as u32;
            let stride = CVPixelBufferGetBytesPerRow(pixel_buffer);
            let base = CVPixelBufferGetBaseAddress(pixel_buffer) as *const u8;
            if base.is_null() || width == 0 || height == 0 {
                CVPixelBufferUnlockBaseAddress(pixel_buffer, CVPixelBufferLockFlags::ReadOnly);
                return None;
            }
            let row_bytes = width as usize * 4;
            let mut bgra = vec![0u8; row_bytes * height as usize];
            for row in 0..height as usize {
                let src = base.add(row * stride);
                let dst = bgra.as_mut_ptr().add(row * row_bytes);
                std::ptr::copy_nonoverlapping(src, dst, row_bytes);
            }
            CVPixelBufferUnlockBaseAddress(pixel_buffer, CVPixelBufferLockFlags::ReadOnly);

            let rgba = BgraFrame::new(bgra).rgba();
            let image = image::RgbaImage::from_raw(width, height, rgba)?;
            let size = CaptureSize::new(image.width(), image.height()).downscaled(GIF_MAX_EDGE);
            let scaled = if (size.width, size.height) == image.dimensions() {
                image
            } else {
                image::imageops::resize(
                    &image,
                    size.width,
                    size.height,
                    image::imageops::FilterType::Triangle,
                )
            };
            let (width, height) = scaled.dimensions();
            Some((scaled.into_raw(), width, height))
        }
    }
}

struct GifEncoder;

impl GifEncoder {
    fn run(path: PathBuf, rx: crossbeam_channel::Receiver<GifMsg>) {
        let mut encoder: Option<gif::Encoder<std::io::BufWriter<std::fs::File>>> = None;
        let delay = (100 / GIF_FPS.max(1)) as u16;
        while let Ok((rgba, width, height)) = rx.recv() {
            if encoder.is_none() {
                let Ok(file) = std::fs::File::create(&path) else {
                    return;
                };
                let writer = std::io::BufWriter::new(file);
                match gif::Encoder::new(writer, width as u16, height as u16, &[]) {
                    Ok(mut value) => {
                        let _ = value.set_repeat(gif::Repeat::Infinite);
                        encoder = Some(value);
                    }
                    Err(_) => return,
                }
            }
            let Some(encoder) = encoder.as_mut() else {
                return;
            };
            let quantizer = color_quant::NeuQuant::new(10, 256, &rgba);
            let indices = rgba
                .chunks_exact(4)
                .map(|pixel| quantizer.index_of(pixel) as u8)
                .collect();
            let mut frame = gif::Frame {
                width: width as u16,
                height: height as u16,
                delay,
                ..Default::default()
            };
            frame.buffer = std::borrow::Cow::Owned(indices);
            frame.palette = Some(quantizer.color_map_rgb());
            let _ = encoder.write_frame(&frame);
        }
    }
}

impl EncodeState {
    fn new(
        temp_mp4: &Path,
        width: u32,
        height: u32,
        bitrate: i32,
        start: Instant,
        gif_tx: Option<Sender<GifMsg>>,
    ) -> Result<Self, String> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&temp_mp4.to_string_lossy()));
        let file_type = unsafe { AVFileTypeMPEG4 }.ok_or("AVFileTypeMPEG4 unavailable")?;
        let writer = unsafe {
            AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url, file_type)
        }
        .map_err(|error| format!("AVAssetWriter init failed: {error:?}"))?;

        let codec_key = unsafe { AVVideoCodecKey }.ok_or("AVVideoCodecKey unavailable")?;
        let codec_h264 =
            unsafe { AVVideoCodecTypeH264 }.ok_or("AVVideoCodecTypeH264 unavailable")?;
        let width_key = unsafe { AVVideoWidthKey }.ok_or("AVVideoWidthKey unavailable")?;
        let height_key = unsafe { AVVideoHeightKey }.ok_or("AVVideoHeightKey unavailable")?;
        let compression_key = unsafe { AVVideoCompressionPropertiesKey }
            .ok_or("AVVideoCompressionPropertiesKey unavailable")?;
        let bitrate_key =
            unsafe { AVVideoAverageBitRateKey }.ok_or("AVVideoAverageBitRateKey unavailable")?;

        let bitrate_number = NSNumber::new_i32(bitrate);
        let compression = NSDictionary::<NSString, AnyObject>::from_slices(
            &[bitrate_key],
            &[AsRef::<AnyObject>::as_ref(&bitrate_number)],
        );
        let width_number = NSNumber::new_i32(width as i32);
        let height_number = NSNumber::new_i32(height as i32);
        let keys: [&NSString; 4] = [codec_key, width_key, height_key, compression_key];
        let objects: [&AnyObject; 4] = [
            AsRef::<AnyObject>::as_ref(codec_h264),
            AsRef::<AnyObject>::as_ref(&width_number),
            AsRef::<AnyObject>::as_ref(&height_number),
            AsRef::<AnyObject>::as_ref(&compression),
        ];
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(&keys, &objects);

        let media_type = unsafe { AVMediaTypeVideo }.ok_or("AVMediaTypeVideo unavailable")?;
        let input = unsafe {
            AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                media_type,
                Some(&settings),
            )
        };
        unsafe { input.setExpectsMediaDataInRealTime(true) };
        let adaptor = unsafe {
            AVAssetWriterInputPixelBufferAdaptor::initWithAssetWriterInput_sourcePixelBufferAttributes(
                AVAssetWriterInputPixelBufferAdaptor::alloc(),
                &input,
                None,
            )
        };

        unsafe {
            writer.addInput(&input);
            if !writer.startWriting() {
                return Err("AVAssetWriter.startWriting failed".to_string());
            }
        }
        Ok(Self {
            writer: SendCell(writer),
            input: SendCell(input),
            adaptor: SendCell(adaptor),
            started_session: false,
            start,
            last_gif_ms: None,
            gif_tx,
            appended: 0,
            paused: false,
            pause_anchor: None,
            pts_offset: unsafe { CMTime::new(0, 1) },
        })
    }
}

impl CaptureRuntime {
    fn supported() -> bool {
        let version = NSOperatingSystemVersion {
            majorVersion: 14,
            minorVersion: 0,
            patchVersion: 0,
        };
        NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
    }

    fn window_number(window_entity: Entity) -> Option<u32> {
        WINIT_WINDOWS.with_borrow(|winit_windows| {
            let window = winit_windows.get_window(window_entity)?;
            let handle = window.window_handle().ok()?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return None;
            };
            let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
            let window = view.window()?;
            Some(window.windowNumber() as u32)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start(
        &mut self,
        window_entity: Entity,
        img_w: u32,
        img_h: u32,
        crop: Option<CropRect>,
        request_id: [u8; 16],
        gif: bool,
        max_secs: u32,
        default_dir: PathBuf,
        scale: f64,
        tx: Sender<RecordOutcome>,
        wake: Option<WakeFn>,
    ) -> RecordStartResponse {
        let err = |m: String| RecordStartResponse {
            request_id,
            result: Err(m),
        };

        if self.active.is_some() {
            return err("a recording is already in progress; stop it first".into());
        }
        if !Self::supported() {
            return err("recording requires macOS 14 or later".into());
        }
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            unsafe { CGRequestScreenCaptureAccess() };
            return err(PERMISSION_MSG.into());
        }
        let Some(window_id) = Self::window_number(window_entity) else {
            return err("cannot resolve native window".into());
        };

        let (out_w, out_h) = crop.map_or((img_w, img_h), |c| (c.w, c.h));
        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        let rid: String = request_id[..4].iter().map(|b| format!("{b:02x}")).collect();
        let tmp_dir = vmux_ecs::profile::ProfilePaths::current().recording();
        if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
            return err(format!("cannot create {}: {e}", tmp_dir.display()));
        }
        let temp_mp4 = tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.mp4"));
        let temp_gif = gif.then(|| tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.gif")));

        let (done_tx, done_rx) = mpsc::channel::<Result<Arc<RecordingState>, String>>();
        let handler = RcBlock::new(move |content: *mut SCShareableContent, _e: *mut NSError| {
            Self::setup(
                content,
                window_id,
                out_w,
                out_h,
                crop,
                scale,
                gif,
                max_secs,
                &temp_mp4,
                &temp_gif,
                &default_dir,
                &tx,
                &wake,
                done_tx.clone(),
            );
        });
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };

        match done_rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(state)) => {
                self.active = Some(state);
                RecordStartResponse {
                    request_id,
                    result: Ok(max_secs),
                }
            }
            Ok(Err(m)) => err(m),
            Err(_) => err("timed out preparing capture".into()),
        }
    }

    pub(crate) fn stop(&mut self, request_id: [u8; 16], dir: Option<String>, name: Option<String>) {
        let Some(state) = self.active.clone() else {
            return;
        };
        {
            let mut out = state.out.lock().unwrap();
            if out.finalizing {
                return;
            }
            out.dir = dir;
            out.name = name;
            out.request_id = Some(request_id);
            out.finalizing = true;
        }
        state.finalize();
    }

    pub(crate) fn poll_auto_stop(&mut self) {
        let Some(state) = self.active.clone() else {
            return;
        };
        if Instant::now() < state.deadline {
            return;
        }
        {
            let mut out = state.out.lock().unwrap();
            if out.finalizing {
                return;
            }
            out.request_id = None;
            out.finalizing = true;
        }
        state.finalize();
    }

    pub(crate) fn pause(&mut self) {
        if let Some(state) = &self.active {
            state.encode.lock().unwrap().paused = true;
        }
    }

    pub(crate) fn resume(&mut self) {
        if let Some(state) = &self.active {
            state.encode.lock().unwrap().paused = false;
        }
    }

    pub(crate) fn done(&mut self) {
        let Some(state) = self.active.clone() else {
            return;
        };
        {
            let mut out = state.out.lock().unwrap();
            if out.finalizing {
                return;
            }
            out.request_id = None;
            out.finalizing = true;
        }
        state.finalize();
    }

    pub(crate) fn complete(&mut self) {
        self.active = None;
    }
}

impl CaptureRuntime {
    #[allow(clippy::too_many_arguments)]
    fn setup(
        content: *mut SCShareableContent,
        window_id: u32,
        output_width: u32,
        output_height: u32,
        crop: Option<CropRect>,
        scale: f64,
        gif: bool,
        max_secs: u32,
        temp_mp4: &Path,
        temp_gif: &Option<PathBuf>,
        default_dir: &Path,
        tx: &Sender<RecordOutcome>,
        wake: &Option<WakeFn>,
        done_tx: mpsc::Sender<Result<Arc<RecordingState>, String>>,
    ) {
        if content.is_null() {
            let _ = done_tx.send(Err("SCShareableContent unavailable".into()));
            return;
        }
        let content = unsafe { &*content };
        let windows = unsafe { content.windows() };
        let Some(window) = windows
            .iter()
            .find(|window| unsafe { window.windowID() } == window_id)
        else {
            let _ = done_tx.send(Err("vmux window not shareable".into()));
            return;
        };

        let filter = unsafe {
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
        };
        let size =
            CaptureSize::new(output_width, output_height).downscaled(super::RECORDING_MAX_EDGE);
        let config = unsafe { SCStreamConfiguration::new() };
        unsafe {
            config.setWidth(size.width as usize);
            config.setHeight(size.height as usize);
            config.setPixelFormat(PIXEL_FORMAT_BGRA);
            config.setMinimumFrameInterval(CMTime::new(1, 60));
            config.setShowsCursor(true);
            config.setQueueDepth(6);
            if let Some(crop) = crop {
                config.setSourceRect(CGRect::new(
                    CGPoint::new(crop.x as f64 / scale, crop.y as f64 / scale),
                    CGSize::new(crop.w as f64 / scale, crop.h as f64 / scale),
                ));
            }
        }

        let (gif_tx, gif_join) = if let Some(path) = temp_gif.clone() {
            let (sender, receiver) = crossbeam_channel::bounded::<GifMsg>(8);
            let join = std::thread::spawn(move || GifEncoder::run(path, receiver));
            (Some(sender), Some(join))
        } else {
            (None, None)
        };

        let start = Instant::now();
        let encode = match EncodeState::new(
            temp_mp4,
            size.width,
            size.height,
            super::RECORDING_BITRATE_BPS,
            start,
            gif_tx,
        ) {
            Ok(encode) => encode,
            Err(error) => {
                let _ = done_tx.send(Err(error));
                return;
            }
        };
        let queue = DispatchQueue::new("ai.vmux.recording", None);
        let state = Arc::new(RecordingState {
            stream: Mutex::new(None),
            encode: Mutex::new(encode),
            gif_join: Mutex::new(gif_join),
            _queue: SendCell(queue),
            _delegate: Mutex::new(None),
            temp_mp4: temp_mp4.to_path_buf(),
            temp_gif: temp_gif.clone(),
            gif,
            default_dir: default_dir.to_path_buf(),
            deadline: start + Duration::from_secs(max_secs as u64),
            out: Mutex::new(FinalizeTarget::default()),
            tx: tx.clone(),
            wake: wake.clone(),
        });

        let delegate = StreamOutput::new(Arc::downgrade(&state));
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                None,
            )
        };
        if let Err(error) = unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*delegate),
                SCStreamOutputType::Screen,
                Some(&state._queue.0),
            )
        } {
            let _ = done_tx.send(Err(format!("addStreamOutput failed: {error:?}")));
            return;
        }
        *state.stream.lock().unwrap() = Some(SendCell(stream.clone()));
        *state._delegate.lock().unwrap() = Some(SendCell(delegate));

        let start_state = state.clone();
        let start_block = RcBlock::new(move |error: *mut NSError| {
            if error.is_null() {
                let _ = done_tx.send(Ok(start_state.clone()));
            } else {
                let description = unsafe { (*error).localizedDescription() };
                let _ = done_tx.send(Err(format!("startCapture failed: {description}")));
            }
        });
        unsafe { stream.startCaptureWithCompletionHandler(Some(&start_block)) };
    }
}

impl RecordingState {
    fn finalize(self: Arc<Self>) {
        let stream = self.stream.lock().unwrap().take();
        if let Some(stream) = stream {
            let state = self.clone();
            let completion = RcBlock::new(move |_error: *mut NSError| {
                state.clone().finish_writer();
            });
            unsafe { stream.0.stopCaptureWithCompletionHandler(Some(&completion)) };
        } else {
            self.finish_writer();
        }
    }

    fn finish_writer(self: Arc<Self>) {
        let appended = {
            let mut encode = self.encode.lock().unwrap();
            encode.gif_tx = None;
            if encode.appended > 0 {
                unsafe { encode.input.0.markAsFinished() };
            }
            encode.appended
        };
        if let Some(join) = self.gif_join.lock().unwrap().take() {
            let _ = join.join();
        }
        if appended == 0 {
            self.deliver(Err(
                "recording captured no frames (grant Screen Recording permission and retry)".into(),
            ));
            return;
        }
        let writer = self.encode.lock().unwrap().writer.0.clone();
        let state = self.clone();
        let completion = RcBlock::new(move || {
            state.clone().deliver(Ok(()));
        });
        unsafe { writer.finishWritingWithCompletionHandler(&completion) };
    }

    fn deliver(&self, finish_result: Result<(), String>) {
        let target = self.out.lock().unwrap().clone();
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        let (final_mp4, final_gif) = CaptureOutput::paths(
            target.dir.as_deref(),
            target.name.as_deref(),
            self.gif,
            &timestamp,
            &self.default_dir,
        );

        let result = finish_result.and_then(|()| -> Result<RecordingInfo, String> {
            if let Some(parent) = final_mp4.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
            }
            std::fs::rename(&self.temp_mp4, &final_mp4)
                .map_err(|error| format!("cannot move mp4: {error}"))?;
            let gif_path = match (&self.temp_gif, &final_gif) {
                (Some(temp), Some(destination)) => {
                    std::fs::rename(temp, destination)
                        .map_err(|error| format!("cannot move gif: {error}"))?;
                    Some(destination.to_string_lossy().into_owned())
                }
                _ => None,
            };
            let bytes = std::fs::metadata(&final_mp4)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let duration_ms = self.encode.lock().unwrap().start.elapsed().as_millis() as u64;
            Ok(RecordingInfo {
                mp4_path: final_mp4.to_string_lossy().into_owned(),
                gif_path,
                duration_ms,
                bytes,
                auto_stopped: target.request_id.is_none(),
            })
        });

        if result.is_err() {
            let _ = std::fs::remove_file(&self.temp_mp4);
            if let Some(temp) = &self.temp_gif {
                let _ = std::fs::remove_file(temp);
            }
        }

        let _ = self.tx.send(RecordOutcome {
            request_id: target.request_id,
            result,
        });
        if let Some(wake) = &self.wake {
            wake();
        }
    }
}
