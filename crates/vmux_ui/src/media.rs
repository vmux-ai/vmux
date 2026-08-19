//! Driving a media element from outside its own controls.

/// One `<audio>` or `<video>` the page rendered, addressed by element id.
///
/// Playback is the one piece of an element's state that no attribute reaches: a page can render
/// `controls` and `autoplay`, but there is nothing to render for *playing*, and nothing to read it
/// back from either. So a keyboard shortcut over a video has to ask whatever holds the document.
pub struct MediaElement {
    element_id: String,
}

impl MediaElement {
    /// The media element rendered under this id.
    pub fn with_id(element_id: impl Into<String>) -> Self {
        Self {
            element_id: element_id.into(),
        }
    }

    /// Play it if it is paused, pause it if it is playing.
    ///
    /// A toggle rather than two calls because the page cannot read which state it is in: asking
    /// would be a round trip whose answer is stale by the time anything acts on it, where the
    /// element deciding for itself is always right.
    pub fn toggle_playback(&self) {
        crate::transport::Host::toggle_media(&self.element_id);
    }
}
