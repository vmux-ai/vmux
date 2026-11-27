use crate::url::{IosVersion, SimulatorRoute};
use std::option::Option;
use std::path::PathBuf;
use std::process::Command;
use std::string::String;

#[derive(bevy::prelude::Component)]
pub struct Axe {
    path: PathBuf,
}

impl Axe {
    pub const BIN: &'static str = "axe";

    pub(super) fn at(path: PathBuf) -> Self {
        Self { path }
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
    pub(super) fn matches_route(&self, route: &SimulatorRoute) -> bool {
        let Some(version) = route.version() else {
            return true;
        };
        self.version.as_ref() == Some(version)
            && route.device_name().is_none_or(|name| name == self.name)
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
        super::device_driver::SimulatorDeviceDriver::pixel_size(self, axe)
    }

    pub fn screenshot(&self, axe: &Axe, path: &std::path::Path) -> Result<Vec<u8>, String> {
        super::device_driver::SimulatorDeviceDriver::screenshot(self, axe, path)
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
