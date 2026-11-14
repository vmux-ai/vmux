use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use super::Native;
use crate::Result;
use crate::processes::Processes;

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

    pub fn reset_layout(&self) -> Result<()> {
        let base = if cfg!(target_os = "macos") {
            self.home()?.join("Library/Application Support/Vmux")
        } else {
            std::env::temp_dir().join("Vmux")
        };
        let profile = self
            .environment
            .get("VMUX_BUILD_PROFILE")
            .unwrap_or(OsStr::new("dev"));
        let path = PathBuf::from(self.get(
            "VMUX_DATA_DIR",
            if profile == "dev" {
                base.join("dev")
            } else {
                base.clone()
            },
        ));
        if !path.is_dir() {
            return Ok(());
        }
        if Processes::capture()?.uses(&path) {
            return Err("Layout files are in use; quit that build before resetting".into());
        }
        let suffix = self.text("date", &[OsStr::new("+%Y%m%d-%H%M%S")])?;
        for name in ["store.ron", "store.version"] {
            let source = path.join(name);
            if !source.is_file() {
                continue;
            }
            if profile != "dev" {
                fs::rename(
                    &source,
                    path.join(format!("{name}.cleanup-{}", suffix.trim())),
                )?;
            } else {
                fs::remove_file(source)?;
            }
        }
        let profiles = path.join("profiles");
        if profiles.is_dir() {
            for entry in fs::read_dir(profiles)? {
                let session = entry?.path().join("session.ron");
                if session.is_file() {
                    fs::remove_file(session)?;
                }
            }
        }
        if profile == "dev" && path.join("logs").is_dir() {
            fs::remove_dir_all(path.join("logs"))?;
        }
        println!("Layout reset: {}", path.display());
        Ok(())
    }

    pub fn conformance(&mut self) -> Result<()> {
        let chrome = self.environment.required("CHROME_BIN")?.to_owned();
        let default = self.checkout.directory("debug").join("vmux_desktop");
        let binary = PathBuf::from(self.get("VMUX_BIN", &default));
        if binary == default {
            self.cargo(
                &[
                    "build",
                    "-p",
                    "vmux_extension",
                    "-p",
                    "vmux_desktop",
                    "-p",
                    "vmux_app",
                    "--features",
                    "vmux_desktop/conformance,vmux_extension/conformance,vmux_app/service-daemon",
                ]
                .map(Into::into),
            )?;
        } else {
            self.environment
                .required("VMUX_CONFORMANCE_BUILD_PROFILE")?;
            self.cargo(
                &[
                    "build",
                    "-p",
                    "vmux_extension",
                    "--features",
                    "conformance",
                    "--bin",
                    "vmux-extension-conformance",
                ]
                .map(Into::into),
            )?;
        }
        let output = self.checkout.target.join("extension-conformance");
        fs::create_dir_all(&output)?;
        let harness = self
            .checkout
            .directory("debug")
            .join("vmux-extension-conformance");
        self.run_command(
            &harness,
            &[
                OsStr::new("capture"),
                OsStr::new("--target"),
                OsStr::new("chrome"),
                OsStr::new("--browser"),
                &chrome,
                OsStr::new("--output"),
                output.join("chrome.json").as_os_str(),
            ],
        )?;
        self.run_command(
            &harness,
            &[
                OsStr::new("capture"),
                OsStr::new("--target"),
                OsStr::new("vmux"),
                OsStr::new("--browser"),
                binary.as_os_str(),
                OsStr::new("--output"),
                output.join("vmux.json").as_os_str(),
            ],
        )?;
        self.run_command(
            &harness,
            &[
                OsStr::new("compare"),
                OsStr::new("--baseline"),
                output.join("chrome.json").as_os_str(),
                OsStr::new("--candidate"),
                output.join("vmux.json").as_os_str(),
            ],
        )
    }
}
