//! Host facilities a page cannot get from `std` alone.
//!
//! CEF runs the UI as wasm, where there is no reactor and no system clock; the native hosts have
//! no JS globals. Neither difference belongs in a component, so both live here, one pair of
//! bodies per capability.

/// Milliseconds since the Unix epoch.
///
/// `SystemTime::now` panics on `wasm32-unknown-unknown`, so the CEF page reads the clock through
/// JS. Callers wanting testable logic should take the result as a parameter rather than calling
/// this inside the function under test.
#[cfg(web)]
pub fn now_millis() -> i64 {
    js_sys::Date::now() as i64
}

#[cfg(not(web))]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Resolve after `ms` milliseconds.
#[cfg(web)]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(web))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
}

/// A pseudo-random index below `len`, saturating to 0 for an empty range.
#[cfg(web)]
pub fn random_index(len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    ((js_sys::Math::random() * len as f64) as usize).min(len - 1)
}

#[cfg(not(web))]
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
