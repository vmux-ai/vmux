use super::device::{Axe, SimulatorDevice};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Component)]
pub struct HidBroker {
    sender: Sender<HidRequest>,
}

impl HidBroker {
    pub fn start(axe: &Axe, device: &SimulatorDevice) -> io::Result<Self> {
        let client = HidBrokerClient::new(axe.path().to_path_buf(), device.udid.clone())?;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("vmux-simulator-input".into())
            .spawn(move || client.run(receiver))?;
        Ok(Self { sender })
    }

    pub fn dispatch(&self, request: HidRequest) {
        if self.sender.send(request).is_err() {
            error!("simulator input worker stopped");
        }
    }
}

pub struct HidRequest {
    primitives: Vec<HidPrimitive>,
    fallback: Vec<String>,
    coalescible: bool,
}

impl HidRequest {
    pub fn tap(point: (f32, f32)) -> Self {
        Self {
            primitives: vec![
                HidPrimitive::touch(HidKind::Down, point),
                HidPrimitive::delay(0.1),
                HidPrimitive::touch(HidKind::Up, point),
            ],
            fallback: vec![
                "tap".into(),
                "-x".into(),
                format!("{:.0}", point.0),
                "-y".into(),
                format!("{:.0}", point.1),
                "--tap-style".into(),
                "physical".into(),
            ],
            coalescible: false,
        }
    }

    pub fn triple_tap(point: (f32, f32)) -> Self {
        let mut primitives = Vec::with_capacity(11);
        for tap in 0..3 {
            primitives.push(HidPrimitive::touch(HidKind::Down, point));
            primitives.push(HidPrimitive::delay(0.05));
            primitives.push(HidPrimitive::touch(HidKind::Up, point));
            if tap < 2 {
                primitives.push(HidPrimitive::delay(0.05));
            }
        }
        let step = format!("tap -x {:.0} -y {:.0}", point.0, point.1);
        Self {
            primitives,
            fallback: vec![
                "batch".into(),
                "--tap-style".into(),
                "physical".into(),
                "--step".into(),
                step.clone(),
                "--step".into(),
                step.clone(),
                "--step".into(),
                step,
            ],
            coalescible: false,
        }
    }

    pub fn down(point: (f32, f32)) -> Self {
        Self {
            primitives: vec![HidPrimitive::touch(HidKind::Down, point)],
            fallback: vec![
                "touch".into(),
                "-x".into(),
                format!("{:.0}", point.0),
                "-y".into(),
                format!("{:.0}", point.1),
                "--down".into(),
            ],
            coalescible: false,
        }
    }

    pub fn move_to(point: (f32, f32)) -> Self {
        Self {
            primitives: vec![HidPrimitive::touch(HidKind::Down, point)],
            fallback: vec![
                "touch".into(),
                "-x".into(),
                format!("{:.0}", point.0),
                "-y".into(),
                format!("{:.0}", point.1),
                "--down".into(),
            ],
            coalescible: true,
        }
    }

    pub fn up(point: (f32, f32)) -> Self {
        Self {
            primitives: vec![HidPrimitive::touch(HidKind::Up, point)],
            fallback: vec![
                "touch".into(),
                "-x".into(),
                format!("{:.0}", point.0),
                "-y".into(),
                format!("{:.0}", point.1),
                "--up".into(),
            ],
            coalescible: false,
        }
    }

    pub fn swipe(from: (f32, f32), to: (f32, f32), duration_ms: u32) -> Self {
        let steps = (duration_ms / 16).clamp(2, 60);
        let delay = f64::from(duration_ms) / f64::from(steps) / 1_000.0;
        let mut primitives = Vec::with_capacity((steps as usize * 2) + 2);
        primitives.push(HidPrimitive::touch(HidKind::Down, from));
        for step in 1..=steps {
            let progress = step as f32 / steps as f32;
            let point = (
                from.0 + (to.0 - from.0) * progress,
                from.1 + (to.1 - from.1) * progress,
            );
            primitives.push(HidPrimitive::delay(delay));
            primitives.push(HidPrimitive::touch(HidKind::Down, point));
        }
        primitives.push(HidPrimitive::touch(HidKind::Up, to));
        Self {
            primitives,
            fallback: vec![
                "swipe".into(),
                "--start-x".into(),
                format!("{:.0}", from.0),
                "--start-y".into(),
                format!("{:.0}", from.1),
                "--end-x".into(),
                format!("{:.0}", to.0),
                "--end-y".into(),
                format!("{:.0}", to.1),
                "--duration".into(),
                format!("{:.3}", f64::from(duration_ms) / 1_000.0),
            ],
            coalescible: false,
        }
    }
}

#[derive(Serialize)]
struct HidBrokerRequest {
    primitives: Vec<HidPrimitive>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum HidKind {
    Down,
    Up,
    Delay,
}

#[derive(Serialize)]
struct HidPrimitive {
    kind: HidKind,
    x: Option<f64>,
    y: Option<f64>,
    duration: Option<f64>,
}

impl HidPrimitive {
    fn touch(kind: HidKind, point: (f32, f32)) -> Self {
        Self {
            kind,
            x: Some(point.0 as f64),
            y: Some(point.1 as f64),
            duration: None,
        }
    }

    fn delay(duration: f64) -> Self {
        Self {
            kind: HidKind::Delay,
            x: None,
            y: None,
            duration: Some(duration),
        }
    }
}

#[derive(Deserialize)]
struct HidBrokerHandshake {
    ready: bool,
}

#[derive(Deserialize)]
struct HidBrokerResponse {
    error: Option<String>,
}

struct HidBrokerClient {
    axe: PathBuf,
    udid: String,
    endpoint: PathBuf,
}

impl HidBrokerClient {
    const MAX_MESSAGE_BYTES: usize = 64 * 1024;
    const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

    fn new(axe: PathBuf, udid: String) -> io::Result<Self> {
        let developer = Self::developer_directory()?;
        let temporary = std::env::temp_dir();
        let uid = unsafe { libc::getuid() };
        let endpoint = Self::endpoint_path(&udid, &developer, &temporary, uid);
        Self::ensure_private_directory(
            endpoint
                .parent()
                .ok_or_else(|| io::Error::other("AXe HID endpoint has no directory"))?,
            uid,
        )?;
        Ok(Self {
            axe,
            udid,
            endpoint,
        })
    }

    fn run(self, receiver: Receiver<HidRequest>) {
        if let Err(error) = self.exchange(Vec::new()) {
            warn!("could not warm simulator input: {error}");
        }
        let mut pending = None;
        loop {
            let mut request = match pending.take() {
                Some(request) => request,
                None => match receiver.recv() {
                    Ok(request) => request,
                    Err(_) => return,
                },
            };
            if request.coalescible {
                while let Ok(next) = receiver.try_recv() {
                    if next.coalescible {
                        request = next;
                    } else {
                        pending = Some(next);
                        break;
                    }
                }
            }
            if let Err(error) = self.exchange(request.primitives) {
                warn!("fast simulator input failed: {error}");
                self.fallback(request.fallback);
            }
        }
    }

    fn exchange(&self, primitives: Vec<HidPrimitive>) -> io::Result<()> {
        let mut stream = self.ready_stream()?;
        let mut data = serde_json::to_vec(&HidBrokerRequest { primitives })?;
        data.push(b'\n');
        stream.write_all(&data)?;
        let response: HidBrokerResponse =
            serde_json::from_slice(&Self::read_message(&mut stream)?)?;
        if let Some(error) = response.error {
            return Err(io::Error::other(error));
        }
        Ok(())
    }

    fn ready_stream(&self) -> io::Result<UnixStream> {
        if let Ok(stream) = self.connect() {
            return Ok(stream);
        }
        let deadline = Instant::now() + Self::STARTUP_TIMEOUT;
        let _startup_lock = self.acquire_startup_lock(deadline)?;
        if let Ok(stream) = self.connect() {
            return Ok(stream);
        }
        let mut spawned = false;
        while Instant::now() < deadline {
            if !spawned && !self.broker_is_alive()? {
                self.remove_stale_endpoint()?;
                self.spawn();
                spawned = true;
            }
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(stream) = self.connect() {
                return Ok(stream);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "AXe HID broker did not become ready",
        ))
    }

    fn connect(&self) -> io::Result<UnixStream> {
        let mut stream = UnixStream::connect(&self.endpoint)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let handshake: HidBrokerHandshake =
            serde_json::from_slice(&Self::read_message(&mut stream)?)?;
        if !handshake.ready {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "AXe HID broker session is stale",
            ));
        }
        Ok(stream)
    }

    fn spawn(&self) {
        let mut command = std::process::Command::new(&self.axe);
        command
            .args(["hid-broker", "--udid", &self.udid])
            .env("AXE_HID_STABILIZATION_MS", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Axe::run_detached(command);
    }

    fn fallback(&self, arguments: Vec<String>) {
        let mut command = std::process::Command::new(&self.axe);
        command.args(arguments).args(["--udid", &self.udid]);
        match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => warn!("simulator input fallback failed: {status}"),
            Err(error) => warn!("simulator input fallback failed: {error}"),
        }
    }

    fn read_message(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut byte = [0u8; 1];
        while data.len() <= Self::MAX_MESSAGE_BYTES {
            let count = stream.read(&mut byte)?;
            if count == 0 {
                break;
            }
            if byte[0] == b'\n' {
                return Ok(data);
            }
            data.push(byte[0]);
        }
        let _ = stream.shutdown(Shutdown::Both);
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AXe HID broker message is missing or too large",
        ))
    }

    fn acquire_startup_lock(&self, deadline: Instant) -> io::Result<File> {
        let lock = Self::open_private_lock(&self.suffixed_endpoint(".lock"))?;
        loop {
            if Self::try_lock(&lock)? {
                return Ok(lock);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "AXe HID broker startup lock timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn broker_is_alive(&self) -> io::Result<bool> {
        let lock = Self::open_private_lock(&self.suffixed_endpoint(".lifetime.lock"))?;
        Ok(!Self::try_lock(&lock)?)
    }

    fn remove_stale_endpoint(&self) -> io::Result<()> {
        let metadata = match fs::symlink_metadata(&self.endpoint) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let uid = unsafe { libc::getuid() };
        if !metadata.file_type().is_socket() || metadata.uid() != uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to remove an unowned AXe HID endpoint",
            ));
        }
        fs::remove_file(&self.endpoint)
    }

    fn suffixed_endpoint(&self, suffix: &str) -> PathBuf {
        let mut path = self.endpoint.as_os_str().to_os_string();
        path.push(suffix);
        path.into()
    }

    fn open_private_lock(path: &Path) -> io::Result<File> {
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)?;
        let metadata = lock.metadata()?;
        let uid = unsafe { libc::getuid() };
        if !metadata.file_type().is_file() || metadata.uid() != uid || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "AXe HID lock is not a private owned file",
            ));
        }
        Ok(lock)
    }

    fn try_lock(lock: &File) -> io::Result<bool> {
        loop {
            let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(false);
            }
            return Err(error);
        }
    }

    fn ensure_private_directory(path: &Path, uid: u32) -> io::Result<()> {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "AXe HID directory is not private and owned",
            ));
        }
        Ok(())
    }

    fn developer_directory() -> io::Result<PathBuf> {
        if let Some(path) = std::env::var_os("DEVELOPER_DIR") {
            return fs::canonicalize(path);
        }
        let output = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("xcode-select could not resolve Xcode"));
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        fs::canonicalize(path)
    }

    fn endpoint_path(udid: &str, developer: &Path, temporary: &Path, uid: u32) -> PathBuf {
        let directory = temporary.join(format!("axe-hid-{uid}"));
        let simulator = Self::fnv1a64(udid);
        let developer = Self::fnv1a64(&developer.to_string_lossy());
        directory.join(format!("{simulator:x}-{developer:x}-v2.sock"))
    }

    fn fnv1a64(value: &str) -> u64 {
        let mut hash = 14_695_981_039_346_656_037u64;
        for byte in value.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_matches_the_axe_protocol() {
        let endpoint = HidBrokerClient::endpoint_path(
            "174D774A-1F21-455C-AB54-AF19D513988A",
            Path::new("/Applications/Xcode.app/Contents/Developer"),
            Path::new("/private/var/folders/example/T"),
            501,
        );

        assert_eq!(
            endpoint.file_name().and_then(|name| name.to_str()),
            Some("8924da6ea82aea1b-8987718aa6246ff8-v2.sock")
        );
    }

    #[test]
    fn live_touch_phases_match_the_broker_protocol() {
        let requests = [
            HidRequest::down((120.0, 240.0)),
            HidRequest::move_to((120.0, 180.0)),
            HidRequest::up((120.0, 180.0)),
        ];
        let kinds: Vec<_> = requests
            .into_iter()
            .map(|request| {
                let json = serde_json::to_value(HidBrokerRequest {
                    primitives: request.primitives,
                })
                .expect("request");
                json["primitives"][0]["kind"]
                    .as_str()
                    .expect("kind")
                    .to_string()
            })
            .collect();

        assert_eq!(kinds, ["down", "down", "up"]);
    }

    #[test]
    fn a_tap_holds_before_release() {
        let request = HidRequest::tap((120.0, 240.0));
        let json = serde_json::to_value(HidBrokerRequest {
            primitives: request.primitives,
        })
        .expect("request");
        let primitives = json["primitives"].as_array().expect("primitives");

        assert_eq!(primitives[0]["kind"], "down");
        assert_eq!(primitives[1]["kind"], "delay");
        assert_eq!(primitives[1]["duration"], 0.1);
        assert_eq!(primitives[2]["kind"], "up");
    }

    #[test]
    fn a_triple_tap_repeats_at_one_point() {
        let request = HidRequest::triple_tap((120.0, 240.0));
        let json = serde_json::to_value(HidBrokerRequest {
            primitives: request.primitives,
        })
        .expect("request");
        let primitives = json["primitives"].as_array().expect("primitives");

        assert_eq!(primitives.len(), 11);
        assert_eq!(
            primitives
                .iter()
                .filter(|primitive| primitive["kind"] == "down")
                .count(),
            3
        );
        assert_eq!(
            primitives
                .iter()
                .filter(|primitive| primitive["kind"] == "up")
                .count(),
            3
        );
        assert!(primitives.iter().all(|primitive| {
            primitive["kind"] == "delay" || (primitive["x"] == 120.0 && primitive["y"] == 240.0)
        }));
    }

    #[test]
    fn a_swipe_starts_and_ends_at_the_requested_points() {
        let request = HidRequest::swipe((12.0, 34.0), (56.0, 78.0), 300);
        let json = serde_json::to_value(HidBrokerRequest {
            primitives: request.primitives,
        })
        .expect("request");
        let primitives = json["primitives"].as_array().expect("primitives");

        assert_eq!(primitives.first().expect("first")["kind"], "down");
        assert_eq!(primitives.first().expect("first")["x"], 12.0);
        assert_eq!(primitives.last().expect("last")["kind"], "up");
        assert_eq!(primitives.last().expect("last")["x"], 56.0);
        assert!(
            primitives
                .iter()
                .any(|primitive| primitive["kind"] == "delay")
        );
    }
}
