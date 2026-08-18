//! Platforms with no clipboard integration: writes are logged and dropped, and
//! every read comes back empty.

use tracing::warn;

impl super::Clipboard {
    pub(super) fn write_blocking(_text: &str) {
        warn!("clipboard write not implemented on this platform");
    }

    pub(super) fn read_text() -> Option<String> {
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
