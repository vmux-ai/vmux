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
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
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
    start: Instant,
    last_gif_ms: Option<u64>,
    gif_tx: Option<Sender<GifMsg>>,
    /// Count of pixel buffers successfully appended. Zero at finalize means the
    /// capture produced no frames (the writer never started a session).
    appended: u64,
    /// Paused state. While paused, frames are dropped (mp4 + gif) and the gap is
    /// folded into `pts_offset` on resume for a seamless (cut) timeline.
    paused: bool,
    /// PTS of the frame at which the current pause began.
    pause_anchor: Option<CMTime>,
    /// Accumulated paused duration subtracted from every appended frame's PTS.
    pts_offset: CMTime,
}

struct RecordingState {
    stream: Mutex<Option<SendCell<Retained<SCStream>>>>,
    encode: Mutex<EncodeState>,
    gif_join: Mutex<Option<JoinHandle<()>>>,
    _queue: SendCell<DispatchRetained<DispatchQueue>>,
    /// The SCStream output delegate, retained for the recording's lifetime.
    /// SCStream does not strongly retain it, so without this it would dealloc
    /// and stop delivering sample buffers.
    _delegate: Mutex<Option<SendCell<Retained<StreamOutput>>>>,
    temp_mp4: PathBuf,
    temp_gif: Option<PathBuf>,
    gif: bool,
    /// Configured/default output directory used when stop provides no explicit dir.
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

fn active() -> &'static Mutex<Option<Arc<RecordingState>>> {
    static ACTIVE: OnceLock<Mutex<Option<Arc<RecordingState>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
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
            handle_sample(&state, sample);
        }
    }
);

impl StreamOutput {
    fn new(state: Weak<RecordingState>) -> Retained<Self> {
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

    if enc.paused {
        // Drop frames while paused; remember where the pause began so the gap
        // can be removed on resume.
        if enc.pause_anchor.is_none() {
            enc.pause_anchor = Some(pts);
        }
        return;
    }
    // Resuming after a pause: fold the paused span into the running offset so the
    // output timeline is continuous (a seamless cut, no frozen gap).
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
        if should_sample_gif_frame(elapsed_ms, enc.last_gif_ms, GIF_FPS) {
            enc.last_gif_ms = Some(elapsed_ms);
            if let Some(frame) = pixel_buffer_to_downscaled_rgba(&image_buffer) {
                let _ = gif_tx.try_send(frame);
            }
        }
    }
}

fn pixel_buffer_to_downscaled_rgba(pb: &CVPixelBuffer) -> Option<(Vec<u8>, u32, u32)> {
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
        let img = image::RgbaImage::from_raw(w, h, rgba)?;
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
    default_dir: PathBuf,
    scale: f64,
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
    let tmp_dir = vmux_core::profile::recording_dir();
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        return err(format!("cannot create {}: {e}", tmp_dir.display()));
    }
    let temp_mp4 = tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.mp4"));
    let temp_gif = gif.then(|| tmp_dir.join(format!(".vmux-rec-{ts}-{rid}.gif")));

    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    let handler = RcBlock::new(move |content: *mut SCShareableContent, _e: *mut NSError| {
        setup_stream(
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

    // Wait for the capture to actually start (or fail) so the agent sees real
    // errors instead of a false "started". 8s covers the SCK content fetch +
    // startCapture, comfortably under the broker query timeout.
    match done_rx.recv_timeout(Duration::from_secs(8)) {
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
    out_w: u32,
    out_h: u32,
    crop: Option<CropRect>,
    scale: f64,
    gif: bool,
    max_secs: u32,
    temp_mp4: &Path,
    temp_gif: &Option<PathBuf>,
    default_dir: &Path,
    tx: &Sender<RecordOutcome>,
    wake: &Option<WakeFn>,
    done_tx: mpsc::Sender<Result<(), String>>,
) {
    if content.is_null() {
        let _ = done_tx.send(Err("SCShareableContent unavailable".into()));
        return;
    }
    let content = unsafe { &*content };
    let windows = unsafe { content.windows() };
    let Some(window) = windows
        .iter()
        .find(|w| unsafe { w.windowID() } == window_id)
    else {
        let _ = done_tx.send(Err("vmux window not shareable".into()));
        return;
    };

    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
    };
    let config = unsafe { SCStreamConfiguration::new() };
    unsafe {
        config.setWidth(out_w as usize);
        config.setHeight(out_h as usize);
        config.setPixelFormat(PIXEL_FORMAT_BGRA);
        config.setMinimumFrameInterval(CMTime::new(1, 60));
        config.setShowsCursor(true);
        config.setQueueDepth(6);
        // For pane recording, capture only the crop sub-region. sourceRect is in
        // points (logical, top-left origin); our CropRect is physical px.
        if let Some(c) = crop {
            config.setSourceRect(CGRect::new(
                CGPoint::new(c.x as f64 / scale, c.y as f64 / scale),
                CGSize::new(c.w as f64 / scale, c.h as f64 / scale),
            ));
        }
    }

    let (writer, input, adaptor) = match build_writer(temp_mp4, out_w, out_h) {
        Ok(t) => t,
        Err(e) => {
            let _ = done_tx.send(Err(e));
            return;
        }
    };

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
            start,
            last_gif_ms: None,
            gif_tx,
            appended: 0,
            paused: false,
            pause_anchor: None,
            pts_offset: unsafe { CMTime::new(0, 1) },
        }),
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
        SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None)
    };
    if let Err(e) = unsafe {
        stream.addStreamOutput_type_sampleHandlerQueue_error(
            ProtocolObject::from_ref(&*delegate),
            SCStreamOutputType::Screen,
            Some(&state._queue.0),
        )
    } {
        let _ = done_tx.send(Err(format!("addStreamOutput failed: {e:?}")));
        return;
    }
    *state.stream.lock().unwrap() = Some(SendCell(stream.clone()));
    *state._delegate.lock().unwrap() = Some(SendCell(delegate));

    // Only mark the session active once capture has actually started, and report
    // any startCapture error back to `start` instead of swallowing it.
    let start_state = state.clone();
    let start_block = RcBlock::new(move |e: *mut NSError| {
        if e.is_null() {
            *active().lock().unwrap() = Some(start_state.clone());
            let _ = done_tx.send(Ok(()));
        } else {
            let desc = unsafe { (*e).localizedDescription() };
            let _ = done_tx.send(Err(format!("startCapture failed: {desc}")));
        }
    });
    unsafe { stream.startCaptureWithCompletionHandler(Some(&start_block)) };
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

pub(crate) fn pause() {
    if let Some(state) = active().lock().unwrap().clone() {
        state.encode.lock().unwrap().paused = true;
    }
}

pub(crate) fn resume() {
    if let Some(state) = active().lock().unwrap().clone() {
        state.encode.lock().unwrap().paused = false;
    }
}

/// Finalize an in-progress recording to the default dir (no explicit dir/name),
/// driven from the tray. Mirrors the auto-stop path (`request_id == None`).
pub(crate) fn done() {
    let Some(state) = active().lock().unwrap().clone() else {
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
    let appended = {
        let mut enc = state.encode.lock().unwrap();
        enc.gif_tx = None;
        if enc.appended > 0 {
            unsafe { enc.input.0.markAsFinished() };
        }
        enc.appended
    };
    if let Some(join) = state.gif_join.lock().unwrap().take() {
        let _ = join.join();
    }
    if appended == 0 {
        // No frames captured: the writer never started a session, so calling
        // finishWriting would hang. Abandon it and report a clear error.
        deliver(
            state,
            Err(
                "recording captured no frames (grant Screen Recording permission and retry)".into(),
            ),
        );
        return;
    }
    let writer = state.encode.lock().unwrap().writer.0.clone();
    let s = state.clone();
    let completion = RcBlock::new(move || {
        deliver(s.clone(), Ok(()));
    });
    unsafe { writer.finishWritingWithCompletionHandler(&completion) };
}

fn deliver(state: Arc<RecordingState>, finish_result: Result<(), String>) {
    let target = state.out.lock().unwrap().clone();
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let (final_mp4, final_gif) = resolve_output_paths(
        target.dir.as_deref(),
        target.name.as_deref(),
        state.gif,
        &ts,
        &state.default_dir,
    );

    let result = finish_result.and_then(|()| -> Result<RecordingInfo, String> {
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
    });

    if result.is_err() {
        let _ = std::fs::remove_file(&state.temp_mp4);
        if let Some(tmp) = &state.temp_gif {
            let _ = std::fs::remove_file(tmp);
        }
    }

    *active().lock().unwrap() = None;
    let _ = state.tx.send(RecordOutcome {
        request_id: target.request_id,
        result,
    });
    if let Some(w) = &state.wake {
        w();
    }
}
