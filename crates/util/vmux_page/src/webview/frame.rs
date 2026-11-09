use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use tracing::error;

use crate::webview::dom_request::QueuedDomRequest;

#[derive(Clone)]
pub(crate) struct PageFrame {
    requests: Vec<QueuedDomRequest>,
    edits: Vec<u8>,
}

impl PageFrame {
    pub(crate) fn new(requests: Vec<QueuedDomRequest>, edits: Vec<u8>) -> Self {
        Self { requests, edits }
    }

    pub(crate) fn requests(&self) -> &[QueuedDomRequest] {
        &self.requests
    }

    pub(crate) fn into_body(self) -> Vec<u8> {
        let requests = match serde_json::to_vec(&self.requests) {
            Ok(requests) => requests,
            Err(error) => {
                error!("vmux_page: dom requests would not serialize: {error}");
                b"[]".to_vec()
            }
        };

        let mut body = Vec::with_capacity(4 + requests.len() + self.edits.len());
        body.extend_from_slice(&(requests.len() as u32).to_le_bytes());
        body.extend_from_slice(&requests);
        body.extend_from_slice(&self.edits);

        body
    }
}

#[derive(Serialize)]
pub(crate) struct InitialFrame {
    revision: String,
    sequence: String,
    epoch: String,
    capability: String,
    interactive: bool,
    body: String,
}

impl InitialFrame {
    pub(crate) fn new(frame: PageFrame, identity: (u64, u64, u64, u64), interactive: bool) -> Self {
        Self {
            revision: identity.0.to_string(),
            sequence: identity.1.to_string(),
            epoch: identity.2.to_string(),
            capability: identity.3.to_string(),
            interactive,
            body: STANDARD.encode(frame.into_body()),
        }
    }

    pub(crate) fn script(&self) -> String {
        let frame = serde_json::to_string(self).expect("initial page frame is serializable");
        format!("globalThis.__vmuxInitialFrame={frame};")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webview::dom_request::{DomRequest, RequestQueue};

    #[test]
    fn the_length_prefix_locates_the_edits_the_page_has_to_run() {
        let edits = vec![9u8, 8, 7, 0, 255];
        let requests = RequestQueue::default();
        let body = PageFrame::new(
            vec![requests.stamp(DomRequest::Focus {
                element: "prompt".to_string(),
            })],
            edits.clone(),
        )
        .into_body();

        let length = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
        let requests = std::str::from_utf8(&body[4..4 + length]).unwrap();

        assert!(requests.contains(r#""kind":"focus""#));
        assert!(requests.contains(r#""element":"prompt""#));
        assert_eq!(&body[4 + length..], &edits);
    }

    #[test]
    fn a_frame_carrying_only_edits_still_says_where_they_start() {
        let body = PageFrame::new(Vec::new(), vec![1u8, 2, 3]).into_body();

        let length = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;

        assert_eq!(&body[4..4 + length], b"[]");
        assert_eq!(&body[4 + length..], &[1u8, 2, 3]);
    }
}
