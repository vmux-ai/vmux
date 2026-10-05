pub struct TextRun {
    element_id: String,
}

impl TextRun {
    pub fn in_element(element_id: impl Into<String>) -> Self {
        Self {
            element_id: element_id.into(),
        }
    }

    pub async fn offset_at(&self, x: f64, y: f64) -> Option<u32> {
        crate::transport::Host::text_offset_at(&self.element_id, x, y).await
    }
}
