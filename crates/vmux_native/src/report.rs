//! What a page says back, over wry's string IPC.
//!
//! The other direction from [`route`](crate::route): that is the page asking the host for
//! something, this is the page telling it something. Five kinds, and the wire is text because
//! that is all `window.ipc` carries.

use std::rc::Rc;

use tracing::{error, info, warn};

use crate::dom::SurfaceDom;
use crate::embed::Outbox;
use crate::page::NativePage;

/// One page-to-host message, decoded.
enum PageReport<'a> {
    /// The interpreter is up and holding a root, so the page can take a batch at all.
    Initialized,
    /// The last batch was applied, which is what releases the next render.
    Flushed,
    /// Where the caret is, volunteered because nothing can ask for it.
    Caret {
        element: &'a str,
        start: usize,
        end: usize,
    },
    /// Whether anything in the document is selected, which a field's own range cannot answer.
    Selected(bool),
    Console {
        level: &'a str,
        text: &'a str,
    },
    /// A payload a page emitted, still base64 as the shim sent it.
    Emitted(&'a str),
}

impl<'a> PageReport<'a> {
    fn of(body: &'a str) -> Self {
        // The interpreter's own two, which it sends as `{"method":..}` through `sendIpcMessage`
        // rather than through the shim, so they arrive as JSON and everything else does not.
        if body.contains(r#""method":"initialize""#) {
            return Self::Initialized;
        }
        if body.contains(r#""method":"flushed""#) {
            return Self::Flushed;
        }
        if let Some(rest) = body.strip_prefix("selected:") {
            return Self::Selected(rest == "1");
        }
        // Split from the right: an element id may contain a colon, the two offsets may not.
        if let Some(rest) = body.strip_prefix("caret:")
            && let Some((rest, end)) = rest.rsplit_once(':')
            && let Some((element, start)) = rest.rsplit_once(':')
            && let Ok(start) = start.parse::<usize>()
            && let Ok(end) = end.parse::<usize>()
        {
            return Self::Caret {
                element,
                start,
                end,
            };
        }
        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            return Self::Console { level, text };
        }

        Self::Emitted(body)
    }
}

/// The page's messages, delivered to whatever answers for each.
///
/// A page emits through an `ArrayBuffer`; wry's IPC carries text, so the shim base64s the same
/// `BinIpcEnvelope` bytes and this undoes it. The envelope framing is left alone, because the host
/// matches its id with `bin_ipc_event_id::<E>()` and that has to keep agreeing.
pub(crate) struct PageMessage {
    outbox: Rc<dyn Outbox>,
    name: &'static str,
    dom: SurfaceDom,
}

impl PageMessage {
    pub(crate) fn new(page: &NativePage, outbox: Rc<dyn Outbox>, dom: SurfaceDom) -> Self {
        Self {
            outbox,
            name: page.url,
            dom,
        }
    }

    pub(crate) fn receive(&self, body: &str) {
        match PageReport::of(body) {
            PageReport::Initialized => self.dom.page_is_ready(),
            PageReport::Flushed => self.dom.page_flushed(),
            PageReport::Caret {
                element,
                start,
                end,
            } => self.dom.report_caret(element, start, end),
            PageReport::Selected(selected) => self.dom.report_document_selection(selected),
            PageReport::Console { level, text } => self.log(level, text),
            PageReport::Emitted(payload) => self.emit(payload),
        }
    }

    /// The page's console, under the name of the page that wrote it.
    fn log(&self, level: &str, text: &str) {
        let name = self.name;
        match level {
            "error" | "reject" => error!("{name}: {text}"),
            "warn" => warn!("{name}: {text}"),
            _ => info!("{name}: {text}"),
        }
    }

    fn emit(&self, payload: &str) {
        use base64::Engine;
        use vmux_ui::transport::bin_ipc_envelope::BinIpcEnvelope;

        let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
            Ok(bytes) => bytes,
            Err(error) => {
                error!("vmux_native: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "vmux_native: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        if self.outbox.send(&id, &payload).is_err() {
            error!("vmux_native: the host outbox is closed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl PageReport<'_> {
        /// The decoded report, flattened to something an assertion can name.
        fn described(body: &str) -> String {
            match PageReport::of(body) {
                PageReport::Initialized => "initialized".to_string(),
                PageReport::Flushed => "flushed".to_string(),
                PageReport::Caret {
                    element,
                    start,
                    end,
                } => format!("caret {element} {start}..{end}"),
                PageReport::Selected(selected) => format!("selected {selected}"),
                PageReport::Console { level, text } => format!("console {level} {text}"),
                PageReport::Emitted(payload) => format!("emitted {payload}"),
            }
        }
    }

    /// Every report is decoded from the same untagged string, so one prefix shadowing another or
    /// one field splitting off the wrong end is silent: the caret simply reads zero forever and
    /// a page that asks where it is quietly gets the wrong answer.
    ///
    /// The colon in the element id is the case the offsets have to be split from the right for.
    #[test]
    fn each_report_decodes_from_the_string_the_shim_posts() {
        let decoded: Vec<String> = [
            r#"{"method":"initialize"}"#,
            r#"{"method":"flushed"}"#,
            "caret:prompt:3:7",
            "caret:vmux:prompt:0:0",
            "selected:1",
            "selected:0",
            "log:warn:something",
            "AAAA",
        ]
        .iter()
        .map(|body| PageReport::described(body))
        .collect();

        assert_eq!(
            decoded,
            [
                "initialized",
                "flushed",
                "caret prompt 3..7",
                "caret vmux:prompt 0..0",
                "selected true",
                "selected false",
                "console warn something",
                "emitted AAAA",
            ]
        );
    }
}
