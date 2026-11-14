use std::ffi::OsStr;

use super::Native;
use crate::Result;

impl Native {
    pub fn simulator(&self) -> Result<()> {
        let data: serde_json::Value = serde_json::from_slice(&self.output(
            "xcrun",
            &[
                OsStr::new("simctl"),
                OsStr::new("list"),
                OsStr::new("devices"),
                OsStr::new("available"),
                OsStr::new("-j"),
            ],
        )?)?;
        let runtimes = data
            .get("devices")
            .and_then(serde_json::Value::as_object)
            .ok_or("Missing simulator devices")?;
        let mut available = None;
        for devices in runtimes.values().rev() {
            for device in devices.as_array().ok_or("Invalid simulator device list")? {
                if !device
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.starts_with("iPhone"))
                {
                    continue;
                }
                if device.get("state").and_then(serde_json::Value::as_str) == Some("Booted") {
                    return Ok(());
                }
                if available.is_none()
                    && device
                        .get("isAvailable")
                        .and_then(serde_json::Value::as_bool)
                        != Some(false)
                {
                    available = device
                        .get("udid")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from);
                }
            }
        }
        let device = available.ok_or("No iPhone simulator is available; install one in Xcode")?;
        self.run_command(
            "xcrun",
            &[
                OsStr::new("simctl"),
                OsStr::new("boot"),
                OsStr::new(&device),
            ],
        )?;
        self.run_command(
            "xcrun",
            &[
                OsStr::new("simctl"),
                OsStr::new("bootstatus"),
                OsStr::new(&device),
                OsStr::new("-b"),
            ],
        )?;
        self.run_command(
            "open",
            &[
                OsStr::new("-a"),
                OsStr::new("Simulator"),
                OsStr::new("--args"),
                OsStr::new("-CurrentDeviceUDID"),
                OsStr::new(&device),
            ],
        )
    }
}
