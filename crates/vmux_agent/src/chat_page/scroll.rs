//! Keeping the transcript pinned while it grows.
//!
//! Both writes have to wait for layout: `scroll_height` is stale until the rows just appended have
//! been laid out, so moving immediately lands short. Hence the `requestAnimationFrame` and the
//! zero-delay timeout — they are the point of these functions, not incidental.
//!
//! This is the last of the page's DOM dependency. Porting it means a `MountedData` handle from the
//! container's `onmounted` in place of the id lookup, and awaiting `get_scroll_size` in place of
//! the deferral; `metrics` is the hard one, since its caller reads synchronously and cannot yield
//! without the splice racing the measurement.

use std::cell::Cell;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

fn chat_scroll_element() -> Option<web_sys::Element> {
    web_sys::window()?
        .document()?
        .get_element_by_id("chat-scroll")
}

thread_local! {
    static SCROLL_TO_BOTTOM_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Scroll height and current offset, read before older messages are spliced in.
pub fn metrics() -> Option<(i32, i32)> {
    chat_scroll_element().map(|element| (element.scroll_height(), element.scroll_top()))
}

pub fn to_bottom() {
    if SCROLL_TO_BOTTOM_PENDING.replace(true) {
        return;
    }
    let callback = Closure::once_into_js(move || {
        SCROLL_TO_BOTTOM_PENDING.set(false);
        if let Some(element) = chat_scroll_element() {
            element.set_scroll_top(element.scroll_height());
        }
    })
    .unchecked_into::<js_sys::Function>();
    if let Some(window) = web_sys::window()
        && window.request_animation_frame(&callback).is_ok()
    {
        return;
    }
    let _ = callback.call0(&JsValue::NULL);
}

/// Hold the reader's place after older messages are prepended above them.
pub fn restore(previous_height: i32, previous_top: i32) {
    let callback = Closure::once_into_js(move || {
        if let Some(element) = chat_scroll_element() {
            let added_height = element.scroll_height().saturating_sub(previous_height);
            element.set_scroll_top(previous_top.saturating_add(added_height));
        }
    })
    .unchecked_into::<js_sys::Function>();
    if let Some(window) = web_sys::window()
        && window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&callback, 0)
            .is_ok()
    {
        return;
    }
    let _ = callback.call0(&JsValue::NULL);
}
