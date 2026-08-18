//! Claiming keyboard focus from a host that grants it late.

/// A claim on keyboard focus for one element, honoured as soon as the host grants the document
/// focus.
///
/// CEF grants an off-screen browser keyboard focus a frame or more after the page mounts — by
/// which time the `autofocus` attribute has already been ignored, because the document was not
/// focused when it was parsed. So the claim asks once, and if the document does not yet have
/// focus to give, waits for the `focus` event that says it does.
///
/// Waiting on the event rather than re-asking every frame is possible because the two failures
/// are not really separate. Calling `focus()` makes the element `activeElement` there and then;
/// what lags is `document.hasFocus()`, which is the host's to grant and the host's to announce.
/// There is nothing else to wait for, so there is nothing to poll for.
///
/// The CEF side of this is worth knowing, because it is not what it looks like: `on_set_focus`
/// returns 1 to *cancel* CEF focus, so that winit keeps the macOS first responder and Bevy keeps
/// the keyboard. Focus reaches a page only through the host's own `set_focus` calls
/// (`sync_osr_focus_to_active_pane`), and Blink turns those into the ordinary window `focus`
/// event — which is why the page can listen for a plain DOM event and does not need CEF's
/// `on_got_focus` routed to it over IPC.
///
/// This is a fact about the host, not about any page, which is why it lives here rather than in
/// the two pages that used to carry a copy of it.
#[derive(Clone)]
#[cfg_attr(not(web), allow(dead_code))]
pub struct FocusClaim {
    /// Owned where it has to be: most ids are constants, but a row in a tree is named after the
    /// path it shows and there is no static string for that.
    element_id: std::borrow::Cow<'static, str>,
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
    pub fn new(element_id: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            element_id: element_id.into(),
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

    impl FocusClaim {
        /// Take focus now, or when the host next grants the document any. Concurrent claims on the
        /// same element share one wait.
        pub fn request(self) {
            let Some(window) = web_sys::window() else {
                return;
            };
            if self.settle() || pending(&window, &self.element_id) {
                return;
            }
            set_pending(&window, &self.element_id, true);
            self.wait_for_window_focus(&window);
        }

        /// Assert the claim again the next time the window gains focus, and keep waiting if that
        /// still was not enough.
        ///
        /// Nothing can arrive between the check that failed and this listener existing, because
        /// both are one synchronous run and a `focus` event has to queue behind it. The listener
        /// captures a copy of the claim and no signal, so — unlike a listener holding a
        /// component's state — there is nothing it can outlive.
        fn wait_for_window_focus(self, window: &web_sys::Window) {
            let claim = self.clone();
            let handler = Closure::once_into_js(move || {
                let Some(window) = web_sys::window() else {
                    return;
                };
                if claim.settle() {
                    set_pending(&window, &claim.element_id, false);
                } else {
                    claim.wait_for_window_focus(&window);
                }
            });
            let options = web_sys::AddEventListenerOptions::new();
            options.set_once(true);
            let target: &web_sys::EventTarget = window.as_ref();
            if target
                .add_event_listener_with_callback_and_add_event_listener_options(
                    "focus",
                    handler.unchecked_ref(),
                    &options,
                )
                .is_err()
            {
                set_pending(window, &self.element_id, false);
            }
        }

        /// Assert the claim once. True when the document genuinely holds focus and the element is
        /// the active one, which is the only state worth stopping on.
        fn settle(&self) -> bool {
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return true;
            };
            let Some(element) = document.get_element_by_id(&self.element_id) else {
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

        fn is_active(&self, document: &web_sys::Document) -> bool {
            let Some(active) = document.active_element() else {
                return false;
            };
            active.id() == self.element_id.as_ref()
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

    /// The latch lives on `window` rather than in a Rust static because a listener waiting on
    /// focus outlives the wasm instance that added it: a document reused for another page has to
    /// see the wait the previous one left running.
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
    /// Ask the installed host to focus the element.
    ///
    /// Not inert any more. This used to be, on the reasoning that no host takes the caret away —
    /// true of the phone, where the page is the whole app, and false of the desktop, which renders
    /// this page's components into a document it owns. A page that cannot claim focus there is a
    /// page that cannot be typed into.
    pub fn request(self) {
        crate::transport::Host::focus_element(&self.element_id);
    }
}
