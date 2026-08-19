//! Claiming keyboard focus from a host that grants it late.

/// A claim on keyboard focus for one element, honoured as soon as the host grants the document
/// focus.
///
/// CEF grants an off-screen browser keyboard focus a frame or more after the page mounts — by
/// which time the `autofocus` attribute has already been ignored, because the document was not
/// focused when it was parsed. So the claim asks once, and if the document does not yet have
/// focus to give, waits for the `focus` event that says it does.
///
/// Waiting on the event rather than re-asking every frame is possible because the two failures
/// are not really separate. Calling `focus()` makes the element `activeElement` there and then;
/// what lags is `document.hasFocus()`, which is the host's to grant and the host's to announce.
/// There is nothing else to wait for, so there is nothing to poll for.
///
/// The CEF side of this is worth knowing, because it is not what it looks like: `on_set_focus`
/// returns 1 to *cancel* CEF focus, so that winit keeps the macOS first responder and Bevy keeps
/// the keyboard. Focus reaches a page only through the host's own `set_focus` calls
/// (`sync_osr_focus_to_active_pane`), and Blink turns those into the ordinary window `focus`
/// event — which is why the page can listen for a plain DOM event and does not need CEF's
/// `on_got_focus` routed to it over IPC.
///
/// This is a fact about the host, not about any page, which is why it lives here rather than in
/// the two pages that used to carry a copy of it.
#[derive(Clone)]
pub struct FocusClaim {
    /// Owned where it has to be: most ids are constants, but a row in a tree is named after the
    /// path it shows and there is no static string for that.
    element_id: std::borrow::Cow<'static, str>,
    caret: Caret,
}

/// Where to leave the caret once focus lands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Caret {
    /// Wherever the host put it.
    AsIs,
    /// Past the last character.
    ToEnd,
}

impl FocusClaim {
    /// Claim focus for the element with this id.
    pub fn new(element_id: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            element_id: element_id.into(),
            caret: Caret::AsIs,
        }
    }

    /// Move the caret past the last character each time focus is re-asserted.
    pub fn caret_at_end(mut self) -> Self {
        self.caret = Caret::ToEnd;
        self
    }
}

impl FocusClaim {
    /// Ask the installed host to focus the element.
    ///
    /// Not inert any more. This used to be, on the reasoning that no host takes the caret away —
    /// true of the phone, where the page is the whole app, and false of the desktop, which renders
    /// this page's components into a document it owns. A page that cannot claim focus there is a
    /// page that cannot be typed into.
    pub fn request(self) {
        crate::transport::Host::focus_element(&self.element_id);
    }
}
