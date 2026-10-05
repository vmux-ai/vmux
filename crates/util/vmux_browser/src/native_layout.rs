pub struct NativeLayout;

impl NativeLayout {
    pub fn pointer_is_inside() -> bool {
        crate::host::NATIVE_LAYOUT_POINTER_INSIDE.load(std::sync::atomic::Ordering::Relaxed)
    }
}
