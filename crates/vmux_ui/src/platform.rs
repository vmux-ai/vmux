//! Host facilities a page cannot get from `std` alone.

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
