use super::device::{Axe, SimulatorDevice};
use bevy::prelude::*;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Component)]
pub struct StreamServer {
    port: u16,
    capability: String,
    layout: BgraFrameLayout,
    stop: Arc<AtomicBool>,
    frames: Option<Arc<EncodedFrameExchange>>,
    capture: Option<Arc<Mutex<Child>>>,
    active: AtomicBool,
}

impl StreamServer {
    const FRAME_INTERVAL: Duration = Duration::from_millis(67);
    const JPEG_QUALITY: u8 = 70;
    const SCALE: &'static str = "0.3";

    pub fn start(
        axe: &Axe,
        device: SimulatorDevice,
        pixels: Option<(u32, u32)>,
    ) -> io::Result<Self> {
        let axe = axe.path().to_path_buf();
        let layout = BgraFrameLayout::of(pixels.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "simulator pixel size is unavailable",
            )
        })?)?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let capability = uuid::Uuid::new_v4().simple().to_string();
        let shared = Self::shared_stream(&axe, &device, layout);
        let stream = shared.as_ref().map(|(stream, _)| stream.clone());
        let frames = stream.as_ref().map(|stream| stream.frames.clone());
        let capture = shared.map(|(_, capture)| capture);
        let stop = Arc::new(AtomicBool::new(false));
        let listener_stop = stop.clone();
        let listener_capability = capability.clone();
        let listener_thread = std::thread::Builder::new()
            .name("vmux-simulator-stream".into())
            .spawn(move || Self::accept_loop(listener, stream, listener_capability, listener_stop));
        if let Err(error) = listener_thread {
            stop.store(true, Ordering::Release);
            if let Some(frames) = &frames {
                frames.close();
            }
            if let Some(capture) = &capture {
                Self::stop_capture(capture);
            }
            return Err(error);
        }
        Ok(Self {
            port,
            capability,
            layout,
            stop,
            frames,
            capture,
            active: AtomicBool::new(true),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn capability(&self) -> &str {
        &self.capability
    }

    pub fn frame_width(&self) -> u32 {
        self.layout.width
    }

    pub fn frame_height(&self) -> u32 {
        self.layout.height
    }

    pub fn frame_stride(&self) -> u32 {
        self.layout.stride
    }

    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, Ordering::AcqRel) == active {
            return;
        }
        let Some(capture) = &self.capture else {
            return;
        };
        let Ok(child) = capture.lock() else {
            return;
        };
        let Ok(pid) = i32::try_from(child.id()) else {
            return;
        };
        let signal = if active { libc::SIGCONT } else { libc::SIGSTOP };
        unsafe {
            libc::kill(pid, signal);
        }
    }

    fn accept_loop(
        listener: TcpListener,
        stream: Option<SharedStream>,
        capability: String,
        stop: Arc<AtomicBool>,
    ) {
        while !stop.load(Ordering::Acquire) {
            let socket = match listener.accept() {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Err(error) => {
                    warn!("simulator stream listener stopped: {error}");
                    return;
                }
            };
            if let Err(error) = socket.set_nonblocking(false) {
                warn!("simulator stream socket could not become blocking: {error}");
                continue;
            }
            let stream = stream.clone();
            let capability = capability.clone();
            let spawned = std::thread::Builder::new()
                .name("vmux-simulator-pipe".into())
                .spawn(move || Self::pipe(socket, stream, &capability));
            if spawned.is_err() {
                warn!("could not spawn a stream thread");
            }
        }
    }

    fn pipe(mut socket: TcpStream, stream: Option<SharedStream>, capability: &str) {
        let after = match Self::read_request(&mut socket, capability) {
            Ok(StreamRequest::Frames { after }) => after,
            Ok(StreamRequest::Preflight) => {
                let _ = Self::write_preflight(&mut socket);
                return;
            }
            Err(_) => return,
        };
        let Some(stream) = stream else {
            return;
        };
        Self::write_bgra(socket, stream, after);
    }

    fn shared_stream(
        axe: &PathBuf,
        device: &SimulatorDevice,
        layout: BgraFrameLayout,
    ) -> Option<(SharedStream, Arc<Mutex<Child>>)> {
        let (child, stdout) = Self::spawn_bgra(axe, device)?;
        let capture = Arc::new(Mutex::new(child));
        let frames = Arc::new(EncodedFrameExchange::default());
        let published = frames.clone();
        let reader_capture = capture.clone();
        let reader = std::thread::Builder::new()
            .name("vmux-simulator-capture".into())
            .spawn(move || {
                Self::read_bgra(stdout, published, layout, Self::FRAME_INTERVAL);
                Self::stop_capture(&reader_capture);
            });
        if reader.is_err() {
            frames.close();
            Self::stop_capture(&capture);
            return None;
        }
        Some((SharedStream { frames }, capture))
    }

    fn spawn_bgra(axe: &PathBuf, device: &SimulatorDevice) -> Option<(Child, ChildStdout)> {
        let mut child = std::process::Command::new(axe)
            .args(["stream-video", "--udid", &device.udid])
            .args(["--format", "bgra"])
            .args(["--scale", Self::SCALE])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        Some((child, stdout))
    }

    fn read_bgra(
        mut stdout: ChildStdout,
        frames: Arc<EncodedFrameExchange>,
        layout: BgraFrameLayout,
        interval: Duration,
    ) {
        let mut frame = vec![0; layout.frame_bytes];
        let mut published_at = Instant::now() - interval;
        loop {
            if stdout.read_exact(&mut frame).is_err() {
                frames.close();
                return;
            }
            let now = Instant::now();
            if now.duration_since(published_at) < interval {
                continue;
            }
            let Ok(encoded) = layout.jpeg(&frame, Self::JPEG_QUALITY) else {
                frames.close();
                return;
            };
            frames.replace(encoded);
            published_at = now;
        }
    }

    fn write_bgra(mut socket: TcpStream, stream: SharedStream, after: u64) {
        let _ = socket.set_nodelay(true);
        let Some((generation, frame)) = stream.frames.after(after) else {
            return;
        };
        if Self::write_header(&mut socket, frame.len(), generation).is_err() {
            return;
        }
        if let Err(error) = socket.write_all(&frame) {
            warn!("simulator frame write failed: {error}");
        }
    }

    fn read_request(socket: &mut TcpStream, capability: &str) -> io::Result<StreamRequest> {
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut request = Vec::with_capacity(1024);
        let mut chunk = [0; 1024];
        while request.len() < 8192 {
            let count = socket.read(&mut chunk)?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "simulator stream request ended early",
                ));
            }
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        socket.set_read_timeout(None)?;
        let Some(request) = StreamRequest::of(&request, capability) else {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulator stream capability is missing or invalid",
            ));
        };
        Ok(request)
    }

    #[cfg(test)]
    fn request_has_capability(request: &[u8], capability: &str) -> bool {
        StreamRequest::of(request, capability).is_some()
    }

    fn write_preflight(socket: &mut TcpStream) -> io::Result<()> {
        socket.write_all(
            b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
    }

    fn write_header(
        socket: &mut impl Write,
        frame_bytes: usize,
        generation: u64,
    ) -> io::Result<()> {
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Expose-Headers: X-Vmux-Generation\r\nCache-Control: no-store\r\nContent-Length: {frame_bytes}\r\nX-Vmux-Generation: {generation}\r\nConnection: close\r\n\r\n"
        )
    }

    fn stop_capture(capture: &Mutex<Child>) {
        let Ok(mut child) = capture.lock() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamRequest {
    Frames { after: u64 },
    Preflight,
}

impl StreamRequest {
    fn of(request: &[u8], capability: &str) -> Option<Self> {
        let expected_path = format!("/{capability}");
        let line = request.split(|byte| *byte == b'\n').next()?;
        let line = std::str::from_utf8(line).ok()?.trim_end_matches('\r');
        let mut parts = line.split_whitespace();
        let method = parts.next()?;
        let target = parts.next()?;
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if path != expected_path {
            return None;
        }
        match method {
            "GET" => Some(Self::Frames {
                after: Self::after_generation(query),
            }),
            "OPTIONS" => Some(Self::Preflight),
            _ => None,
        }
    }

    fn after_generation(query: &str) -> u64 {
        for field in query.split('&') {
            let Some(value) = field.strip_prefix("after=") else {
                continue;
            };
            if let Ok(generation) = value.parse() {
                return generation;
            }
        }
        0
    }
}

impl Drop for StreamServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(frames) = &self.frames {
            frames.close();
        }
        if let Some(capture) = &self.capture {
            Self::stop_capture(capture);
        }
    }
}

#[derive(Clone, Copy)]
struct BgraFrameLayout {
    width: u32,
    height: u32,
    stride: u32,
    frame_bytes: usize,
}

impl BgraFrameLayout {
    fn of((source_width, source_height): (u32, u32)) -> io::Result<Self> {
        let width = source_width.saturating_mul(3) / 10;
        let height = source_height.saturating_mul(3) / 10;
        let row_bytes = width.checked_mul(4).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "simulator frame width is invalid",
            )
        })?;
        let stride = row_bytes
            .checked_add(63)
            .map(|value| value & !63)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "simulator frame stride is invalid",
                )
            })?;
        let frame_bytes = stride
            .checked_mul(height)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "simulator frame size is invalid",
                )
            })?;
        if width == 0 || height == 0 || frame_bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "simulator frame size is empty",
            ));
        }
        Ok(Self {
            width,
            height,
            stride,
            frame_bytes,
        })
    }

    fn jpeg(self, bgra: &[u8], quality: u8) -> io::Result<Vec<u8>> {
        if bgra.len() != self.frame_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "simulator BGRA frame size is invalid",
            ));
        }
        let pixel_count = self
            .width
            .checked_mul(self.height)
            .and_then(|value| value.checked_mul(3))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "simulator RGB frame size is invalid",
                )
            })?;
        let mut rgb = Vec::with_capacity(pixel_count);
        let row_pixels = usize::try_from(self.width).unwrap_or_default();
        let stride = usize::try_from(self.stride).unwrap_or_default();
        for row in bgra.chunks_exact(stride) {
            for pixel in row[..row_pixels * 4].chunks_exact(4) {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(
                &rgb,
                self.width,
                self.height,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(io::Error::other)?;
        Ok(jpeg)
    }
}

#[derive(Clone)]
struct SharedStream {
    frames: Arc<EncodedFrameExchange>,
}

#[derive(Default)]
struct EncodedFrameExchange {
    state: Mutex<EncodedFrameState>,
    ready: Condvar,
}

impl EncodedFrameExchange {
    fn replace(&self, frame: Vec<u8>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        state.latest = Some(Arc::from(frame));
        state.generation = state.generation.wrapping_add(1).max(1);
        self.ready.notify_all();
    }

    fn after(&self, generation: u64) -> Option<(u64, Arc<[u8]>)> {
        let mut state = self.state.lock().ok()?;
        while state.generation == generation && !state.closed {
            state = self.ready.wait(state).ok()?;
        }
        if state.closed {
            return None;
        }
        Some((state.generation, state.latest.as_ref()?.clone()))
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            self.ready.notify_all();
        }
    }
}

#[derive(Default)]
struct EncodedFrameState {
    latest: Option<Arc<[u8]>>,
    generation: u64,
    closed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_layout_matches_core_video_stride_alignment() {
        let layout = BgraFrameLayout::of((1206, 2622)).unwrap();

        assert_eq!(layout.width, 361);
        assert_eq!(layout.height, 786);
        assert_eq!(layout.stride, 1472);
        assert_eq!(layout.frame_bytes, 1_156_992);
    }

    #[test]
    fn bgra_frames_encode_as_bounded_jpeg_images() {
        let layout = BgraFrameLayout::of((120, 240)).unwrap();
        let mut bgra = vec![0; layout.frame_bytes];
        for pixel in bgra.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[30, 20, 10, 255]);
        }

        let jpeg = layout.jpeg(&bgra, 75).unwrap();
        let decoded = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();

        assert_eq!(decoded.width(), layout.width);
        assert_eq!(decoded.height(), layout.height);
        assert!(jpeg.len() < layout.frame_bytes);
    }

    #[test]
    fn stream_requests_require_the_capability() {
        assert!(StreamServer::request_has_capability(
            b"GET /secret HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "secret"
        ));
        assert!(!StreamServer::request_has_capability(
            b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "secret"
        ));
        assert!(!StreamServer::request_has_capability(
            b"GET /secret-extra HTTP/1.1\r\nHost: localhost\r\n\r\n",
            "secret"
        ));
        assert_eq!(
            StreamRequest::of(
                b"OPTIONS /secret HTTP/1.1\r\nAccess-Control-Request-Private-Network: true\r\n\r\n",
                "secret"
            ),
            Some(StreamRequest::Preflight)
        );
    }

    #[test]
    fn encoded_frames_are_shared_between_stream_clients() {
        let frames = EncodedFrameExchange::default();
        frames.replace(vec![1, 2, 3]);

        let (_, first) = frames.after(0).expect("first client");
        let (_, second) = frames.after(0).expect("second client");

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_frame_response_is_bounded_so_webviews_do_not_buffer_forever() {
        let mut response = Vec::new();
        StreamServer::write_header(&mut response, 3, 7).unwrap();

        let response = String::from_utf8(response).unwrap();
        assert!(response.contains("Content-Length: 3\r\n"));
        assert!(response.contains("X-Vmux-Generation: 7\r\n"));
        assert!(!response.contains("Transfer-Encoding"));
    }

    #[test]
    fn a_frame_request_carries_the_generation_it_has_already_drawn() {
        assert_eq!(
            StreamRequest::of(b"GET /secret?after=42 HTTP/1.1\r\n\r\n", "secret"),
            Some(StreamRequest::Frames { after: 42 })
        );
    }
}
