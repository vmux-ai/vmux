pub struct MediaElement {
    element_id: String,
}

impl MediaElement {
    pub fn with_id(element_id: impl Into<String>) -> Self {
        Self {
            element_id: element_id.into(),
        }
    }

    pub fn toggle_playback(&self) {
        crate::transport::Host::toggle_media(&self.element_id);
    }
}
