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

/// Put `text` on the system clipboard, reporting whether it landed.
///
/// Not a `PageHost` request: the clipboard is the machine's, not the document's, and routing it
/// through the page would additionally need `vmux://` to be a secure context before
/// `navigator.clipboard` would answer at all.
#[cfg(web)]
pub async fn copy_to_clipboard(text: String) -> bool {
    use wasm_bindgen::{JsCast, JsValue};

    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(navigator) = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("navigator"))
    else {
        return false;
    };
    let Ok(clipboard) = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")) else {
        return false;
    };
    let Ok(write_text) = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .and_then(|function| function.dyn_into::<js_sys::Function>())
    else {
        return false;
    };
    let Ok(promise) = write_text
        .call1(&clipboard, &JsValue::from_str(&text))
        .and_then(|promise| promise.dyn_into::<js_sys::Promise>())
    else {
        return false;
    };
    wasm_bindgen_futures::JsFuture::from(promise).await.is_ok()
}

#[cfg(not(web))]
pub async fn copy_to_clipboard(text: String) -> bool {
    // `vmux_clipboard::write` hands the work to a thread and logs its own failures, so there is
    // no outcome to wait for.
    vmux_clipboard::write(text);
    true
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
