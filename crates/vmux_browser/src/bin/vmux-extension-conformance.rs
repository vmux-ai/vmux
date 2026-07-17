use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use vmux_core::extension::{crx, manifest, store};

const CHROMIUM_MAJOR: u32 = 148;
const CONFORMANCE_PROFILE: &str = "extension-conformance";
const MAX_CAPTURE_HEADER_BYTES: usize = 16 * 1024;
const MAX_CAPTURE_BODY_BYTES: usize = 1024 * 1024;
const FIXTURE_MANIFEST: &str =
    include_str!("../../tests/fixtures/extension_conformance/manifest.json");
const FIXTURE_BACKGROUND: &str =
    include_str!("../../tests/fixtures/extension_conformance/background.js");
const FIXTURE_ECHO_HTML: &str =
    include_str!("../../tests/fixtures/extension_conformance/echo.html");
const FIXTURE_ECHO_SOURCE: &str =
    include_str!("../../tests/fixtures/extension_conformance/echo.js");
const FIXTURE_PUBLIC_KEY: &[u8] =
    include_bytes!("../../tests/fixtures/extension_conformance/test_public_key.der");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Observation {
    key: String,
    value: Value,
}

#[cfg(test)]
impl Observation {
    fn new(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Capture {
    target: String,
    chromium_major: u32,
    observations: Vec<Observation>,
    #[serde(default)]
    internal_observations: Vec<Observation>,
}

impl Capture {
    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }
}

enum Action {
    Capture {
        target: String,
        browser: PathBuf,
        output: PathBuf,
    },
    Compare {
        baseline: PathBuf,
        candidate: PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    match parse_action(std::env::args().skip(1).collect())? {
        Action::Capture {
            target,
            browser,
            output,
        } => {
            let diagnostics = output.parent().unwrap_or_else(|| Path::new("."));
            let capture = capture(&target, &browser, diagnostics)?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(
                output,
                serde_json::to_string_pretty(&capture).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
        }
        Action::Compare {
            baseline,
            candidate,
        } => {
            let baseline = Capture::from_json(
                &std::fs::read_to_string(baseline).map_err(|error| error.to_string())?,
            )?;
            let candidate = Capture::from_json(
                &std::fs::read_to_string(candidate).map_err(|error| error.to_string())?,
            )?;
            compare_shared(&baseline, &candidate)
        }
    }
}

fn parse_action(args: Vec<String>) -> Result<Action, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage());
    };
    let values = parse_flags(&args[1..])?;
    match command {
        "capture" => {
            let target = required(&values, "target")?.to_string();
            if target != "chrome" && target != "vmux" {
                return Err("--target must be chrome or vmux".into());
            }
            Ok(Action::Capture {
                target,
                browser: required(&values, "browser")?.into(),
                output: required(&values, "output")?.into(),
            })
        }
        "compare" => Ok(Action::Compare {
            baseline: required(&values, "baseline")?.into(),
            candidate: required(&values, "candidate")?.into(),
        }),
        _ => Err(usage()),
    }
}

fn parse_flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
    if !args.len().is_multiple_of(2) {
        return Err(usage());
    }
    let mut values = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        let Some(key) = pair[0].strip_prefix("--") else {
            return Err(usage());
        };
        values.insert(key.into(), pair[1].clone());
    }
    Ok(values)
}

fn required<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn usage() -> String {
    "usage: vmux-extension-conformance capture --target <chrome|vmux> --browser <path> --output <path> | compare --baseline <path> --candidate <path>".into()
}

fn capture(target: &str, browser: &Path, diagnostics: &Path) -> Result<Capture, String> {
    if target == "chrome" {
        verify_chromium_major(browser)?;
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let collector_token = uuid::Uuid::new_v4().to_string();
    let collector = format!(
        "http://{}/capture?token={collector_token}",
        listener.local_addr().map_err(|error| error.to_string())?
    );
    let temp = tempfile::Builder::new()
        .prefix("vc")
        .tempdir_in("/tmp")
        .map_err(|error| error.to_string())?;
    let extension = prepare_fixture(temp.path(), target, &collector)?;
    let extension_id = crx::extension_id_from_key(FIXTURE_PUBLIC_KEY);
    let mut command = Command::new(browser);
    let child_home = temp.path().join("home");
    let child_tmp = child_home.join("tmp");
    std::fs::create_dir_all(&child_tmp).map_err(|error| error.to_string())?;
    command
        .env_clear()
        .env("HOME", &child_home)
        .env("TMPDIR", &child_tmp)
        .env("USER", "vmux-conformance")
        .env("LOGNAME", "vmux-conformance");
    for name in [
        "PATH",
        "LANG",
        "LC_ALL",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut vmux_home = None;
    if target == "chrome" {
        let profile = temp.path().join("chrome-profile");
        command.args([
            format!("--user-data-dir={}", profile.display()),
            format!("--load-extension={}", extension.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--use-mock-keychain".into(),
            format!("chrome-extension://{extension_id}/echo.html"),
        ]);
    } else {
        link_debug_cef_framework(&child_home)?;
        install_vmux_fixture(&child_home, &extension, &extension_id)?;
        command
            .env("VMUX_EXTENSION_CONFORMANCE", "1")
            .env("VMUX_EXTENSION_CONFORMANCE_ID", &extension_id)
            .env("VMUX_TEST", "1")
            .env("VMUX_PROFILE", CONFORMANCE_PROFILE);
        vmux_home = Some(child_home);
    }
    let stderr_path = temp.path().join("browser.stderr.log");
    let stderr_file = std::fs::File::create(&stderr_path).map_err(|error| error.to_string())?;
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));
    let child = command
        .spawn()
        .map_err(|error| format!("failed to launch browser {}: {error}", browser.display()))?;
    let expected = if target == "vmux" { 2 } else { 1 };
    let timeout = if target == "vmux" {
        Duration::from_secs(50)
    } else {
        Duration::from_secs(30)
    };
    let captures = collect_captures(
        listener,
        child,
        expected,
        timeout,
        &stderr_path,
        &collector_token,
    );
    let service_result = vmux_home.as_deref().map(terminate_vmux_service).transpose();
    let diagnostics_result =
        persist_diagnostics(target, diagnostics, &stderr_path, vmux_home.as_deref());
    let mut errors = Vec::new();
    let captures = match captures {
        Ok(captures) => Some(captures),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    if let Err(error) = service_result {
        errors.push(format!("service cleanup failed: {error}"));
    }
    if let Err(error) = diagnostics_result {
        errors.push(format!("diagnostics persistence failed: {error}"));
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let captures = captures.expect("captures exist when no errors were recorded");
    merge_captures(target, captures)
}

fn persist_diagnostics(
    target: &str,
    root: &Path,
    stderr_path: &Path,
    vmux_home: Option<&Path>,
) -> Result<(), String> {
    let destination = root.join("diagnostics").join(target);
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    secure_directory(&destination)?;
    copy_diagnostic_file(stderr_path, &destination.join("stderr.log"))?;
    if let Some(home) = vmux_home {
        let data = vmux_data_dir(home)?;
        let copied_data = destination.join(vmux_data_suffix()?);
        copy_diagnostic_directory(&data.join("logs"), &copied_data.join("logs"))?;
        copy_diagnostic_file(
            &data
                .join("profiles")
                .join(CONFORMANCE_PROFILE)
                .join("chrome_debug.log"),
            &copied_data
                .join("profiles")
                .join(CONFORMANCE_PROFILE)
                .join("chrome_debug.log"),
        )?;
    }
    Ok(())
}

fn vmux_data_suffix() -> Result<PathBuf, String> {
    Ok(match vmux_build_profile()?.as_str() {
        "release" | "local" => PathBuf::from("Vmux"),
        profile => PathBuf::from("Vmux").join(profile),
    })
}

fn vmux_build_profile() -> Result<String, String> {
    let profile = std::env::var("VMUX_CONFORMANCE_BUILD_PROFILE").unwrap_or_else(|_| "dev".into());
    if profile.is_empty()
        || !profile
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(format!("invalid vmux build profile: {profile:?}"));
    }
    Ok(profile)
}

#[cfg(target_os = "macos")]
fn vmux_data_dir(home: &Path) -> Result<PathBuf, String> {
    Ok(home
        .join("Library/Application Support")
        .join(vmux_data_suffix()?))
}

#[cfg(not(target_os = "macos"))]
fn vmux_data_dir(home: &Path) -> Result<PathBuf, String> {
    Ok(home.join("tmp").join(vmux_data_suffix()?))
}

#[cfg(unix)]
fn terminate_vmux_service(home: &Path) -> Result<(), String> {
    let pid_path = vmux_data_dir(home)?.join(format!(
        "services/vmux-{}-{CONFORMANCE_PROFILE}.pid",
        vmux_build_profile()?,
    ));
    let started = Instant::now();
    let pid = loop {
        match std::fs::read_to_string(&pid_path) {
            Ok(pid) => break pid,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && started.elapsed() < Duration::from_secs(5) =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        }
    };
    let pid = pid
        .trim()
        .parse::<i32>()
        .map_err(|error| format!("invalid vmux service PID: {error}"))?;
    if pid <= 1 {
        return Err(format!("invalid vmux service PID: {pid}"));
    }
    signal_process(pid, libc::SIGTERM)?;
    let started = Instant::now();
    while process_exists(pid) && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    if process_exists(pid) {
        signal_process(pid, libc::SIGKILL)?;
        let started = Instant::now();
        while process_exists(pid) && started.elapsed() < Duration::from_secs(1) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if process_exists(pid) {
        return Err(format!("vmux service {pid} did not exit after SIGKILL"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn terminate_vmux_service(_home: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn signal_process(pid: i32, signal: i32) -> Result<(), String> {
    if unsafe { libc::kill(pid, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error.to_string())
    }
}

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

fn copy_diagnostic_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let entries = match std::fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if file_type.is_file() {
            copy_diagnostic_file(&entry.path(), &destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn copy_diagnostic_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        secure_directory(parent)?;
    }
    match std::fs::copy(source, destination) {
        Ok(_) => secure_file(destination),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(unix)]
fn secure_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn secure_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn secure_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn link_debug_cef_framework(home: &Path) -> Result<(), String> {
    let real_home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let source =
        PathBuf::from(real_home).join(".local/share/Chromium Embedded Framework.framework");
    if !source.is_dir() {
        return Err(format!(
            "CEF framework not found at {}; run make setup-cef",
            source.display()
        ));
    }
    let destination = home.join(".local/share/Chromium Embedded Framework.framework");
    let parent = destination.parent().ok_or("CEF link has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(source, destination).map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn link_debug_cef_framework(_home: &Path) -> Result<(), String> {
    Ok(())
}

fn verify_chromium_major(browser: &Path) -> Result<(), String> {
    let output = Command::new(browser)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "browser --version failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout);
    let major = version
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| format!("could not parse Chromium version from {version:?}"))?;
    if major != CHROMIUM_MAJOR {
        return Err(format!(
            "expected Chromium {CHROMIUM_MAJOR}, found {major}: {}",
            version.trim()
        ));
    }
    Ok(())
}

fn prepare_fixture(root: &Path, target: &str, collector: &str) -> Result<PathBuf, String> {
    let extension = root.join("extension");
    std::fs::create_dir_all(&extension).map_err(|error| error.to_string())?;
    let key = base64::engine::general_purpose::STANDARD.encode(FIXTURE_PUBLIC_KEY);
    std::fs::write(
        extension.join("manifest.json"),
        FIXTURE_MANIFEST.replace("__VMUX_TEST_PUBLIC_KEY__", &key),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(extension.join("background.js"), FIXTURE_BACKGROUND)
        .map_err(|error| error.to_string())?;
    std::fs::write(extension.join("echo.html"), FIXTURE_ECHO_HTML)
        .map_err(|error| error.to_string())?;
    std::fs::write(extension.join("echo.js"), FIXTURE_ECHO_SOURCE)
        .map_err(|error| error.to_string())?;
    std::fs::write(
        extension.join("config.js"),
        format!(
            "globalThis.VMUX_CONFORMANCE = {{ target: {}, collector: {} }};\n",
            serde_json::to_string(target).map_err(|error| error.to_string())?,
            serde_json::to_string(collector).map_err(|error| error.to_string())?,
        ),
    )
    .map_err(|error| error.to_string())?;
    Ok(extension)
}

fn install_vmux_fixture(home: &Path, extension: &Path, extension_id: &str) -> Result<(), String> {
    let root = home.join(".vmux/extensions");
    let manifest_text = std::fs::read_to_string(extension.join("manifest.json"))
        .map_err(|error| error.to_string())?;
    let parsed = manifest::parse(&manifest_text)?;
    let source = store::source_dir(&root, extension_id, &parsed.version);
    copy_tree(extension, &source)?;
    let source_hash = store::tree_sha256(&source)?;
    let mut profile_enabled = std::collections::BTreeMap::new();
    profile_enabled.insert(CONFORMANCE_PROFILE.into(), true);
    let mut index = store::Index::default();
    let mut approved_grants = std::collections::BTreeMap::new();
    approved_grants.insert(
        CONFORMANCE_PROFILE.into(),
        store::ExtensionGrants {
            permissions: parsed.permissions.clone(),
            host_permissions: parsed.host_permissions.clone(),
        },
    );
    index.upsert(store::ExtEntry {
        id: extension_id.into(),
        name: manifest::resolve_name(extension, &parsed),
        version: parsed.version,
        popup: parsed.popup,
        icon: parsed.icon,
        enabled: false,
        profile_enabled,
        permissions: parsed.permissions,
        optional_permissions: parsed.optional_permissions,
        host_permissions: parsed.host_permissions,
        optional_host_permissions: parsed.optional_host_permissions,
        approved_grants,
        source_hash,
        public_key_b64: Some(base64::engine::general_purpose::STANDARD.encode(FIXTURE_PUBLIC_KEY)),
    });
    index.save(&root)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn collect_captures(
    listener: TcpListener,
    mut child: Child,
    expected: usize,
    timeout: Duration,
    stderr_path: &Path,
    collector_token: &str,
) -> Result<Vec<Capture>, String> {
    let started = Instant::now();
    let mut captures = Vec::new();
    let mut failure = None;
    while captures.len() < expected {
        match listener.accept() {
            Ok((stream, _)) => match read_capture(stream, collector_token, started + timeout) {
                Ok(capture) => {
                    if let Some(error) = capture
                        .internal_observations
                        .iter()
                        .find(|observation| observation.key == "worker.error")
                        .map(|observation| observation.value.to_string())
                    {
                        failure = Some(format!("extension worker failed: {error}"));
                        break;
                    }
                    captures.push(capture);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            failure = Some(format!("browser exited before capture completed: {status}"));
            break;
        }
        if started.elapsed() >= timeout {
            failure = Some(format!(
                "capture timed out after {} seconds with {} of {expected} captures",
                timeout.as_secs(),
                captures.len(),
            ));
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let (status, stderr) = terminate_child(&mut child, stderr_path)?;
    if let Some(error) = failure {
        return Err(format!(
            "{error}; child status: {status}; stderr:\n{stderr}"
        ));
    }
    Ok(captures)
}

fn terminate_child(child: &mut Child, stderr_path: &Path) -> Result<(ExitStatus, String), String> {
    if child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_none()
    {
        child.kill().map_err(|error| error.to_string())?;
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();
    Ok((status, stderr))
}

fn read_capture(
    mut stream: TcpStream,
    collector_token: &str,
    deadline: Instant,
) -> Result<Capture, String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 8192];
    let (header_end, content_length) = loop {
        let read = read_capture_chunk(&mut stream, &mut buffer, deadline)?;
        if read == 0 {
            return Err("collector request ended before headers".into());
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(metadata) = capture_request_metadata(&request, collector_token)? {
            break metadata;
        }
    };
    while request.len() < header_end + content_length {
        let read = read_capture_chunk(&mut stream, &mut buffer, deadline)?;
        if read == 0 {
            return Err("collector request ended before body".into());
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let capture = serde_json::from_slice(&request[header_end..header_end + content_length])
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .map_err(|error| error.to_string())?;
    Ok(capture)
}

fn read_capture_chunk(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<usize, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("collector request exceeded capture deadline".into());
    }
    stream
        .set_read_timeout(Some(remaining.min(Duration::from_secs(2))))
        .map_err(|error| error.to_string())?;
    stream.read(buffer).map_err(|error| error.to_string())
}

fn capture_request_metadata(
    request: &[u8],
    collector_token: &str,
) -> Result<Option<(usize, usize)>, String> {
    let Some(header_end) = find_bytes(request, b"\r\n\r\n") else {
        if request.len() > MAX_CAPTURE_HEADER_BYTES {
            return Err("collector request headers exceed size limit".into());
        }
        return Ok(None);
    };
    if header_end > MAX_CAPTURE_HEADER_BYTES {
        return Err("collector request headers exceed size limit".into());
    }
    let headers = std::str::from_utf8(&request[..header_end]).map_err(|error| error.to_string())?;
    let request_line = headers
        .lines()
        .next()
        .ok_or("collector request line is missing")?;
    let expected_request = format!("POST /capture?token={collector_token} HTTP/1.1");
    if request_line != expected_request {
        return Err("collector request is not authorized".into());
    }
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or("collector request has no content-length")?;
    if content_length > MAX_CAPTURE_BODY_BYTES {
        return Err("collector request body exceeds size limit".into());
    }
    Ok(Some((header_end + 4, content_length)))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn merge_captures(target: &str, captures: Vec<Capture>) -> Result<Capture, String> {
    let mut observations = BTreeMap::new();
    let mut internal = BTreeMap::new();
    for capture in captures {
        if capture.chromium_major != CHROMIUM_MAJOR {
            return Err(format!(
                "capture reported Chromium {}, expected {CHROMIUM_MAJOR}",
                capture.chromium_major
            ));
        }
        for observation in capture.observations {
            observations.insert(observation.key, observation.value);
        }
        for observation in capture.internal_observations {
            internal.insert(observation.key, observation.value);
        }
    }
    Ok(Capture {
        target: target.into(),
        chromium_major: CHROMIUM_MAJOR,
        observations: observations
            .into_iter()
            .map(|(key, value)| Observation { key, value })
            .collect(),
        internal_observations: internal
            .into_iter()
            .map(|(key, value)| Observation { key, value })
            .collect(),
    })
}

fn compare_shared(baseline: &Capture, candidate: &Capture) -> Result<(), String> {
    if baseline.chromium_major != candidate.chromium_major {
        return Err(format!(
            "Chromium major mismatch: baseline={}, candidate={}",
            baseline.chromium_major, candidate.chromium_major
        ));
    }
    let baseline = normalize_observations(&baseline.observations)?;
    let candidate = normalize_observations(&candidate.observations)?;
    if baseline == candidate {
        return Ok(());
    }
    let mut differences = Vec::new();
    let keys = baseline
        .keys()
        .chain(candidate.keys())
        .collect::<BTreeSet<_>>();
    for key in keys {
        if baseline.get(key) != candidate.get(key) {
            differences.push(format!(
                "{key}: baseline={}, candidate={}",
                baseline
                    .get(key)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<missing>".into()),
                candidate
                    .get(key)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<missing>".into())
            ));
        }
    }
    Err(format!(
        "shared extension observations differ:\n{}",
        differences.join("\n")
    ))
}

fn normalize_observations(observations: &[Observation]) -> Result<BTreeMap<String, Value>, String> {
    let mut normalized = BTreeMap::new();
    for observation in observations {
        if normalized.contains_key(&observation.key) {
            return Err(format!("duplicate observation key: {}", observation.key));
        }
        let value = if observation.key == "runtime.id" {
            Value::String("<extension-id>".into())
        } else {
            normalize_value(&observation.value, &observation.key)
        };
        normalized.insert(observation.key.clone(), value);
    }
    Ok(normalized)
}

fn normalize_value(value: &Value, path: &str) -> Value {
    match value {
        Value::Array(values) => {
            if path.ends_with("tabIds") || path.ends_with("tab_ids") {
                return Value::Array(values.iter().map(|_| Value::from(0)).collect());
            }
            Value::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| normalize_value(value, &format!("{path}[{index}]")))
                    .collect(),
            )
        }
        Value::Object(values) => {
            let tab_object = values.contains_key("url")
                && (values.contains_key("windowId") || values.contains_key("window_id"));
            Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        let normalized = if extension_id_key(key) {
                            Value::String("<extension-id>".into())
                        } else if timestamp_key(key) {
                            Value::String("<timestamp>".into())
                        } else if tab_id_key(key) || key == "id" && tab_object {
                            Value::from(0)
                        } else {
                            normalize_value(value, &format!("{path}.{key}"))
                        };
                        (key.clone(), normalized)
                    })
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

fn extension_id_key(key: &str) -> bool {
    matches!(key, "extension_id" | "extensionId")
}

fn tab_id_key(key: &str) -> bool {
    matches!(key, "tab_id" | "tabId")
}

fn timestamp_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("timestamp") || key.ends_with("_at")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_observations_match_after_normalization() {
        let chrome = Capture::from_json(include_str!(
            "../../tests/fixtures/extension_conformance/chromium-148-runtime.json"
        ))
        .unwrap();
        let vmux = Capture {
            target: "vmux".into(),
            chromium_major: 148,
            observations: vec![
                Observation::new("runtime.id.length", serde_json::json!(32)),
                Observation::new("storage.local.roundTrip", serde_json::json!("value")),
                Observation::new("runtime.message.roundTrip", serde_json::json!("pong")),
            ],
            internal_observations: vec![Observation::new(
                "bridge.connected",
                serde_json::json!(true),
            )],
        };
        compare_shared(&chrome, &vmux).unwrap();
    }

    #[test]
    fn normalization_ignores_only_dynamic_identity_fields() {
        let capture = |target: &str, extension_id: &str, tab_id: i64, timestamp: i64| Capture {
            target: target.into(),
            chromium_major: 148,
            observations: vec![
                Observation::new("runtime.id", serde_json::json!(extension_id)),
                Observation::new(
                    "tabs.snapshot",
                    serde_json::json!([{ "id": tab_id, "windowId": 1, "url": "https://example.com/", "updated_at": timestamp }]),
                ),
            ],
            internal_observations: Vec::new(),
        };
        compare_shared(
            &capture("chrome", "aaaaaaaa", 10, 100),
            &capture("vmux", "bbbbbbbb", 99, 200),
        )
        .unwrap();
    }

    #[test]
    fn mismatch_reports_observation_key() {
        let baseline = Capture {
            target: "chrome".into(),
            chromium_major: 148,
            observations: vec![Observation::new(
                "runtime.message",
                serde_json::json!("pong"),
            )],
            internal_observations: Vec::new(),
        };
        let candidate = Capture {
            target: "vmux".into(),
            chromium_major: 148,
            observations: vec![Observation::new(
                "runtime.message",
                serde_json::json!("wrong"),
            )],
            internal_observations: Vec::new(),
        };
        assert!(
            compare_shared(&baseline, &candidate)
                .unwrap_err()
                .contains("runtime.message")
        );
    }

    #[test]
    fn collector_requires_token_and_bounded_body() {
        let valid = b"POST /capture?token=secret HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}";
        let body_start = find_bytes(valid, b"\r\n\r\n").unwrap() + 4;
        assert_eq!(
            capture_request_metadata(valid, "secret").unwrap(),
            Some((body_start, 2))
        );
        assert!(capture_request_metadata(valid, "wrong").is_err());
        let oversized = format!(
            "POST /capture?token=secret HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_CAPTURE_BODY_BYTES + 1
        );
        assert!(capture_request_metadata(oversized.as_bytes(), "secret").is_err());
    }

    #[test]
    fn vmux_fixture_index_round_trips_profile_grants() {
        let temp = tempfile::tempdir().unwrap();
        let extension = prepare_fixture(temp.path(), "vmux", "http://127.0.0.1/capture").unwrap();
        let home = temp.path().join("home");
        let extension_id = crx::extension_id_from_key(FIXTURE_PUBLIC_KEY);

        install_vmux_fixture(&home, &extension, &extension_id).unwrap();

        let index = store::Index::load(&home.join(".vmux/extensions")).unwrap();
        let entry = &index.entries[0];
        assert_eq!(entry.id, extension_id);
        assert!(entry.enabled_for(CONFORMANCE_PROFILE));
        assert!(
            entry
                .grants_for(CONFORMANCE_PROFILE)
                .covers(&entry.permissions, &entry.host_permissions)
        );
    }
}
