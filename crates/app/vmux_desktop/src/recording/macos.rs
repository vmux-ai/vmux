use super::lifecycle::Source;
use super::native::{
    CaptureControl, CaptureHandle, Completion, Encoded, OpenCapture, OpenRequest, Opened,
};
use super::{BgraFrame, GifSampling, PERMISSION_MSG};
use crate::capture_output::CaptureSize;
use bevy::prelude::Entity;
use block2::RcBlock;
use crossbeam_channel::Sender;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
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
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Instant;

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
    gif_fps: u32,
    gif_max_edge: u32,
    appended: u64,
    paused: bool,
    pause_anchor: Option<CMTime>,
    pts_offset: CMTime,
}

struct RecordingState {
    stream: Mutex<Option<SendCell<Retained<SCStream>>>>,
    encode: Mutex<EncodeState>,
    gif_join: Mutex<Option<JoinHandle<Result<(), String>>>>,
    _queue: SendCell<DispatchRetained<DispatchQueue>>,
    _delegate: Mutex<Option<SendCell<Retained<StreamOutput>>>>,
    attempt: Entity,
    completion: Completion,
    finalizing: Mutex<bool>,
}

pub(super) struct Backend;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VmuxStreamOutput"]
    #[ivars = (vmux_ecs::EntityTarget<Source>, Weak<RecordingState>)]
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
            let (owner, state) = self.ivars();
            let Some(state) = state.upgrade() else {
                return;
            };
            if owner.entity() != state.attempt {
                return;
            }
            state.handle_sample(sample);
        }
    }
);

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
            if GifSampling::new(elapsed_ms, enc.last_gif_ms, enc.gif_fps).should_sample() {
                enc.last_gif_ms = Some(elapsed_ms);
                if let Some(frame) = CaptureFrame::rgba(&image_buffer, enc.gif_max_edge) {
                    let _ = gif_tx.try_send(frame);
                }
            }
        }
    }
}

struct CaptureFrame;

impl CaptureFrame {
    fn rgba(pixel_buffer: &CVPixelBuffer, max_edge: u32) -> Option<(Vec<u8>, u32, u32)> {
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
            let size = CaptureSize::new(image.width(), image.height()).downscaled(max_edge);
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
    fn run(path: PathBuf, rx: crossbeam_channel::Receiver<GifMsg>, fps: u32) -> Result<(), String> {
        let mut encoder: Option<gif::Encoder<std::io::BufWriter<std::fs::File>>> = None;
        let delay = (100 / fps.max(1)) as u16;
        while let Ok((rgba, width, height)) = rx.recv() {
            if encoder.is_none() {
                let file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
                let writer = std::io::BufWriter::new(file);
                let mut value = gif::Encoder::new(writer, width as u16, height as u16, &[])
                    .map_err(|error| error.to_string())?;
                value
                    .set_repeat(gif::Repeat::Infinite)
                    .map_err(|error| error.to_string())?;
                encoder = Some(value);
            }
            let Some(encoder) = encoder.as_mut() else {
                return Err("GIF encoder unavailable".into());
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
            encoder
                .write_frame(&frame)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
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
        gif_fps: u32,
        gif_max_edge: u32,
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
            gif_fps,
            gif_max_edge,
            appended: 0,
            paused: false,
            pause_anchor: None,
            pts_offset: unsafe { CMTime::new(0, 1) },
        })
    }
}

impl Backend {
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
            Some(view.window()?.windowNumber() as u32)
        })
    }

    fn setup(
        content: *mut SCShareableContent,
        window_id: u32,
        request: &OpenRequest,
        completion: &Completion,
    ) {
        if content.is_null() {
            completion.opened(Opened {
                attempt: request.attempt,
                handle: None,
                result: Err("SCShareableContent unavailable".into()),
            });
            return;
        }
        let content = unsafe { &*content };
        let windows = unsafe { content.windows() };
        let Some(window) = windows
            .iter()
            .find(|window| unsafe { window.windowID() } == window_id)
        else {
            completion.opened(Opened {
                attempt: request.attempt,
                handle: None,
                result: Err("vmux window not shareable".into()),
            });
            return;
        };
        let filter = unsafe {
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
        };
        let config = Self::configuration(request);
        let state = match RecordingState::prepare(request, completion.clone()) {
            Ok(state) => state,
            Err(error) => {
                completion.opened(Opened {
                    attempt: request.attempt,
                    handle: None,
                    result: Err(error),
                });
                return;
            }
        };
        state.attach(&filter, &config);
    }

    fn configuration(request: &OpenRequest) -> Retained<SCStreamConfiguration> {
        let config = unsafe { SCStreamConfiguration::new() };
        unsafe {
            config.setWidth(request.width as usize);
            config.setHeight(request.height as usize);
            config.setPixelFormat(PIXEL_FORMAT_BGRA);
            config.setMinimumFrameInterval(CMTime::new(1, request.frame_rate as i32));
            config.setShowsCursor(true);
            config.setQueueDepth(6);
            if let Some(crop) = request.crop {
                config.setSourceRect(CGRect::new(
                    CGPoint::new(crop.x as f64 / request.scale, crop.y as f64 / request.scale),
                    CGSize::new(crop.w as f64 / request.scale, crop.h as f64 / request.scale),
                ));
            }
        }
        config
    }
}

impl OpenCapture for Backend {
    fn open(&self, request: OpenRequest, completion: Completion) -> Result<(), String> {
        if !Self::supported() {
            return Err("recording requires macOS 14 or later".into());
        }
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            unsafe { CGRequestScreenCaptureAccess() };
            return Err(PERMISSION_MSG.into());
        }
        let window_id =
            Self::window_number(request.window).ok_or("cannot resolve native window")?;
        let handler = RcBlock::new(
            move |content: *mut SCShareableContent, _error: *mut NSError| {
                Self::setup(content, window_id, &request, &completion);
            },
        );
        unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
        Ok(())
    }
}

impl RecordingState {
    fn prepare(request: &OpenRequest, completion: Completion) -> Result<Arc<Self>, String> {
        let (gif_tx, gif_rx) = match &request.temp_gif {
            Some(_) => {
                let (tx, rx) = crossbeam_channel::bounded::<GifMsg>(8);
                (Some(tx), Some(rx))
            }
            None => (None, None),
        };
        let encode = EncodeState::new(
            &request.temp_mp4,
            request.width,
            request.height,
            request.bitrate,
            Instant::now(),
            gif_tx,
            request.gif_fps,
            request.gif_max_edge,
        )?;
        let gif_join = match (request.temp_gif.clone(), gif_rx) {
            (Some(path), Some(rx)) => {
                let fps = request.gif_fps;
                Some(std::thread::spawn(move || GifEncoder::run(path, rx, fps)))
            }
            _ => None,
        };
        Ok(Arc::new(Self {
            stream: Mutex::new(None),
            encode: Mutex::new(encode),
            gif_join: Mutex::new(gif_join),
            _queue: SendCell(DispatchQueue::new("ai.vmux.recording", None)),
            _delegate: Mutex::new(None),
            attempt: request.attempt,
            completion,
            finalizing: Mutex::new(false),
        }))
    }

    fn attach(self: Arc<Self>, filter: &SCContentFilter, configuration: &SCStreamConfiguration) {
        let delegate = StreamOutput::alloc().set_ivars((
            vmux_ecs::EntityTarget::new(self.attempt),
            Arc::downgrade(&self),
        ));
        let delegate: Retained<StreamOutput> = unsafe { msg_send![super(delegate), init] };
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                filter,
                configuration,
                None,
            )
        };
        if let Err(error) = unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*delegate),
                SCStreamOutputType::Screen,
                Some(&self._queue.0),
            )
        } {
            self.completion.opened(Opened {
                attempt: self.attempt,
                handle: Some(CaptureHandle::new(self.clone())),
                result: Err(format!("addStreamOutput failed: {error:?}")),
            });
            return;
        }
        *self.stream.lock().unwrap() = Some(SendCell(stream.clone()));
        *self._delegate.lock().unwrap() = Some(SendCell(delegate));
        let state = self.clone();
        let start_block = RcBlock::new(move |error: *mut NSError| {
            let result = if error.is_null() {
                Ok(state.encode.lock().unwrap().start)
            } else {
                let description = unsafe { (*error).localizedDescription() };
                Err(format!("startCapture failed: {description}"))
            };
            state.completion.opened(Opened {
                attempt: state.attempt,
                handle: Some(CaptureHandle::new(state.clone())),
                result,
            });
        });
        unsafe { stream.startCaptureWithCompletionHandler(Some(&start_block)) };
    }

    fn finish_writer(self: Arc<Self>, stream_error: Option<String>) {
        let appended = {
            let mut encode = self.encode.lock().unwrap();
            encode.gif_tx = None;
            if encode.appended > 0 {
                unsafe { encode.input.0.markAsFinished() };
            }
            encode.appended
        };
        let gif_error = match self.gif_join.lock().unwrap().take() {
            Some(join) => match join.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some("GIF encoder worker failed".into()),
            },
            None => None,
        };
        let writer = self.encode.lock().unwrap().writer.0.clone();
        if appended == 0 {
            unsafe { writer.cancelWriting() };
            self.deliver(Err(
                "recording captured no frames (grant Screen Recording permission and retry)".into(),
            ));
            return;
        }
        let state = self.clone();
        let observed_writer = writer.clone();
        let completion = RcBlock::new(move || {
            let result = match stream_error.as_ref().or(gif_error.as_ref()) {
                Some(error) => Err(error.clone()),
                None if unsafe { observed_writer.status() } == AVAssetWriterStatus::Completed => {
                    Ok(())
                }
                None => {
                    let error = unsafe { observed_writer.error() };
                    Err(match error {
                        Some(error) => {
                            format!("recording writer failed: {}", error.localizedDescription())
                        }
                        None => "recording writer did not complete".into(),
                    })
                }
            };
            state.deliver(result);
        });
        unsafe { writer.finishWritingWithCompletionHandler(&completion) };
    }

    fn deliver(&self, finish_result: Result<(), String>) {
        let result =
            finish_result.map(|()| self.encode.lock().unwrap().start.elapsed().as_millis() as u64);
        self.completion.encoded(Encoded {
            attempt: self.attempt,
            result,
        });
    }
}

impl CaptureControl for RecordingState {
    fn pause(&self) {
        self.encode.lock().unwrap().paused = true;
    }

    fn resume(&self) {
        self.encode.lock().unwrap().paused = false;
    }

    fn finish(self: Arc<Self>) {
        {
            let mut finalizing = self.finalizing.lock().unwrap();
            if *finalizing {
                return;
            }
            *finalizing = true;
        }
        let stream = self.stream.lock().unwrap().take();
        if let Some(stream) = stream {
            let state = self.clone();
            let completion = RcBlock::new(move |error: *mut NSError| {
                let error = if error.is_null() {
                    None
                } else {
                    Some(format!("stopCapture failed: {}", unsafe {
                        (*error).localizedDescription()
                    }))
                };
                state.clone().finish_writer(error);
            });
            unsafe { stream.0.stopCaptureWithCompletionHandler(Some(&completion)) };
        } else {
            self.finish_writer(None);
        }
    }
}
