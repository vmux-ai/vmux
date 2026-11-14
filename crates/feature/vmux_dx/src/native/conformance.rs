use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use super::Native;
use crate::Result;

impl Native {
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
