use std::fs;
use std::path::Path;
use std::process::Stdio;

use super::device::{Axe, SimulatorDevice};

pub(super) struct SimulatorDeviceDriver;

impl SimulatorDeviceDriver {
    pub(super) fn pixel_size(device: &SimulatorDevice, axe: &Axe) -> Option<(u32, u32)> {
        let path = std::env::temp_dir().join(format!(
            "vmux-simulator-{}-{}.png",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let status = axe
            .command()
            .args(["screenshot", "--udid", &device.udid, "--output"])
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
        SimulatorDevice::png_size(&bytes.ok()?)
    }

    pub(super) fn screenshot(
        device: &SimulatorDevice,
        axe: &Axe,
        path: &Path,
    ) -> Result<Vec<u8>, String> {
        let output = axe
            .command()
            .args(["screenshot", "--udid", &device.udid, "--output"])
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
}
