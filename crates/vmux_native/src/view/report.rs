//! What a page says back, over wry's string IPC.
//!
//! The other direction from [`route`](crate::view::route): that is the page asking the host for
//! something, this is the page telling it something. The wire is text, because that is all
//! `window.ipc` carries — which is why anything with a shape rides a request over `vmux://`
//! instead, and why what is left here keeps shrinking.

use std::rc::Rc;

use tracing::{error, info, warn};

use crate::page::NativePage;
use crate::view::embed::{Outbox, Wake};
use crate::view::measurement::{Measured, PendingReads};

/// One page-to-host message, decoded.
enum PageReport<'a> {
    /// A console line, which stays here rather than riding a request precisely because it is worth
    /// most when the page is broken: a log coupled to the frame loop goes silent with it.
    Console { level: &'a str, text: &'a str },
    /// What an element measured, against the token that asked. `None` when the page found no
    /// element to measure.
    Measured {
        token: u64,
        measured: Option<Measured>,
    },
    /// A link the shim held back rather than let it navigate the document away.
    Link(&'a str),
    /// A payload a page emitted, still base64 as the shim sent it.
    Emitted(&'a str),
}

impl<'a> PageReport<'a> {
    fn of(body: &'a str) -> Self {
        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            return Self::Console { level, text };
        }
        if let Some(rest) = body.strip_prefix("measured:")
            && let Some((token, values)) = rest.split_once(':')
            && let Ok(token) = token.parse()
        {
            return Self::Measured {
                token,
                measured: Self::numbers(values),
            };
        }
        if let Some(href) = body.strip_prefix("link:") {
            return Self::Link(href);
        }

        Self::Emitted(body)
    }

    /// The four numbers, or `None` for the empty list a page sends when it found no element.
    fn numbers(values: &str) -> Option<Measured> {
        let mut measured = [0.0; 4];
        let mut counted = 0;
        for value in values.split(',') {
            let slot = measured.get_mut(counted)?;
            *slot = value.parse().ok()?;
            counted += 1;
        }

        (counted == measured.len()).then_some(measured)
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
    reads: PendingReads,
    waker: Rc<dyn Wake>,
}

impl PageMessage {
    pub(crate) fn new(
        page: &NativePage,
        outbox: Rc<dyn Outbox>,
        reads: PendingReads,
        waker: Rc<dyn Wake>,
    ) -> Self {
        Self {
            outbox,
            name: page.url,
            reads,
            waker,
        }
    }

    pub(crate) fn receive(&self, body: &str) {
        match PageReport::of(body) {
            PageReport::Console { level, text } => self.log(level, text),
            PageReport::Measured { token, measured } => self.measured(token, measured),
            PageReport::Link(href) => self.link(href),
            PageReport::Emitted(payload) => self.emit(payload),
        }
    }

    /// A link that went nowhere.
    ///
    /// The shim lets a fragment through to the engine and holds everything else, because a native
    /// page's document is the one its `VirtualDom` is mounted against and navigating it away ends
    /// the page. Nothing opens the held ones. A link that should go somewhere is a button that
    /// emits an event the host answers — `MdInline::WikiLink` already is one — so this says which
    /// page still renders an `<a>` expecting the engine to do it.
    fn link(&self, href: &str) {
        warn!(
            "{}: no link is followed from a page hosted here, {href} went nowhere",
            self.name
        );
    }

    /// Resolve whatever asked, then wake: a task that can now run is a render nobody has scheduled.
    fn measured(&self, token: u64, measured: Option<Measured>) {
        self.reads.answer(token, measured);
        self.waker.wake();
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
                PageReport::Measured { token, measured } => match measured {
                    Some([a, b, c, d]) => format!("measured {token} {a},{b},{c},{d}"),
                    None => format!("measured {token} gone"),
                },
                PageReport::Link(href) => format!("link {href}"),
                PageReport::Emitted(payload) => format!("emitted {payload}"),
            }
        }
    }

    /// Every kind arrives as the same untagged string, so a body taken for the wrong one is silent:
    /// a log read as a payload is a base64 error in place of the message that explains the crash,
    /// and a message split on the wrong colon loses everything after the first one.
    ///
    /// A measurement short of four numbers must read as an absent element rather than as zeros,
    /// which would scroll a page to the top instead of refusing.
    #[test]
    fn each_report_decodes_from_the_string_the_shim_posts() {
        let decoded: Vec<String> = [
            "log:warn:something",
            "log:error:at line 3: boom",
            "measured:7:1,2,3,4",
            "measured:9:",
            "measured:9:1,2",
            "link:https://example.com/a:b",
            "AAAA",
        ]
        .iter()
        .map(|body| PageReport::described(body))
        .collect();

        assert_eq!(
            decoded,
            [
                "console warn something",
                "console error at line 3: boom",
                "measured 7 1,2,3,4",
                "measured 9 gone",
                "measured 9 gone",
                "link https://example.com/a:b",
                "emitted AAAA",
            ]
        );
    }
}
