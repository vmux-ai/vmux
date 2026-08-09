//! Pointer, scroll and click forwarding between the platform's native event
//! monitors and the offscreen layout webview, isolated by platform.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub use macos::{
    NativeLayoutPointerMoveResult, flush_native_layout_pointer_move, forward_native_layout_click,
    forward_native_layout_scroll, queue_native_layout_pointer_move,
};
#[cfg(target_os = "macos")]
pub(crate) use macos::{
    clear_native_layout_pointer_state, physical_cef_pointer_hit_rect,
    set_native_layout_mouse_presenter, set_native_layout_pointer_regions,
};

/// The layout webview as the native event monitors see it. Every operation is
/// implemented once per platform in a sibling module — exactly one of which is
/// compiled.
pub(crate) struct NativeLayout;
