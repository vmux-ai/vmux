use crate::url::IosVersion;
use std::process::Command;

/// The `axe` CLI, which injects HID events straight into the guest.
///
/// Host-window automation does not work here: Simulator.app reads the real HID stream, so
/// `CGEventPostToPid` is silently dropped and a global tap would need the window visible and
/// unobstructed. AXe links Xcode's private CoreSimulator/SimulatorKit and absorbs the
/// per-Xcode-version churn in those signatures.
pub struct Axe;

impl Axe {
    pub const BIN: &'static str = "axe";

    pub fn version() -> Option<String> {
        let output = Command::new(Self::BIN).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn command() -> Command {
        Command::new(Self::BIN)
    }

    /// Waits off-thread: every gesture and keystroke costs a process spawn, and blocking on it
    /// would stall the frame that produced it.
    pub fn run_detached(mut command: Command) {
        std::thread::spawn(move || {
            let _ = command.status();
        });
    }
}

/// A booted simulator, identified the way AXe addresses it.
#[derive(bevy::prelude::Resource, Debug, Clone, PartialEq, Eq)]
pub struct SimulatorDevice {
    pub udid: String,
    pub name: String,
    pub version: Option<IosVersion>,
}

impl SimulatorDevice {
    pub fn booted() -> Option<Self> {
        Self::booted_matching(None)
    }

    /// The booted device on `want`, or any booted device when `want` is `None`.
    ///
    /// A URL pins a runtime, so a page opened on 27.0 must not silently mirror a 26.5 device
    /// that happens to also be booted.
    pub fn booted_matching(want: Option<&IosVersion>) -> Option<Self> {
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "booted", "-j"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Self::from_simctl_json(&output.stdout, want)
    }

    fn from_simctl_json(bytes: &[u8], want: Option<&IosVersion>) -> Option<Self> {
        let parsed: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let runtimes = parsed.get("devices")?.as_object()?;
        for (runtime, entries) in runtimes {
            let version = IosVersion::from_runtime_key(runtime);
            if let Some(want) = want
                && version.as_ref() != Some(want)
            {
                continue;
            }
            let Some(entries) = entries.as_array() else {
                continue;
            };
            for entry in entries {
                let udid = entry.get("udid").and_then(|v| v.as_str());
                let name = entry.get("name").and_then(|v| v.as_str());
                let (Some(udid), Some(name)) = (udid, name) else {
                    continue;
                };
                return Some(Self {
                    udid: udid.to_string(),
                    name: name.to_string(),
                    version,
                });
            }
        }
        None
    }

    /// Logical point size of the display, which is the space `axe tap` addresses.
    ///
    /// Read from the accessibility root rather than assumed from the device name: the stream
    /// reports pixels and the two differ by the device scale (3x on this phone, 2x on iPads).
    pub fn point_size(&self) -> Option<(f32, f32)> {
        let output = Axe::command()
            .args(["describe-ui", "--udid", &self.udid])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Self::root_frame_size(&output.stdout)
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
        let device = SimulatorDevice::from_simctl_json(TWO_RUNTIMES, None).expect("device");

        let version = device.version.expect("version");
        assert!(
            ["26.5", "27.0"].contains(&version.as_str()),
            "got {version}"
        );
    }

    #[test]
    fn a_pinned_version_selects_that_runtime_and_not_another_booted_one() {
        let want = IosVersion::parse("27.0").expect("version");

        let device = SimulatorDevice::from_simctl_json(TWO_RUNTIMES, Some(&want)).expect("device");

        assert_eq!(device.udid, "174D774A-1F21-455C-AB54-AF19D513988A");
        assert_eq!(device.name, "iPhone 17 Pro");
        assert_eq!(device.version.as_ref(), Some(&want));
    }

    #[test]
    fn a_pinned_version_with_nothing_booted_on_it_yields_nothing() {
        let want = IosVersion::parse("18.0").expect("version");

        assert_eq!(
            SimulatorDevice::from_simctl_json(TWO_RUNTIMES, Some(&want)),
            None
        );
    }

    #[test]
    fn no_booted_device_when_every_runtime_is_empty() {
        let json = br#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-27-0":[]}}"#;

        assert_eq!(SimulatorDevice::from_simctl_json(json, None), None);
    }

    #[test]
    fn malformed_output_does_not_panic() {
        assert_eq!(SimulatorDevice::from_simctl_json(b"not json", None), None);
        assert_eq!(SimulatorDevice::from_simctl_json(b"{}", None), None);
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
}
