//! Where a point on screen falls in text the page laid out.

/// One run of text a page rendered, addressed by the id it rendered it under.
///
/// The counterpart to [`crate::caret::TextCaret`], which is a text field's own caret. This is
/// ordinary rendered text, with no caret and no selection in it — only the question of which
/// character somebody just pointed at. A monospace grid answers that with arithmetic on a cell
/// size and needs no host at all; proportional text answers it only out of the engine's own
/// layout, which is why it goes through one.
pub struct TextRun {
    element_id: String,
}

impl TextRun {
    /// The run rendered under this id.
    pub fn in_element(element_id: impl Into<String>) -> Self {
        Self {
            element_id: element_id.into(),
        }
    }

    /// How many characters into the run a point on screen falls, or `None` where nothing can say.
    ///
    /// Waits, unlike everything on [`crate::caret::TextCaret`], so a caller inside a pointer
    /// handler has to have settled `prevent_default` before asking. Every caller does: whether a
    /// click belongs to the page never depends on where in a line it landed.
    pub async fn offset_at(&self, x: f64, y: f64) -> Option<u32> {
        crate::transport::Host::text_offset_at(&self.element_id, x, y).await
    }
}
