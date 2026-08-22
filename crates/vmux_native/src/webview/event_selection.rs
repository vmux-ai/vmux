//! What was selected when an event was raised, as that event's own request reported it.
//!
//! The other half of what arrives on `__events`: [`EventRequest`](crate::EventRequest) is what
//! happened, this is what it happened to. Both travel as headers, and for the same reason — the
//! page sends them with a synchronous `XMLHttpRequest`, whose body does not reliably arrive.
//!
//! Riding the event is what makes this the event's *own* selection. A handler settles
//! `prevent_default` before it returns, so it cannot wait to be told, and a report sent separately
//! travels on the host's run loop and can land after the decision it was meant to inform.

/// A page's selection, frozen at the moment an event was raised.
#[derive(Default)]
pub(crate) struct EventSelection {
    field: Option<(String, usize, usize)>,
    document: bool,
}

impl EventSelection {
    /// Set to `1` when anything in the document as a whole is selected.
    const DOCUMENT: &'static str = "x-vmux-selected";
    /// `<element>:<start>:<end>` in UTF-8 bytes, absent when no text field has focus.
    const FIELD: &'static str = "x-vmux-caret";

    pub(crate) fn of(headers: &wry::http::HeaderMap) -> Self {
        Self {
            field: Self::field_in(headers),
            document: headers
                .get(Self::DOCUMENT)
                .is_some_and(|value| value == "1"),
        }
    }

    /// The range in this field, collapsed at zero when the event came from a different one.
    pub(crate) fn in_field(&self, element_id: &str) -> (usize, usize) {
        match &self.field {
            Some((id, start, end)) if id == element_id => (*start, *end),
            _ => (0, 0),
        }
    }

    pub(crate) fn in_document(&self) -> bool {
        self.document
    }

    /// Split from the right: an element id may contain a colon, the two offsets may not.
    fn field_in(headers: &wry::http::HeaderMap) -> Option<(String, usize, usize)> {
        let reported = headers.get(Self::FIELD)?.to_str().ok()?;
        let (rest, end) = reported.rsplit_once(':')?;
        let (element, start) = rest.rsplit_once(':')?;

        Some((element.to_string(), start.parse().ok()?, end.parse().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl EventSelection {
        /// The pair of headers the shim sets, as it sets them.
        fn reported(field: &str, document: &str) -> Self {
            let mut headers = wry::http::HeaderMap::new();
            headers.insert(Self::FIELD, field.parse().expect("a header value"));
            headers.insert(Self::DOCUMENT, document.parse().expect("a header value"));

            Self::of(&headers)
        }
    }

    /// The offsets split from the right because an element id may hold a colon, and splitting the
    /// other way reads as a caret pinned at zero — which is silent, and turns every Up in a
    /// multi-line draft into prompt recall.
    #[test]
    fn a_caret_header_answers_for_the_field_it_names_and_no_other() {
        let selection = EventSelection::reported("vmux:prompt:3:7", "1");

        assert_eq!(selection.in_field("vmux:prompt"), (3, 7));
        assert_eq!(selection.in_field("somewhere-else"), (0, 0));
        assert!(selection.in_document());
    }

    /// An event raised with nothing focused carries neither header, and a page asking then must
    /// get the same answer as one asking about the wrong field rather than a stale range.
    #[test]
    fn an_event_that_reports_nothing_has_nothing_selected() {
        let selection = EventSelection::of(&wry::http::HeaderMap::new());

        assert_eq!(selection.in_field("vmux:prompt"), (0, 0));
        assert!(!selection.in_document());
    }
}
