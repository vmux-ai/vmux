#[derive(Default)]
pub(crate) struct EventSelection {
    field: Option<(String, usize, usize)>,
    document: bool,
}

impl EventSelection {
    const DOCUMENT: &'static str = "x-vmux-selected";
    const FIELD: &'static str = "x-vmux-caret";

    pub(crate) fn of(headers: &wry::http::HeaderMap) -> Self {
        Self {
            field: Self::field_in(headers),
            document: headers
                .get(Self::DOCUMENT)
                .is_some_and(|value| value == "1"),
        }
    }

    pub(crate) fn in_field(&self, element_id: &str) -> (usize, usize) {
        match &self.field {
            Some((id, start, end)) if id == element_id => (*start, *end),
            _ => (0, 0),
        }
    }

    pub(crate) fn in_document(&self) -> bool {
        self.document
    }

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
        fn reported(field: &str, document: &str) -> Self {
            let mut headers = wry::http::HeaderMap::new();
            headers.insert(Self::FIELD, field.parse().expect("a header value"));
            headers.insert(Self::DOCUMENT, document.parse().expect("a header value"));

            Self::of(&headers)
        }
    }

    #[test]
    fn a_caret_header_answers_for_the_field_it_names_and_no_other() {
        let selection = EventSelection::reported("vmux:prompt:3:7", "1");

        assert_eq!(selection.in_field("vmux:prompt"), (3, 7));
        assert_eq!(selection.in_field("somewhere-else"), (0, 0));
        assert!(selection.in_document());
    }

    #[test]
    fn an_event_that_reports_nothing_has_nothing_selected() {
        let selection = EventSelection::of(&wry::http::HeaderMap::new());

        assert_eq!(selection.in_field("vmux:prompt"), (0, 0));
        assert!(!selection.in_document());
    }
}
