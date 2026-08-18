//! What a page says back, over wry's string IPC.
//!
//! The other direction from [`route`](crate::route): that is the page asking the host for
//! something, this is the page telling it something. The wire is text, because that is all
//! `window.ipc` carries — which is why anything with a shape rides a request over `vmux://`
//! instead, and why what is left here keeps shrinking.

use std::rc::Rc;

use tracing::{error, info, warn};

use crate::embed::Outbox;
use crate::page::NativePage;

/// One page-to-host message, decoded.
enum PageReport<'a> {
    /// A console line, which stays here rather than riding a request precisely because it is worth
    /// most when the page is broken: a log coupled to the frame loop goes silent with it.
    Console { level: &'a str, text: &'a str },
    /// A payload a page emitted, still base64 as the shim sent it.
    Emitted(&'a str),
}

impl<'a> PageReport<'a> {
    fn of(body: &'a str) -> Self {
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
}

impl PageMessage {
    pub(crate) fn new(page: &NativePage, outbox: Rc<dyn Outbox>) -> Self {
        Self {
            outbox,
            name: page.url,
        }
    }

    pub(crate) fn receive(&self, body: &str) {
        match PageReport::of(body) {
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
                PageReport::Console { level, text } => format!("console {level} {text}"),
                PageReport::Emitted(payload) => format!("emitted {payload}"),
            }
        }
    }

    /// Both kinds arrive as the same untagged string, so a body taken for the wrong one is silent:
    /// a log read as a payload is a base64 error in place of the message that explains the crash,
    /// and a message split on the wrong colon loses everything after the first one.
    #[test]
    fn each_report_decodes_from_the_string_the_shim_posts() {
        let decoded: Vec<String> = ["log:warn:something", "log:error:at line 3: boom", "AAAA"]
            .iter()
            .map(|body| PageReport::described(body))
            .collect();

        assert_eq!(
            decoded,
            [
                "console warn something",
                "console error at line 3: boom",
                "emitted AAAA",
            ]
        );
    }
}
