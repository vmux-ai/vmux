//! Claiming keyboard focus from a host that grants it late.

/// A claim on keyboard focus for one element, re-asserted until the host honours it.
///
/// CEF grants an off-screen browser keyboard focus a frame or more after the page mounts — by
/// which time the `autofocus` attribute has already been ignored, because the document was not
/// focused when it was parsed. Asking once is not enough and asking forever is a spin, so the
/// claim asks once a frame up to a bound and stops as soon as the document agrees.
///
/// Polling is the wrong shape and this should not be the final answer: CEF knows the moment it
/// grants focus and says so through `on_got_focus`, which the browser process currently uses only
/// to wake the loop. Routed to the page, this becomes one event and no retry at all. The reason
/// that is not a small change is `on_set_focus`, which returns 1 to *cancel* CEF focus so winit
/// keeps the macOS first responder and Bevy keeps the keyboard — so what "focused" means here is
/// already not what it means in a browser, and the replacement has to be tried against a running
/// app rather than reasoned into place.
///
/// This is a fact about the host, not about any page, which is why it lives here rather than in
/// the two pages that used to carry a copy of it.
#[derive(Clone, Copy)]
#[cfg_attr(not(web), allow(dead_code))]
pub struct FocusClaim {
    element_id: &'static str,
    caret: Caret,
}

/// Where to leave the caret once focus lands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    /// Wherever the host put it.
    AsIs,
    /// Past the last character.
    ToEnd,
}

impl FocusClaim {
    /// Claim focus for the element with this id.
    pub fn new(element_id: &'static str) -> Self {
        Self {
            element_id,
            caret: Caret::AsIs,
        }
    }

    /// Move the caret past the last character each time focus is re-asserted.
    pub fn caret_at_end(mut self) -> Self {
        self.caret = Caret::ToEnd;
        self
    }
}

#[cfg(web)]
mod imp {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::{JsCast, closure::Closure};

    use super::{Caret, FocusClaim};

    /// How many frames to re-assert the claim for before giving up.
    ///
    /// Scheduled with `requestAnimationFrame` rather than a timer, and the difference matters more
    /// than it looks: rAF stops when the page stops rendering, so a page nobody is looking at costs
    /// nothing, while a 16ms timer would keep firing on every background page and — for an
    /// off-screen browser, whose frames Bevy composites — wake the app loop to do it.
    const RETRY_FRAMES: u32 = 90;

    impl FocusClaim {
        /// Take focus now, then keep re-asserting until the document holds it. Concurrent claims
        /// on the same element share one bounded retry.
        pub fn request(self) {
            let Some(window) = web_sys::window() else {
                return;
            };
            if self.settle() || pending(&window, self.element_id) {
                return;
            }
            set_pending(&window, self.element_id, true);
            self.retry(window, RETRY_FRAMES);
        }

        fn retry(self, window: web_sys::Window, frames_left: u32) {
            let retry_window = window.clone();
            let callback = Closure::once(move || {
                if self.settle() || frames_left <= 1 {
                    set_pending(&retry_window, self.element_id, false);
                } else {
                    self.retry(retry_window, frames_left - 1);
                }
            });
            match window.request_animation_frame(callback.as_ref().unchecked_ref()) {
                Ok(_) => callback.forget(),
                Err(_) => set_pending(&window, self.element_id, false),
            }
        }

        /// Assert the claim once. True when the document genuinely holds focus and the element is
        /// the active one, which is the only state worth stopping on.
        fn settle(self) -> bool {
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return true;
            };
            let Some(element) = document.get_element_by_id(self.element_id) else {
                return false;
            };
            if !self.is_active(&document) {
                if let Some(focusable) = element.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = focusable.focus();
                }
                if self.caret == Caret::ToEnd {
                    Self::move_caret_to_end(&element);
                }
            }
            document.has_focus().unwrap_or(false) && self.is_active(&document)
        }

        fn is_active(self, document: &web_sys::Document) -> bool {
            let Some(active) = document.active_element() else {
                return false;
            };
            active.id() == self.element_id
        }

        /// `set_selection_range` counts UTF-16 code units, so a byte length would land the caret
        /// mid-character in any draft that is not pure ASCII.
        fn move_caret_to_end(element: &web_sys::Element) {
            if let Some(textarea) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                let end = textarea.value().encode_utf16().count() as u32;
                let _ = textarea.set_selection_range(end, end);
            } else if let Some(input) = element.dyn_ref::<web_sys::HtmlInputElement>() {
                let end = input.value().encode_utf16().count() as u32;
                let _ = input.set_selection_range(end, end);
            }
        }
    }

    /// The latch lives on `window` rather than in a Rust static because a scheduled retry outlives
    /// the wasm instance that started it: a document reused for another page has to see the chain
    /// the previous one left running.
    fn pending(window: &web_sys::Window, element_id: &str) -> bool {
        js_sys::Reflect::get(window, &key(element_id))
            .map(|value| value.is_truthy())
            .unwrap_or(false)
    }

    fn set_pending(window: &web_sys::Window, element_id: &str, value: bool) {
        let _ = js_sys::Reflect::set(window, &key(element_id), &JsValue::from_bool(value));
    }

    fn key(element_id: &str) -> JsValue {
        JsValue::from_str(&format!("_vmuxFocusPending:{element_id}"))
    }
}

#[cfg(not(web))]
impl FocusClaim {
    /// Inert: no host is taking the caret away, so nothing has to take it back.
    pub fn request(self) {}
}
