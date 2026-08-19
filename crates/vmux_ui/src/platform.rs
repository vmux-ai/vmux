//! Host facilities a page cannot get from `std` alone.
//!
//! These had two bodies each while pages were also compiled to wasm, where there is no reactor,
//! no system clock and no clipboard. Only the native body is left, but the seam stays: a page
//! asking for the machine should say so here rather than in a component.

/// Milliseconds since the Unix epoch.
///
/// Callers wanting testable logic should take the result as a parameter rather than calling this
/// inside the function under test.
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Resolve after `ms` milliseconds.
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
}

/// Put `text` on the system clipboard, reporting whether it landed.
///
/// Not a `PageHost` request: the clipboard is the machine's, not the document's, and routing it
/// through the page would additionally need `vmux://` to be a secure context before
/// `navigator.clipboard` would answer at all.
pub async fn copy_to_clipboard(text: String) -> bool {
    // `vmux_clipboard::write` hands the work to a thread and logs its own failures, so there is
    // no outcome to wait for.
    vmux_clipboard::write(text);
    true
}

/// A pseudo-random index below `len`, saturating to 0 for an empty range.
pub fn random_index(len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos() as usize)
        .unwrap_or_default();
    nanos % len
}
