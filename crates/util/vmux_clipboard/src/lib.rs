#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other;

struct Clipboard;

pub fn write(text: String) {
    if text.is_empty() {
        return;
    }
    std::thread::spawn(move || Clipboard::write_blocking(&text));
}

pub fn read_blocking() -> Option<String> {
    Clipboard::read_text()
}

pub fn has_image() -> bool {
    Clipboard::has_png()
}

pub fn read_image_png() -> Option<Vec<u8>> {
    Clipboard::read_png()
}

pub fn read_image_tiff() -> Option<Vec<u8>> {
    Clipboard::read_tiff()
}

pub fn image_file_path() -> Option<String> {
    Clipboard::image_file_path()
}
