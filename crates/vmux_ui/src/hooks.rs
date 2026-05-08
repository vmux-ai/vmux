//! Dioxus hooks for CEF-embedded UIs (host emit, etc.).

pub mod event_listener;
mod use_theme;

#[allow(unused_imports)]
pub use event_listener::{
    BevyState, EventListenerError, decode_bin_host_emit_js, try_cef_bin_emit_rkyv,
    try_cef_bin_listen, try_cef_emit, try_cef_emit_serde, try_cef_listen, try_emit_ui_ready,
    use_bin_event_listener, use_event_listener,
};

pub use use_theme::use_theme;
