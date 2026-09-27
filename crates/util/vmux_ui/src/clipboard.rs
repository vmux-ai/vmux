pub struct Clipboard;

impl Clipboard {
    pub fn write(text: &str) {
        imp::write(text);
    }
}

mod imp {
    pub(super) fn write(text: &str) {
        crate::transport::Host::write_to_clipboard(text);
    }
}
