//! Linux clipboard: `wl-copy`/`wl-paste` under Wayland, falling back to `xclip`
//! under X11. Image data is not read back on this platform.

use tracing::warn;

impl super::Clipboard {
    pub(super) fn write_blocking(text: &str) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        // Try wl-copy first (Wayland), fall back to xclip (X11).
        let candidates: &[(&str, &[&str])] = &[
            ("/usr/bin/wl-copy", &[]),
            ("/usr/bin/xclip", &["-selection", "clipboard"]),
        ];
        for (bin, args) in candidates {
            if std::path::Path::new(bin).exists() {
                match Command::new(bin).args(*args).stdin(Stdio::piped()).spawn() {
                    Ok(mut child) => {
                        if let Some(stdin) = child.stdin.as_mut() {
                            let _ = stdin.write_all(text.as_bytes());
                        }
                        let _ = child.wait();
                        return;
                    }
                    Err(e) => warn!("{bin} failed: {e}"),
                }
            }
        }
        warn!("no clipboard helper found (need wl-copy or xclip)");
    }

    pub(super) fn read_text() -> Option<String> {
        use std::process::Command;
        let candidates: &[(&str, &[&str])] = &[
            ("/usr/bin/wl-paste", &[]),
            ("/usr/bin/xclip", &["-selection", "clipboard", "-o"]),
        ];
        for (bin, args) in candidates {
            if std::path::Path::new(bin).exists()
                && let Ok(output) = Command::new(bin).args(*args).output()
                && output.status.success()
            {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
        None
    }

    pub(super) fn has_png() -> bool {
        false
    }

    pub(super) fn read_png() -> Option<Vec<u8>> {
        None
    }

    pub(super) fn read_tiff() -> Option<Vec<u8>> {
        None
    }

    pub(super) fn image_file_path() -> Option<String> {
        None
    }
}
