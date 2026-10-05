use crate::url::{IosVersion, SimulatorRoute};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(bevy::prelude::Component)]
pub struct Axe {
    path: PathBuf,
}

impl Axe {
    pub const BIN: &'static str = "axe";

    const BREW_PATHS: [&'static str; 2] = ["/opt/homebrew/bin/axe", "/usr/local/bin/axe"];

    pub fn locate() -> Option<Self> {
        let mut candidates: Vec<PathBuf> = vec![PathBuf::from(Self::BIN)];
        candidates.extend(Self::BREW_PATHS.iter().map(PathBuf::from));
        candidates.extend(Self::bundled());
        for path in candidates {
            let probe = Command::new(&path).arg("--version").output();
            let Ok(output) = probe else {
                continue;
            };
            if output.status.success() {
                return Some(Self { path });
            }
        }
        None
    }

    fn bundled() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let contents = exe.parent()?.parent()?;
        let bundled = contents.join("Resources").join("axe").join(Self::BIN);
        bundled.exists().then_some(bundled)
    }

    pub fn version(&self) -> Option<String> {
        let output = Command::new(&self.path).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn command(&self) -> Command {
        Command::new(&self.path)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn run_detached(mut command: Command) {
        std::thread::spawn(move || {
            let command_debug = format!("{command:?}");
            match command.status() {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    bevy::log::error!("simulator command failed ({status}): {command_debug}")
                }
                Err(error) => {
                    bevy::log::error!("could not run simulator command: {error}: {command_debug}")
                }
            }
        });
    }
}

#[derive(bevy::prelude::Component, Debug, Clone, PartialEq, Eq)]
pub struct SimulatorDevice {
    pub udid: String,
    pub name: String,
    pub version: Option<IosVersion>,
}

impl SimulatorDevice {
    pub fn booted() -> Option<Self> {
        Self::booted_matching(None, None)
    }

    pub fn booted_matching(want: Option<&IosVersion>, device_name: Option<&str>) -> Option<Self> {
        Self::listed("booted", want, device_name).ok().flatten()
    }

    pub fn booted_or_boot(
        want: Option<&IosVersion>,
        device_name: Option<&str>,
    ) -> Result<Self, String> {
        if let Some(device) = Self::booted_matching(want, device_name) {
            return Ok(device);
        }
        let Some(device) = Self::listed("available", want, device_name)? else {
            let runtime = want
                .map(|version| format!(" for iOS {version}"))
                .unwrap_or_default();
            let model = device_name
                .map(|name| format!(" named {name}"))
                .unwrap_or_default();
            return Err(format!("no available iOS Simulator{model}{runtime}"));
        };
        device.boot()
    }

    pub(super) fn matches_route(&self, route: &SimulatorRoute) -> bool {
        match route {
            SimulatorRoute::Unpinned => true,
            SimulatorRoute::Pinned {
                version,
                device_name,
            } => {
                self.version.as_ref() == Some(version)
                    && device_name.as_deref().is_none_or(|name| name == self.name)
            }
        }
    }

    fn listed(
        scope: &str,
        want: Option<&IosVersion>,
        device_name: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", scope, "-j"])
            .output()
            .map_err(|error| format!("could not run simctl: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "simctl could not list {scope} devices: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(Self::from_simctl_json(&output.stdout, want, device_name))
    }

    fn boot(self) -> Result<Self, String> {
        bevy::log::info!("booting {} ({})", self.name, self.udid);
        let boot = Command::new("xcrun")
            .args(["simctl", "boot", &self.udid])
            .output()
            .map_err(|error| format!("could not run simctl boot: {error}"))?;
        let ready = Command::new("xcrun")
            .args(["simctl", "bootstatus", &self.udid, "-b"])
            .output()
            .map_err(|error| format!("could not run simctl bootstatus: {error}"))?;
        if !ready.status.success() {
            let boot_error = String::from_utf8_lossy(&boot.stderr);
            let ready_error = String::from_utf8_lossy(&ready.stderr);
            return Err(format!(
                "could not boot {}: {} {}",
                self.name,
                boot_error.trim(),
                ready_error.trim()
            ));
        }
        bevy::log::info!("booted {} ({})", self.name, self.udid);
        Ok(self)
    }

    fn from_simctl_json(
        bytes: &[u8],
        want: Option<&IosVersion>,
        device_name: Option<&str>,
    ) -> Option<Self> {
        let parsed: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let runtimes = parsed.get("devices")?.as_object()?;
        let mut selected: Option<Self> = None;
        for (runtime, entries) in runtimes {
            let Some(version) = IosVersion::from_runtime_key(runtime) else {
                continue;
            };
            if let Some(want) = want
                && &version != want
            {
                continue;
            }
            let Some(entries) = entries.as_array() else {
                continue;
            };
            for entry in entries {
                if entry.get("isAvailable").and_then(|value| value.as_bool()) == Some(false) {
                    continue;
                }
                let udid = entry.get("udid").and_then(|v| v.as_str());
                let name = entry.get("name").and_then(|v| v.as_str());
                let (Some(udid), Some(name)) = (udid, name) else {
                    continue;
                };
                if device_name.is_some_and(|wanted| wanted != name) {
                    continue;
                }
                let replace = match selected.as_ref().and_then(|device| device.version.as_ref()) {
                    Some(current) => version > *current,
                    None => true,
                };
                if replace {
                    selected = Some(Self {
                        udid: udid.to_string(),
                        name: name.to_string(),
                        version: Some(version.clone()),
                    });
                }
                break;
            }
        }
        selected
    }

    pub fn point_size(&self, axe: &Axe) -> Option<(f32, f32)> {
        let output = axe
            .command()
            .args(["describe-ui", "--udid", &self.udid])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Self::root_frame_size(&output.stdout)
    }

    pub fn pixel_size(&self, axe: &Axe) -> Option<(u32, u32)> {
        let path = std::env::temp_dir().join(format!(
            "vmux-simulator-{}-{}.png",
            std::process::id(),
            self.udid
        ));
        let status = axe
            .command()
            .args(["screenshot", "--udid", &self.udid, "--output"])
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            let _ = fs::remove_file(path);
            return None;
        }
        let bytes = fs::read(&path);
        let _ = fs::remove_file(path);
        Self::png_size(&bytes.ok()?)
    }

    pub fn screenshot(&self, axe: &Axe, path: &std::path::Path) -> Result<Vec<u8>, String> {
        let output = axe
            .command()
            .args(["screenshot", "--udid", &self.udid, "--output"])
            .arg(path)
            .output()
            .map_err(|error| format!("could not capture simulator screenshot: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "could not capture simulator screenshot: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        fs::read(path).map_err(|error| format!("could not read simulator screenshot: {error}"))
    }

    pub(crate) fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
        if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        (width > 0 && height > 0).then_some((width, height))
    }

    fn root_frame_size(bytes: &[u8]) -> Option<(f32, f32)> {
        let parsed: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let root = match &parsed {
            serde_json::Value::Array(items) => items.first()?,
            other => other,
        };
        let frame = root.get("frame")?;
        let width = frame.get("width")?.as_f64()? as f32;
        let height = frame.get("height")?.as_f64()? as f32;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        Some((width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_RUNTIMES: &[u8] = br#"{"devices":{
        "com.apple.CoreSimulator.SimRuntime.iOS-26-5":[
            {"udid":"AAAAAAAA-0000-0000-0000-000000000000","name":"iPhone 17","state":"Booted"}
        ],
        "com.apple.CoreSimulator.SimRuntime.iOS-27-0":[
            {"udid":"174D774A-1F21-455C-AB54-AF19D513988A","name":"iPhone 17 Pro","state":"Booted"}
        ]
    }}"#;

    #[test]
    fn carries_the_runtime_version_of_the_device_it_picks() {
        let device = SimulatorDevice::from_simctl_json(TWO_RUNTIMES, None, None).expect("device");

        assert_eq!(
            device.version.as_ref().map(IosVersion::as_str),
            Some("27.0")
        );
        assert_eq!(device.name, "iPhone 17 Pro");
    }

    #[test]
    fn a_pinned_version_selects_that_runtime_and_not_another_booted_one() {
        let want = IosVersion::parse("27.0").expect("version");

        let device =
            SimulatorDevice::from_simctl_json(TWO_RUNTIMES, Some(&want), None).expect("device");

        assert_eq!(device.udid, "174D774A-1F21-455C-AB54-AF19D513988A");
        assert_eq!(device.name, "iPhone 17 Pro");
        assert_eq!(device.version.as_ref(), Some(&want));
    }

    #[test]
    fn a_pinned_version_with_nothing_booted_on_it_yields_nothing() {
        let want = IosVersion::parse("18.0").expect("version");

        assert_eq!(
            SimulatorDevice::from_simctl_json(TWO_RUNTIMES, Some(&want), None),
            None
        );
    }

    #[test]
    fn no_booted_device_when_every_runtime_is_empty() {
        let json = br#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-27-0":[]}}"#;

        assert_eq!(SimulatorDevice::from_simctl_json(json, None, None), None);
    }

    #[test]
    fn malformed_output_does_not_panic() {
        assert_eq!(
            SimulatorDevice::from_simctl_json(b"not json", None, None),
            None
        );
        assert_eq!(SimulatorDevice::from_simctl_json(b"{}", None, None), None);
    }

    #[test]
    fn unavailable_and_non_ios_devices_are_ignored() {
        let json = br#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.watchOS-27-0":[
                {"udid":"WATCH","name":"Apple Watch"}
            ],
            "com.apple.CoreSimulator.SimRuntime.iOS-27-0":[
                {"udid":"UNAVAILABLE","name":"iPhone 17 Pro","isAvailable":false},
                {"udid":"AVAILABLE","name":"iPhone 17 Pro Max","isAvailable":true}
            ]
        }}"#;

        let device = SimulatorDevice::from_simctl_json(json, None, None).expect("device");

        assert_eq!(device.udid, "AVAILABLE");
        assert_eq!(device.name, "iPhone 17 Pro Max");
    }

    #[test]
    fn a_device_name_selects_that_model_within_the_runtime() {
        let json = br#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-27-0":[
                {"udid":"PHONE","name":"iPhone 17","isAvailable":true},
                {"udid":"PRO","name":"iPhone 17 Pro","isAvailable":true}
            ]
        }}"#;
        let version = IosVersion::parse("27.0").expect("version");

        let device = SimulatorDevice::from_simctl_json(json, Some(&version), Some("iPhone 17 Pro"))
            .expect("device");

        assert_eq!(device.udid, "PRO");
    }

    #[test]
    fn an_attachment_only_answers_for_its_requested_runtime_and_device() {
        let device = SimulatorDevice::from_simctl_json(TWO_RUNTIMES, None, None).expect("device");
        let version = IosVersion::parse("27.0").expect("version");
        let other_version = IosVersion::parse("26.5").expect("version");

        assert!(device.matches_route(&SimulatorRoute::Unpinned));
        assert!(device.matches_route(&SimulatorRoute::Pinned {
            version: version.clone(),
            device_name: None,
        }));
        assert!(device.matches_route(&SimulatorRoute::Pinned {
            version,
            device_name: Some("iPhone 17 Pro".into()),
        }));
        assert!(!device.matches_route(&SimulatorRoute::Pinned {
            version: other_version,
            device_name: None,
        }));
    }

    #[test]
    fn reads_point_size_off_the_accessibility_root() {
        let object = br#"{"AXLabel":"Settings","frame":{"x":0,"y":0,"width":402,"height":874}}"#;
        let array = br#"[{"frame":{"x":0,"y":0,"width":402,"height":874}}]"#;

        assert_eq!(
            SimulatorDevice::root_frame_size(object),
            Some((402.0, 874.0))
        );
        assert_eq!(
            SimulatorDevice::root_frame_size(array),
            Some((402.0, 874.0))
        );
    }

    #[test]
    fn an_empty_or_degenerate_accessibility_tree_has_no_point_size() {
        assert_eq!(SimulatorDevice::root_frame_size(b"[]"), None);
        assert_eq!(SimulatorDevice::root_frame_size(b"{}"), None);
        assert_eq!(
            SimulatorDevice::root_frame_size(br#"{"frame":{"width":0,"height":0}}"#),
            None
        );
    }

    #[test]
    fn reads_pixel_size_from_a_png_header() {
        let mut bytes = vec![0; 24];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&1206u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&2622u32.to_be_bytes());

        assert_eq!(SimulatorDevice::png_size(&bytes), Some((1206, 2622)));
        assert_eq!(SimulatorDevice::png_size(b"not png"), None);
    }
}
