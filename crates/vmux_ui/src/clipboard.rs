//! Putting text on the system clipboard from a page.

/// The system clipboard, as a page reaches it.
///
/// Write-only, and deliberately: reading needs the user's permission on the web and gives a page
/// whatever the user last copied anywhere. Nothing in the UI wants that — every caller here is a
/// copy affordance the user pressed.
pub struct Clipboard;

impl Clipboard {
    /// Copy `text`, silently doing nothing where no clipboard is reachable.
    ///
    /// Silent because a copy that did not happen is a copy the user presses again; surfacing it
    /// would cost an error path in every transcript row to report something the pasteboard will
    /// make obvious.
    pub fn write(text: &str) {
        imp::write(text);
    }
}

/// The host's, because there is no document to ask and the pasteboard is the OS's.
mod imp {
    pub(super) fn write(text: &str) {
        crate::transport::Host::write_to_clipboard(text);
    }
}
