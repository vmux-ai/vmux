//! Pointer, scroll and click forwarding between the platform's native event
//! monitors and the offscreen layout webview, isolated by platform.

/// The layout webview as the native event monitors see it. Every platform-specific
/// operation is implemented once per platform in a sibling module — exactly one of
/// which is compiled.
pub struct NativeLayout;

impl NativeLayout {
    /// Whether the pointer sits over a region the layout webview owns.
    pub fn pointer_is_inside() -> bool {
        crate::NATIVE_LAYOUT_POINTER_INSIDE.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub use macos::NativeLayoutPointerMoveResult;
