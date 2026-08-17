//! The native frontend: the iOS app, and the desktop build that runs this crate's tests.
//!
//! Rust runs in the same process as the WebView, so the host installs its transport before the
//! first page mounts — there is nothing to wait for and nothing to retry. The page's root element
//! belongs to that host rather than to a `web_sys` document, so the writes CEF makes to the
//! document are no-ops here; `theme.css` already carries the defaults they would set.

use dioxus::prelude::Signal;

use crate::transport::Host;
use crate::transport::PageHost;

impl Host {
    /// No default. A native host installs its own transport, and a page served by nothing says so
    /// rather than emitting into a void.
    pub(crate) fn fallback() -> Option<&'static dyn PageHost> {
        None
    }

    pub(crate) fn schedule_listener_retry(_retry_tick: Signal<u32>, _current: u32) {}

    /// Ask the host to reveal the row, and report the request as satisfied.
    ///
    /// Always true, where the browser answers false for an element it cannot find. A host here
    /// reaches the document by queueing a script evaluated *after* the batch that rendered the row,
    /// so the element exists by the time the request runs and there is nothing to retry.
    pub(crate) fn scroll_item_into_view(item_id: &str) -> bool {
        let _ = Host::with_installed(|host| host.scroll_element_into_view(item_id));
        true
    }

    /// `ThemeEvent` is only ever sent by the CEF host (`vmux_browser`), so the radius never changes
    /// here.
    pub(crate) fn set_root_radius(_radius: f32) {}

    /// Locale still resolves — the signal [`crate::hooks::use_theme()`] returns and
    /// [`crate::i18n::Locale::direction`] are the contract — but a native host applies it to its own
    /// root element.
    pub(crate) fn set_root_language(_locale: &str, _direction: &str) {}
}
