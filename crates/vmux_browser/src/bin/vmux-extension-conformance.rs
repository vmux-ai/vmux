use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use vmux_core::extension::{crx, store};

const CHROMIUM_MAJOR: u32 = 148;
const FIXTURE_MANIFEST: &str =
    include_str!("../../tests/fixtures/extension_conformance/manifest.json");
const FIXTURE_BACKGROUND: &str =
    include_str!("../../tests/fixtures/extension_conformance/background.js");
const FIXTURE_PUBLIC_KEY: &[u8] =
    include_bytes!("../../tests/fixtures/extension_conformance/test_public_key.der");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Observation {
    key: String,
    value: Value,
}

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
            let capture = capture(&target, &browser)?;
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

fn capture(target: &str, browser: &Path) -> Result<Capture, String> {
    if target == "chrome" {
        verify_chromium_major(browser)?;
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let collector = format!(
        "http://{}/capture",
        listener.local_addr().map_err(|error| error.to_string())?
    );
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let extension = prepare_fixture(temp.path(), target, &collector)?;
    let extension_id = crx::extension_id_from_key(FIXTURE_PUBLIC_KEY);
    let mut command = Command::new(browser);
    if target == "chrome" {
        let profile = temp.path().join("chrome-profile");
        command.args([
            format!("--user-data-dir={}", profile.display()),
            format!("--load-extension={}", extension.display()),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "about:blank".into(),
        ]);
    } else {
        let home = temp.path().join("home");
        install_vmux_fixture(&home, &extension, &extension_id)?;
        command
            .env("VMUX_EXTENSION_CONFORMANCE", "1")
            .env("HOME", home)
            .env("VMUX_PROFILE", "extension-conformance");
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
    let captures = collect_captures(listener, child, expected, timeout, &stderr_path)?;
    merge_captures(target, captures)
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
    let source = store::source_dir(&root, extension_id, "1.0.0");
    copy_tree(extension, &source)?;
    let source_hash = store::tree_sha256(&source)?;
    store::Index {
        entries: vec![store::ExtEntry {
            id: extension_id.into(),
            name: "vmux extension conformance".into(),
            version: "1.0.0".into(),
            popup: None,
            icon: None,
            enabled: true,
            source_hash,
            public_key_b64: Some(
                base64::engine::general_purpose::STANDARD.encode(FIXTURE_PUBLIC_KEY),
            ),
        }],
    }
    .save(&root)
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
) -> Result<Vec<Capture>, String> {
    let started = Instant::now();
    let mut captures = Vec::new();
    let mut failure = None;
    while captures.len() < expected {
        match listener.accept() {
            Ok((stream, _)) => match read_capture(stream) {
                Ok(capture) => captures.push(capture),
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
                "capture timed out after {} seconds",
                timeout.as_secs()
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

fn read_capture(mut stream: TcpStream) -> Result<Capture, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let mut request = Vec::new();
    let mut buffer = [0u8; 8192];
    let (header_end, content_length) = loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("collector request ended before headers".into());
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = find_bytes(&request, b"\r\n\r\n") {
            let headers =
                std::str::from_utf8(&request[..header_end]).map_err(|error| error.to_string())?;
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .ok_or("collector request has no content-length")?;
            break (header_end + 4, content_length);
        }
    };
    while request.len() < header_end + content_length {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
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
}
