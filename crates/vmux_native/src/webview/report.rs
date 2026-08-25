use std::rc::Rc;

use tracing::{error, info, warn};

use crate::page::NativePage;
use crate::webview::embed::{Outbox, Wake};
use crate::webview::measurement::{Measured, PendingReads};

enum PageReport<'a> {
    Console {
        level: &'a str,
        text: &'a str,
    },
    Measured {
        token: u64,
        measured: Option<Measured>,
    },
    Link(&'a str),
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

    fn link(&self, href: &str) {
        warn!(
            "{}: no link is followed from a page hosted here, {href} went nowhere",
            self.name
        );
    }

    fn measured(&self, token: u64, measured: Option<Measured>) {
        self.reads.answer(token, measured);
        self.waker.wake();
    }

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
