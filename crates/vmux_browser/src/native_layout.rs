//! Where the pointer sits relative to the layout page, for readers that run outside the
//! Bevy schedule.

/// The layout page as the platform's native event monitors see it.
pub struct NativeLayout;

impl NativeLayout {
    /// Whether the pointer sits over a region the layout page owns.
    pub fn pointer_is_inside() -> bool {
        crate::NATIVE_LAYOUT_POINTER_INSIDE.load(std::sync::atomic::Ordering::Relaxed)
    }
}
