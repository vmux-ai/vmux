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
    #[cfg(web)]
    pub fn toggle_playback(&self) {
        use wasm_bindgen::JsCast;

        let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&self.element_id))
        else {
            return;
        };
        let Ok(media) = element.dyn_into::<web_sys::HtmlMediaElement>() else {
            return;
        };
        if media.paused() {
            let _ = media.play();
        } else {
            let _ = media.pause();
        }
    }

    #[cfg(not(web))]
    pub fn toggle_playback(&self) {
        crate::transport::Host::toggle_media(&self.element_id);
    }
}
