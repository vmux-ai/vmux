#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other;

pub struct Clipboard;

impl Clipboard {
    pub fn write(text: String) {
        if text.is_empty() {
            return;
        }
        std::thread::spawn(move || Self::write_blocking(&text));
    }
}
