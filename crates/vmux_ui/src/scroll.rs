//! Bringing an element into view, where only the host can do it.

/// A request to reveal an element the page has moved something to.
///
/// Keyboard selection moves a highlight the viewport knows nothing about, so the row has to be
/// scrolled to explicitly. Two alignments rather than a general set of options, because there are
/// only two asks behind every caller: reveal it disturbing as little as possible, or put it in the
/// middle because what surrounds it matters as much as it does.
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

    /// Reveal the element with this id in the middle of its viewport.
    ///
    /// For a caret that has just jumped elsewhere in a document, where the line it landed on is
    /// only half the answer. No report of whether the element was there, unlike [`Self::nearest`]:
    /// nothing drives this from an effect that would have to try again.
    pub fn center(element_id: &str) {
        crate::transport::Host::center_item(element_id);
    }
}
