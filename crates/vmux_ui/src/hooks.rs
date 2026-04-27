//! Dioxus hooks for CEF-embedded UIs (host emit, etc.).

pub mod event_listener;
mod use_theme;

#[allow(unused_imports)]
pub use event_listener::{
    BevyState, EventListenerError, try_cef_emit, try_cef_emit_serde, try_cef_listen,
    try_emit_ui_ready, use_event_listener, use_rkyv_event_listener,
};

pub use use_theme::use_theme;
