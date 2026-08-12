//! Platforms with no native event monitor in front of the layout webview. Winit
//! delivers every pointer event, so nothing is ever forwarded out of band and the
//! layout never scrolls behind Bevy's back.

impl super::NativeLayout {
    pub(crate) fn last_scroll_at() -> Option<std::time::Instant> {
        None
    }
}
