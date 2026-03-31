//! `document::eval` receive loop (payload type chosen by the UI — JSON, RON string, etc.).

use dioxus::prelude::*;
use serde::de::DeserializeOwned;
use std::cell::RefCell;
use std::rc::Rc;

/// Injects `script` via [`document::eval`] once, then forwards each deserialized message to `on_message`
/// until the eval channel closes.
///
/// `T` must match what the injected JS passes to `dioxus.send` (Dioxus deserializes from JSON on the wire;
/// e.g. a JSON string for RON text, or a JSON object for `serde_json::Value`).
///
/// The callback is [`FnMut`] so handlers can update Dioxus signals (`Signal::set`).
pub fn use_eval_loop<T, F>(script: &'static str, on_message: F)
where
    T: DeserializeOwned + 'static,
    F: FnMut(T) + 'static,
{
    let handler = Rc::new(RefCell::new(on_message));
    use_effect(move || {
        let handler = handler.clone();
        spawn(async move {
            let mut eval = document::eval(script);
            loop {
                let Ok(msg) = eval.recv::<T>().await else {
                    break;
                };
                let mut h = handler.borrow_mut();
                h(msg);
            }
        });
    });
}
