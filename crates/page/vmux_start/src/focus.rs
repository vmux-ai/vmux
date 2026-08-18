//! Keeping the caret in the launcher input.
//!
//! Re-asserting the claim against a host that grants focus late is
//! [`vmux_ui::focus::FocusClaim`]'s job, not this module's. What is left here is the part that
//! genuinely is about the launcher: it is a page with nothing to interact with but one input and
//! a list of results, so a click anywhere else should not blur it, and the claim has to stop the
//! moment an agent page takes over the document.
//!
//! Split out so [`super::page::Page`] is just a page. Everything here is inert off the browser,
//! which is what lets the launcher render somewhere with no `window` to argue with.

/// The launcher's claim on the caret.
pub struct StartFocus;

#[cfg(web)]
mod imp {
    use vmux_ui::components::prompt_composer::{PROMPT_INPUT_ID, prompt_textarea};
    use vmux_ui::focus::FocusClaim;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;

    use super::StartFocus;

    const START_TRANSITIONED: &str = "_startTransitioned";
    const FOCUS_BOUND: &str = "_startFocusBound";
    const CLICK_BOUND: &str = "_startClickBound";

    impl StartFocus {
        /// Take the caret, unless an agent page has already claimed this document.
        pub fn request() {
            let Some(window) = web_sys::window() else {
                return;
            };
            if flag(&window, START_TRANSITIONED) {
                return;
            }
            FocusClaim::new(PROMPT_INPUT_ID).caret_at_end().request();
        }

        /// Bind the two listeners that re-assert the claim: one for when the window regains native
        /// focus, one for clicks that would otherwise blur the input.
        pub fn install() {
            Self::install_window_focus_refocus();
            Self::install_keep_input_focused_on_click();
        }

        /// Stop claiming the caret, because an agent page is replacing the launcher in this
        /// document.
        pub fn release_for_agent_transition() {
            let Some(window) = web_sys::window() else {
                return;
            };
            set_flag(&window, START_TRANSITIONED, true);
        }

        /// Clear the transition latch on mount, so a document reused for a new launcher claims the
        /// caret again.
        pub fn claim_on_mount() {
            let Some(window) = web_sys::window() else {
                return;
            };
            set_flag(&window, START_TRANSITIONED, false);
        }

        /// Refocus whenever this page's window regains native focus. Also covers switching back to
        /// an already-open launcher.
        fn install_window_focus_refocus() {
            let Some(window) = web_sys::window() else {
                return;
            };
            if flag(&window, FOCUS_BOUND) {
                return;
            }
            set_flag(&window, FOCUS_BOUND, true);

            let closure = Closure::wrap(Box::new(|| Self::request()) as Box<dyn FnMut()>);
            let target: &web_sys::EventTarget = window.as_ref();
            let _ =
                target.add_event_listener_with_callback("focus", closure.as_ref().unchecked_ref());
            closure.forget();
        }

        /// Keep the caret in the input no matter where the user clicks. The launcher has nothing to
        /// interact with but the input and the result rows, so a click on the hero background
        /// should never blur it. Cancelling the default focus shift on `mousedown` does not cancel
        /// the click, so selecting a result still works.
        fn install_keep_input_focused_on_click() {
            let Some(window) = web_sys::window() else {
                return;
            };
            if flag(&window, CLICK_BOUND) {
                return;
            }
            set_flag(&window, CLICK_BOUND, true);
            let Some(document) = window.document() else {
                return;
            };

            let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
                let Some(window) = web_sys::window() else {
                    return;
                };
                if flag(&window, START_TRANSITIONED) {
                    return;
                }
                let Some(input) = prompt_textarea(PROMPT_INPUT_ID) else {
                    return;
                };
                if let Some(element) = event
                    .target()
                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                {
                    let on_input = element
                        .closest(&format!("#{PROMPT_INPUT_ID}"))
                        .ok()
                        .flatten()
                        .is_some();
                    let on_results = element
                        .closest("#command-bar-results")
                        .ok()
                        .flatten()
                        .is_some();
                    if on_input || on_results {
                        return;
                    }
                }
                event.prevent_default();
                let _ = input.focus();
            }) as Box<dyn FnMut(web_sys::Event)>);
            let target: &web_sys::EventTarget = document.as_ref();
            let options = web_sys::AddEventListenerOptions::new();
            options.set_capture(true);
            let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
                "mousedown",
                closure.as_ref().unchecked_ref(),
                &options,
            );
            closure.forget();
        }
    }

    /// Latches live on `window` rather than in Rust statics because the listeners outlive any one
    /// mount, and a reused document has to see what the previous one bound.
    fn flag(window: &web_sys::Window, name: &str) -> bool {
        js_sys::Reflect::get(window, &JsValue::from_str(name))
            .map(|value| value.is_truthy())
            .unwrap_or(false)
    }

    fn set_flag(window: &web_sys::Window, name: &str, value: bool) {
        let _ = js_sys::Reflect::set(window, &JsValue::from_str(name), &JsValue::from_bool(value));
    }
}

#[cfg(not(web))]
impl StartFocus {
    /// Ask the host for the caret, which it gives by queueing a script against the field.
    pub fn request() {
        vmux_command::page::focus_prompt_input();
    }

    /// Inert. The web build installs document listeners that take the caret *back* from whatever
    /// the browser gave it to; nothing here takes it away in the first place.
    pub fn install() {}

    /// Inert, for the same reason as [`Self::install`]: there is no capture to release.
    pub fn release_for_agent_transition() {}

    pub fn claim_on_mount() {
        Self::request();
    }
}
