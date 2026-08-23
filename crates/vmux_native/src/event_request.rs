//! The event leg: base64 JSON in, a verdict out while the page waits.
//!
//! What happened. What it happened *to* is [`event_selection`](crate::webview::event_selection), which
//! rides the same request.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use dioxus_html::HtmlEvent;
use serde::Serialize;

/// One user interaction, as the interpreter sends it.
pub struct EventRequest(HtmlEvent);

impl EventRequest {
    /// The request header carrying the event.
    ///
    /// It travels as a header rather than a body because the page sends it with a synchronous
    /// `XMLHttpRequest`, and `send()` on one of those does not reliably carry a body.
    pub const HEADER: &'static str = "dioxus-data";

    /// Decode the header value: base64, then JSON.
    pub fn from_header(value: &str) -> Result<Self, EventRequestError> {
        let json = STANDARD
            .decode(value)
            .map_err(|_| EventRequestError::NotBase64)?;
        let event = serde_json::from_slice(&json).map_err(EventRequestError::NotAnEvent)?;

        Ok(Self(event))
    }

    pub fn into_event(self) -> HtmlEvent {
        self.0
    }
}

/// Why an event could not be read.
#[derive(Debug)]
pub enum EventRequestError {
    NotBase64,
    NotAnEvent(serde_json::Error),
}

impl std::fmt::Display for EventRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBase64 => write!(f, "the {} header is not base64", EventRequest::HEADER),
            Self::NotAnEvent(error) => write!(f, "not an event: {error}"),
        }
    }
}

impl std::error::Error for EventRequestError {}

/// What the page is told once its handlers have run.
#[derive(Debug, PartialEq, Serialize)]
pub struct EventOutcome {
    #[serde(rename = "preventDefault")]
    prevent_default: bool,
}

impl EventOutcome {
    pub fn new(prevent_default: bool) -> Self {
        Self { prevent_default }
    }

    /// The answer for an event that could not be read.
    ///
    /// Letting the browser act is the safer of the two: a page that is merely unresponsive can
    /// still be closed, whereas one that swallows every default action cannot.
    pub fn unreadable() -> Self {
        Self::new(false)
    }

    pub fn prevent_default(&self) -> bool {
        self.prevent_default
    }

    /// The JSON body the page parses out of the response.
    pub fn response_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_else(|_| br#"{"preventDefault":false}"#.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use dioxus_core::ElementId;
    use dioxus_html::{EventData, SerializedMouseData};

    use super::*;

    #[test]
    fn an_event_survives_the_encoding_the_page_puts_it_through() {
        let sent = HtmlEvent {
            element: ElementId(7),
            name: "click".to_string(),
            bubbles: true,
            data: EventData::Mouse(SerializedMouseData::default()),
        };
        let header = STANDARD.encode(serde_json::to_vec(&sent).expect("an event serializes"));

        let received = EventRequest::from_header(&header)
            .expect("the page encodes exactly this")
            .into_event();

        assert_eq!(received.element, ElementId(7));
        assert_eq!(received.name, "click");
        assert!(received.bubbles);
    }

    #[test]
    fn a_header_that_is_not_base64_is_refused_rather_than_read_as_an_event() {
        assert!(matches!(
            EventRequest::from_header("not base64!!"),
            Err(EventRequestError::NotBase64)
        ));
    }

    #[test]
    fn valid_base64_that_is_not_an_event_is_refused_distinctly() {
        let header = STANDARD.encode(r#"{"something":"else"}"#);

        assert!(matches!(
            EventRequest::from_header(&header),
            Err(EventRequestError::NotAnEvent(_))
        ));
    }

    #[test]
    fn the_verdict_uses_the_field_name_the_interpreter_reads() {
        let body = EventOutcome::new(true).response_bytes();

        assert_eq!(
            String::from_utf8(body).expect("json is utf-8"),
            r#"{"preventDefault":true}"#,
            "native.js reads response.preventDefault; any other spelling is silently falsy"
        );
    }
}
