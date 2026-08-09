//! OS clipboard read/write, isolated by platform.
//!
//! Uses absolute paths to system binaries (no `$PATH` lookup) to defend
//! against PATH-hijack on shared systems. Writes happen on a background
//! thread so the Bevy main thread never blocks on subprocess I/O.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other;

/// The system clipboard. Every operation is implemented once per platform in a
/// sibling module — exactly one of which is compiled.
struct Clipboard;

/// Asynchronously write `text` to the system clipboard. Returns immediately;
/// errors are logged.
pub fn write(text: String) {
    if text.is_empty() {
        return;
    }
    std::thread::spawn(move || Clipboard::write_blocking(&text));
}

/// Read text from the system clipboard, blocking. Returns None on any error.
pub fn read_blocking() -> Option<String> {
    Clipboard::read_text()
}

/// Whether the system clipboard currently holds PNG image data.
///
/// On ⌘V in a terminal this decides whether to forward `Ctrl+V` (`0x16`) so the
/// focused agent CLI grabs the image from the pasteboard itself, instead of a
/// text paste. Scoped to PNG so it stays consistent with [`read_image_png`] (the
/// Vibe/boot-draft paths extract PNG); returns `false` otherwise.
pub fn has_image() -> bool {
    Clipboard::has_png()
}

/// Read PNG image bytes from the system clipboard, if present.
///
/// Used for the Vibe fallback, which cannot read the pasteboard itself: vmux
/// writes these bytes to a temp file and pastes its path instead of `Ctrl+V`.
pub fn read_image_png() -> Option<Vec<u8>> {
    Clipboard::read_png()
}

/// Read TIFF image bytes from the system clipboard, if present.
pub fn read_image_tiff() -> Option<Vec<u8>> {
    Clipboard::read_tiff()
}

/// Absolute path of an image *file* on the clipboard (a copied file, e.g. a
/// saved screenshot), if any.
///
/// Distinct from [`has_image`], which reports raw image *data*. Agent CLIs
/// auto-detect an image path pasted as text, so this lets ⌘V attach a copied
/// image file without raw clipboard image data.
pub fn image_file_path() -> Option<String> {
    Clipboard::image_file_path()
}
