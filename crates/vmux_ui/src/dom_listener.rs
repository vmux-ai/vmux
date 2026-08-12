//! Document listeners that end when the component that made them does.

/// A capture-phase listener on the document, removed when this value is dropped.
///
/// A listener handed to `Closure::forget` outlives the component whose signals it closes over, so
/// the first event after an unmount reads a dropped signal and takes the whole page down —
/// `ValueDroppedError` out of `dioxus-signals`, which reads as the page mysteriously going deaf.
/// Latching the install behind a flag stored on the document turns that from a leak into a
/// certainty: the component that remounts sees the flag, installs nothing, and the stale listener
/// holding the dead signal remains the only one there is.
///
/// Hold this in `use_hook` and the listener gets the component's lifetime, which is the one it
/// needed all along. Nothing else is required — no flag, no installed-yet signal, no effect,
/// because the document exists before the first render and the handler can look up whatever
/// element it cares about at event time.
#[cfg_attr(not(web), allow(dead_code))]
pub struct DocumentListener {
    #[cfg(web)]
    event: &'static str,
    #[cfg(web)]
    closure: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(web)]
impl DocumentListener {
    /// Listen for `event` on the document during the capture phase, so the handler runs before
    /// anything inside the page can stop it. `None` if there is no document to listen to.
    pub fn capture(
        event: &'static str,
        handler: impl FnMut(web_sys::Event) + 'static,
    ) -> Option<Self> {
        use wasm_bindgen::JsCast;

        let document = web_sys::window()?.document()?;
        let closure = wasm_bindgen::closure::Closure::wrap(
            Box::new(handler) as Box<dyn FnMut(web_sys::Event)>
        );
        let options = web_sys::AddEventListenerOptions::new();
        options.set_capture(true);
        document
            .add_event_listener_with_callback_and_add_event_listener_options(
                event,
                closure.as_ref().unchecked_ref(),
                &options,
            )
            .ok()?;
        Some(Self { event, closure })
    }
}

#[cfg(web)]
impl Drop for DocumentListener {
    /// The capture flag has to match the one the listener was added with. Remove it without and
    /// the browser looks for a bubble-phase registration, finds none, and leaves this one in
    /// place — the failure this type exists to prevent, reintroduced silently.
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;

        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let options = web_sys::EventListenerOptions::new();
        options.set_capture(true);
        let _ = document.remove_event_listener_with_callback_and_event_listener_options(
            self.event,
            self.closure.as_ref().unchecked_ref(),
            &options,
        );
    }
}

#[cfg(not(web))]
impl DocumentListener {
    /// Inert: there is no document off the browser, so there is nothing to listen to or clean up.
    pub fn capture(_event: &'static str, _handler: impl FnMut() + 'static) -> Option<Self> {
        None
    }
}
