//! Bringing an element into view, where only the host can do it.

/// A request to reveal an element that a list has just selected.
///
/// Keyboard selection moves a highlight the viewport knows nothing about, so the row has to be
/// scrolled to explicitly. Every caller wants the same thing — reveal it, disturb the scroll
/// position as little as possible — which is why this takes no options.
pub struct ScrollIntoView;

impl ScrollIntoView {
    /// Reveal the element with this id, scrolling the least that brings it into view.
    ///
    /// False means the element was not in the document yet, which a caller driving this from an
    /// effect needs to know: it is the difference between latching "done" and trying again on the
    /// next render. Off the browser there is no viewport and nothing to wait for, so the request
    /// is trivially satisfied and this is always true.
    pub fn nearest(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view(element_id)
    }
}
