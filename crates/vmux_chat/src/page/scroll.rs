//! Keeping the transcript pinned while it grows.
//!
//! Both writes have to wait for layout: `scroll_height` is stale until the rows just appended have
//! been laid out, so moving immediately lands short. CEF defers through `requestAnimationFrame`
//! and reads the element back; a native host has no document to look up and instead awaits a
//! measurement on the container, which serialises behind the same paint.

use dioxus::prelude::*;
use std::rc::Rc;

/// The transcript's scroll container, published by its `onmounted`.
///
/// CEF finds the element by id and ignores this; a native host has nothing else to hold on to.
pub type Container = Signal<Option<Rc<MountedData>>>;

#[cfg(web)]
mod imp {
    use super::Container;
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

    pub fn metrics(_container: Container) -> Option<(i32, i32)> {
        chat_scroll_element().map(|element| (element.scroll_height(), element.scroll_top()))
    }

    pub fn to_bottom(_container: Container) {
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

    pub fn restore(_container: Container, previous_height: i32, previous_top: i32) {
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
}

#[cfg(not(web))]
mod imp {
    use super::Container;
    use dioxus::html::geometry::PixelsVector2D;
    use dioxus::prelude::*;

    /// Measuring has to be awaited, but the caller reads this immediately before splicing older
    /// messages in and cannot yield without the splice racing the measurement. `None` skips the
    /// restore — which costs nothing today, because paging older history is desktop-only.
    pub fn metrics(_container: Container) -> Option<(i32, i32)> {
        None
    }

    pub fn to_bottom(container: Container) {
        spawn(async move {
            let Some(element) = container.peek().clone() else {
                return;
            };
            let Ok(size) = element.get_scroll_size().await else {
                return;
            };
            let _ = element
                .scroll(
                    PixelsVector2D::new(0.0, size.height),
                    ScrollBehavior::Instant,
                )
                .await;
        });
    }

    /// Unreachable while `metrics` returns `None`.
    pub fn restore(_container: Container, _previous_height: i32, _previous_top: i32) {}
}

pub use imp::{metrics, restore, to_bottom};
