//! The two host facilities the transcript needs: a timer and a source of randomness.
//!
//! CEF runs the UI as wasm with no tokio reactor; the native mobile host has no JS globals.
//! Neither difference belongs in the components, so both live here.

/// Resolve after `ms` milliseconds.
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
}

/// A pseudo-random index below `len`, saturating to 0 for an empty range.
#[cfg(target_arch = "wasm32")]
pub fn random_index(len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    ((js_sys::Math::random() * len as f64) as usize).min(len - 1)
}

#[cfg(not(target_arch = "wasm32"))]
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
