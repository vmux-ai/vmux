use super::{
    CropRect, GIF_FPS, GIF_MAX_EDGE, PERMISSION_MSG, RecordOutcome, RecordingInfo, WakeFn,
    bgra_to_rgba, downscale_to, resolve_output_paths, should_sample_gif_frame,
};
use bevy::prelude::Entity;
use block2::RcBlock;
use crossbeam_channel::Sender;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVFileTypeMPEG4,
    AVMediaTypeVideo, AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoHeightKey, AVVideoWidthKey,
};
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
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vmux_agent::RecordStartResponse;

const PIXEL_FORMAT_BGRA: u32 = 0x4247_5241; // 'BGRA' fourcc

unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// objc/dispatch retained handles are not auto-`Send`. We move them between the
/// main thread and SCStream's serial queue / completion threads, with access
/// guarded by `RecordingState`'s mutexes and SCStream's serial output queue.
struct SendCell<T>(T);
unsafe impl<T> Send for SendCell<T> {}
unsafe impl<T> Sync for SendCell<T> {}

type GifMsg = (Vec<u8>, u32, u32);

struct EncodeState {
    writer: SendCell<Retained<AVAssetWriter>>,
    input: SendCell<Retained<AVAssetWriterInput>>,
    adaptor: SendCell<Retained<AVAssetWriterInputPixelBufferAdaptor>>,
    started_session: bool,
    crop: Option<CropRect>,
    start: Instant,
    last_gif_ms: Option<u64>,
    gif_tx: Option<Sender<GifMsg>>,
}

struct RecordingState {
    stream: Mutex<Option<SendCell<Retained<SCStream>>>>,
    encode: Mutex<EncodeState>,
    gif_join: Mutex<Option<JoinHandle<()>>>,
    _queue: SendCell<DispatchRetained<DispatchQueue>>,
    temp_mp4: PathBuf,
    temp_gif: Option<PathBuf>,
    gif: bool,
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

fn active() -> &'static Mutex<Option<Arc<RecordingState>>> {
    static ACTIVE: OnceLock<Mutex<Option<Arc<RecordingState>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VmuxStreamOutput"]
    #[ivars = Arc<RecordingState>]
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
            handle_sample(self.ivars(), sample);
        }
    }
);

impl StreamOutput {
    fn new(state: Arc<RecordingState>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(state);
        unsafe { msg_send![super(this), init] }
    }
}

fn handle_sample(state: &Arc<RecordingState>, sample: &CMSampleBuffer) {
    let Some(image_buffer) = (unsafe { sample.image_buffer() }) else {
        return;
    };
    let pts = unsafe { sample.presentation_time_stamp() };
    let mut enc = state.encode.lock().unwrap();

    if !enc.started_session {
        unsafe { enc.writer.0.startSessionAtSourceTime(pts) };
        enc.started_session = true;
    }
    if unsafe { enc.input.0.isReadyForMoreMediaData() } {
        unsafe {
            enc.adaptor
                .0
                .appendPixelBuffer_withPresentationTime(&image_buffer, pts);
        }
    }

    if let Some(gif_tx) = enc.gif_tx.clone() {
        let elapsed_ms = enc.start.elapsed().as_millis() as u64;
        if should_sample_gif_frame(elapsed_ms, enc.last_gif_ms, GIF_FPS) {
            enc.last_gif_ms = Some(elapsed_ms);
            if let Some(frame) = pixel_buffer_to_downscaled_rgba(&image_buffer, enc.crop) {
                let _ = gif_tx.try_send(frame);
            }
        }
    }
}

fn pixel_buffer_to_downscaled_rgba(
    pb: &CVPixelBuffer,
    crop: Option<CropRect>,
) -> Option<(Vec<u8>, u32, u32)> {
    unsafe {
        CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
        let w = CVPixelBufferGetWidth(pb) as u32;
        let h = CVPixelBufferGetHeight(pb) as u32;
        let stride = CVPixelBufferGetBytesPerRow(pb);
        let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
        if base.is_null() || w == 0 || h == 0 {
            CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);
            return None;
        }
        let row_bytes = w as usize * 4;
        let mut bgra = vec![0u8; row_bytes * h as usize];
        for row in 0..h as usize {
            let src = base.add(row * stride);
            let dst = bgra.as_mut_ptr().add(row * row_bytes);
            std::ptr::copy_nonoverlapping(src, dst, row_bytes);
        }
        CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags::ReadOnly);

        let rgba = bgra_to_rgba(&bgra);
        let mut img = image::RgbaImage::from_raw(w, h, rgba)?;
        if let Some(c) = crop {
            img = image::imageops::crop_imm(&img, c.x, c.y, c.w, c.h).to_image();
        }
        let (dw, dh) = downscale_to(img.width(), img.height(), GIF_MAX_EDGE);
        let scaled = if (dw, dh) == img.dimensions() {
            img
        } else {
            image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle)
        };
        let (fw, fh) = scaled.dimensions();
        Some((scaled.into_raw(), fw, fh))
    }
}

fn gif_worker(path: PathBuf, rx: crossbeam_channel::Receiver<GifMsg>) {
    let mut encoder: Option<gif::Encoder<std::io::BufWriter<std::fs::File>>> = None;
    let delay = (100 / GIF_FPS.max(1)) as u16;
    while let Ok((rgba, w, h)) = rx.recv() {
        if encoder.is_none() {
            let Ok(file) = std::fs::File::create(&path) else {
                return;
            };
            let writer = std::io::BufWriter::new(file);
            match gif::Encoder::new(writer, w as u16, h as u16, &[]) {
                Ok(mut e) => {
                    let _ = e.set_repeat(gif::Repeat::Infinite);
                    encoder = Some(e);
                }
                Err(_) => return,
            }
        }
        let Some(enc) = encoder.as_mut() else { return };
        let nq = color_quant::NeuQuant::new(10, 256, &rgba);
        let indices: Vec<u8> = rgba.chunks_exact(4).map(|p| nq.index_of(p) as u8).collect();
        let mut frame = gif::Frame {
            width: w as u16,
            height: h as u16,
            delay,
            ..Default::default()
        };
        frame.buffer = std::borrow::Cow::Owned(indices);
        frame.palette = Some(nq.color_map_rgb());
        let _ = enc.write_frame(&frame);
    }
}

fn os_at_least_14() -> bool {
    use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
    let version = NSOperatingSystemVersion {
        majorVersion: 14,
        minorVersion: 0,
        patchVersion: 0,
    };
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
}

fn window_number(window_entity: Entity) -> Option<u32> {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let win = winit_windows.get_window(window_entity)?;
        let handle = win.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
        let window = view.window()?;
        Some(window.windowNumber() as u32)
    })
}

fn build_writer(
    temp_mp4: &Path,
    out_w: u32,
    out_h: u32,
) -> Result<
    (
        Retained<AVAssetWriter>,
        Retained<AVAssetWriterInput>,
        Retained<AVAssetWriterInputPixelBufferAdaptor>,
    ),
    String,
> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&temp_mp4.to_string_lossy()));
    let file_type = unsafe { AVFileTypeMPEG4 }.ok_or("AVFileTypeMPEG4 unavailable")?;
    let writer = unsafe {
        AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url, file_type)
    }
    .map_err(|e| format!("AVAssetWriter init failed: {e:?}"))?;

    let codec_key = unsafe { AVVideoCodecKey }.ok_or("AVVideoCodecKey unavailable")?;
    let codec_h264 = unsafe { AVVideoCodecTypeH264 }.ok_or("AVVideoCodecTypeH264 unavailable")?;
    let width_key = unsafe { AVVideoWidthKey }.ok_or("AVVideoWidthKey unavailable")?;
    let height_key = unsafe { AVVideoHeightKey }.ok_or("AVVideoHeightKey unavailable")?;

    let w_num = NSNumber::new_i32(out_w as i32);
    let h_num = NSNumber::new_i32(out_h as i32);
    let keys: [&NSString; 3] = [codec_key, width_key, height_key];
    let objects: [&AnyObject; 3] = [
        AsRef::<AnyObject>::as_ref(codec_h264),
        AsRef::<AnyObject>::as_ref(&w_num),
        AsRef::<AnyObject>::as_ref(&h_num),
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
    Ok((writer, input, adaptor))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    window_entity: Entity,
    img_w: u32,
    img_h: u32,
    crop: Option<CropRect>,
    request_id: [u8; 16],
    gif: bool,
    max_secs: u32,
    tx: Sender<RecordOutcome>,
    wake: Option<WakeFn>,
) -> RecordStartResponse {
    let err = |m: String| RecordStartResponse {
        request_id,
        result: Err(m),
    };

    if active().lock().unwrap().is_some() {
        return err("a recording is already in progress; stop it first".into());
    }
    if !os_at_least_14() {
        return err("recording requires macOS 14 or later".into());
    }
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        unsafe { CGRequestScreenCaptureAccess() };
        return err(PERMISSION_MSG.into());
    }
    let Some(window_id) = window_number(window_entity) else {
        return err("cannot resolve native window".into());
    };

    let (out_w, out_h) = crop.map_or((img_w, img_h), |c| (c.w, c.h));
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let rid: String = request_id[..4].iter().map(|b| format!("{b:02x}")).collect();
    let tmp_dir = vmux_core::profile::screenshots_dir();
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        return err(format!("cannot create {}: {e}", tmp_dir.display()));
    }
    let temp_mp4 = tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.mp4"));
    let temp_gif = gif.then(|| tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.gif")));

    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    let handler = RcBlock::new(move |content: *mut SCShareableContent, _e: *mut NSError| {
        let res = setup_stream(
            content, window_id, img_w, img_h, out_w, out_h, crop, gif, max_secs, &temp_mp4,
            &temp_gif, &tx, &wake,
        );
        let _ = done_tx.send(res);
    });
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };

    match done_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => RecordStartResponse {
            request_id,
            result: Ok(max_secs),
        },
        Ok(Err(m)) => err(m),
        Err(_) => err("timed out preparing capture".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_stream(
    content: *mut SCShareableContent,
    window_id: u32,
    img_w: u32,
    img_h: u32,
    out_w: u32,
    out_h: u32,
    crop: Option<CropRect>,
    gif: bool,
    max_secs: u32,
    temp_mp4: &Path,
    temp_gif: &Option<PathBuf>,
    tx: &Sender<RecordOutcome>,
    wake: &Option<WakeFn>,
) -> Result<(), String> {
    if content.is_null() {
        return Err("SCShareableContent unavailable".into());
    }
    let content = unsafe { &*content };
    let windows = unsafe { content.windows() };
    let window = windows
        .iter()
        .find(|w| unsafe { w.windowID() } == window_id)
        .ok_or("vmux window not shareable")?;

    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
    };
    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setWidth(img_w as usize);
        config.setHeight(img_h as usize);
        config.setPixelFormat(PIXEL_FORMAT_BGRA);
        config.setMinimumFrameInterval(CMTime::new(1, 60));
        config.setShowsCursor(true);
        config.setQueueDepth(6);
    }

    let (writer, input, adaptor) = build_writer(temp_mp4, out_w, out_h)?;

    let (gif_tx, gif_join) = if let Some(path) = temp_gif.clone() {
        let (s, r) = crossbeam_channel::bounded::<GifMsg>(8);
        let join = std::thread::spawn(move || gif_worker(path, r));
        (Some(s), Some(join))
    } else {
        (None, None)
    };

    let start = Instant::now();
    let queue = DispatchQueue::new("ai.vmux.recording", None);
    let state = Arc::new(RecordingState {
        stream: Mutex::new(None),
        encode: Mutex::new(EncodeState {
            writer: SendCell(writer),
            input: SendCell(input),
            adaptor: SendCell(adaptor),
            started_session: false,
            crop,
            start,
            last_gif_ms: None,
            gif_tx,
        }),
        gif_join: Mutex::new(gif_join),
        _queue: SendCell(queue),
        temp_mp4: temp_mp4.to_path_buf(),
        temp_gif: temp_gif.clone(),
        gif,
        deadline: start + Duration::from_secs(max_secs as u64),
        out: Mutex::new(FinalizeTarget::default()),
        tx: tx.clone(),
        wake: wake.clone(),
    });

    let delegate = StreamOutput::new(state.clone());
    let stream = unsafe {
        SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None)
    };
    unsafe {
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*delegate),
                SCStreamOutputType::Screen,
                Some(&state._queue.0),
            )
            .map_err(|e| format!("addStreamOutput failed: {e:?}"))?;
    }
    *state.stream.lock().unwrap() = Some(SendCell(stream.clone()));

    let start_block = RcBlock::new(move |_e: *mut NSError| {});
    unsafe { stream.startCaptureWithCompletionHandler(Some(&start_block)) };

    *active().lock().unwrap() = Some(state);
    Ok(())
}

pub(crate) fn stop(request_id: [u8; 16], dir: Option<String>, name: Option<String>) {
    let Some(state) = active().lock().unwrap().clone() else {
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
    finalize(state);
}

pub(crate) fn poll_auto_stop() {
    let Some(state) = active().lock().unwrap().clone() else {
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
    finalize(state);
}

fn finalize(state: Arc<RecordingState>) {
    let stream = state.stream.lock().unwrap().take();
    if let Some(stream) = stream {
        let s = state.clone();
        let completion = RcBlock::new(move |_e: *mut NSError| {
            finish_writer(s.clone());
        });
        unsafe { stream.0.stopCaptureWithCompletionHandler(Some(&completion)) };
    } else {
        finish_writer(state);
    }
}

fn finish_writer(state: Arc<RecordingState>) {
    {
        let mut enc = state.encode.lock().unwrap();
        enc.gif_tx = None;
        unsafe { enc.input.0.markAsFinished() };
    }
    if let Some(join) = state.gif_join.lock().unwrap().take() {
        let _ = join.join();
    }
    let writer = state.encode.lock().unwrap().writer.0.clone();
    let s = state.clone();
    let completion = RcBlock::new(move || {
        deliver(s.clone());
    });
    unsafe { writer.finishWritingWithCompletionHandler(&completion) };
}

fn deliver(state: Arc<RecordingState>) {
    let target = state.out.lock().unwrap().clone();
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let (final_mp4, final_gif) = resolve_output_paths(
        target.dir.as_deref(),
        target.name.as_deref(),
        state.gif,
        &ts,
    );

    let result = (|| -> Result<RecordingInfo, String> {
        if let Some(parent) = final_mp4.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::rename(&state.temp_mp4, &final_mp4)
            .map_err(|e| format!("cannot move mp4: {e}"))?;
        let gif_path = match (&state.temp_gif, &final_gif) {
            (Some(tmp), Some(dst)) => {
                std::fs::rename(tmp, dst).map_err(|e| format!("cannot move gif: {e}"))?;
                Some(dst.to_string_lossy().into_owned())
            }
            _ => None,
        };
        let bytes = std::fs::metadata(&final_mp4).map(|m| m.len()).unwrap_or(0);
        let duration_ms = state.encode.lock().unwrap().start.elapsed().as_millis() as u64;
        Ok(RecordingInfo {
            mp4_path: final_mp4.to_string_lossy().into_owned(),
            gif_path,
            duration_ms,
            bytes,
            auto_stopped: target.request_id.is_none(),
        })
    })();

    *active().lock().unwrap() = None;
    let _ = state.tx.send(RecordOutcome {
        request_id: target.request_id,
        result,
    });
    if let Some(w) = &state.wake {
        w();
    }
}
