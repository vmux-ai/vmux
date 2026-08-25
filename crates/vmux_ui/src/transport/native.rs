use dioxus::prelude::Signal;

use crate::transport::Host;
use crate::transport::PageHost;

impl Host {
    pub(crate) fn fallback() -> Option<&'static dyn PageHost> {
        None
    }

    pub(crate) fn schedule_listener_retry(_retry_tick: Signal<u32>, _current: u32) {}

    pub(crate) fn scroll_item_into_view(item_id: &str) -> bool {
        let _ = Host::with_installed(|host| host.scroll_element_into_view(item_id));
        true
    }

    pub(crate) fn set_root_radius(_radius: f32) {}

    pub(crate) fn set_root_language(_locale: &str, _direction: &str) {}
}
