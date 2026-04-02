//! Dioxus hooks for CEF-embedded UIs (host emit, etc.).

pub mod event_listener;

#[allow(unused_imports)]
pub use event_listener::{
    BevyState, EventListenerError, try_cef_emit, try_cef_emit_serde, try_cef_listen,
    try_emit_ui_ready, use_event_listener,
};
