use tracing::warn;

impl super::Clipboard {
    pub(super) fn write_blocking(_text: &str) {
        warn!("clipboard write not implemented on this platform");
    }

    pub fn read_text() -> Option<String> {
        None
    }

    pub fn has_image() -> bool {
        false
    }

    pub fn read_image_png() -> Option<Vec<u8>> {
        None
    }

    pub fn read_image_tiff() -> Option<Vec<u8>> {
        None
    }

    pub fn image_file_path() -> Option<String> {
        None
    }
}
