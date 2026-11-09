use std::cell::Cell;
use std::rc::Rc;

use tracing::{error, info, warn};

use crate::HistoryDirection;
use crate::page::Page;
use crate::webview::embed::{LifecycleSink, LifecycleUpdate, Outbox, Wake};
use crate::webview::measurement::{Measured, PendingReads};
use crate::webview::route::PageRoutes;

use base64::Engine;
use vmux_api::BinEvent;
use vmux_ui::transport::bin_ipc_envelope::BinIpcEnvelope;

enum PageReport<'a> {
    Console {
        level: &'a str,
        text: &'a str,
    },
    Measured {
        token: u64,
        measured: Option<Measured>,
    },
    Presented(u64),
    Link(&'a str),
    Title(&'a str),
    Favicon(&'a str),
    Interaction,
    History(HistoryDirection),
    Emitted(&'a str),
}

struct PageReportEnvelope;

impl PageReportEnvelope {
    fn payload(body: &str, current_revision: u64, current_epoch: u64) -> Option<&str> {
        let (epoch, body) = body.split_once(':')?;
        let (revision, payload) = body.split_once(':')?;
        (epoch.parse::<u64>().ok() == Some(current_epoch)
            && revision.parse::<u64>().ok() == Some(current_revision))
        .then_some(payload)
    }
}

impl<'a> From<&'a str> for PageReport<'a> {
    fn from(body: &'a str) -> Self {
        if let Some(rest) = body.strip_prefix("log:") {
            let (level, text) = rest.split_once(':').unwrap_or(("log", rest));
            return Self::Console { level, text };
        }
        if let Some(revision) = body
            .strip_prefix("presented:")
            .and_then(|value| value.parse().ok())
        {
            return Self::Presented(revision);
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
        if let Some(title) = body.strip_prefix("title:") {
            return Self::Title(title);
        }
        if let Some(href) = body.strip_prefix("favicon:") {
            return Self::Favicon(href);
        }
        if body == "interaction:" {
            return Self::Interaction;
        }
        if let Some(value) = body.strip_prefix("history:")
            && let Some(direction) = HistoryDirection::parse(value)
        {
            return Self::History(direction);
        }

        Self::Emitted(body)
    }
}

impl PageReport<'_> {
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
    page: Rc<Cell<&'static Page>>,
    navigation_revision: Rc<Cell<u64>>,
    ipc_epoch: Rc<Cell<u64>>,
    blocked: Rc<Cell<bool>>,
    reads: PendingReads,
    waker: Rc<dyn Wake>,
    lifecycle: Rc<dyn LifecycleSink>,
}

impl PageMessage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        page: Rc<Cell<&'static Page>>,
        outbox: Rc<dyn Outbox>,
        navigation_revision: Rc<Cell<u64>>,
        ipc_epoch: Rc<Cell<u64>>,
        blocked: Rc<Cell<bool>>,
        reads: PendingReads,
        waker: Rc<dyn Wake>,
        lifecycle: Rc<dyn LifecycleSink>,
    ) -> Self {
        Self {
            outbox,
            page,
            navigation_revision,
            ipc_epoch,
            blocked,
            reads,
            waker,
            lifecycle,
        }
    }

    pub(crate) fn receive(&self, source: &str, body: &str) {
        if !PageRoutes::serves_document(source, self.page.get()) {
            return;
        }
        let epoch = self.ipc_epoch.get();
        let Some(body) = PageReportEnvelope::payload(body, self.navigation_revision.get(), epoch)
        else {
            return;
        };
        let report = PageReport::from(body);
        if self.blocked.get()
            && !matches!(
                &report,
                PageReport::Console { .. }
                    | PageReport::Measured { .. }
                    | PageReport::Presented(_)
                    | PageReport::Title(_)
                    | PageReport::Favicon(_)
                    | PageReport::Emitted(_)
            )
        {
            return;
        }
        match report {
            PageReport::Console { level, text } => self.log(level, text),
            PageReport::Measured { token, measured } => self.measured(token, measured),
            PageReport::Presented(revision) => {
                if revision == self.navigation_revision.get() {
                    self.lifecycle
                        .update(LifecycleUpdate::FramePainted(revision));
                    self.waker.wake();
                }
            }
            PageReport::Link(href) => self.link(href),
            PageReport::Title(title) => {
                if self.page.get().reports_title {
                    self.outbox.set_title(title);
                }
            }
            PageReport::Favicon(href) => self.outbox.set_favicon(href),
            PageReport::Interaction => self.lifecycle.update(LifecycleUpdate::PageInteracted),
            PageReport::History(direction) => self
                .lifecycle
                .update(LifecycleUpdate::HistoryRequested(direction)),
            PageReport::Emitted(payload) => self.emit(payload, epoch),
        }
    }

    fn link(&self, href: &str) {
        let name = self.page.get().url;
        warn!("{name}: no link is followed from a page hosted here, {href} went nowhere");
    }

    fn measured(&self, token: u64, measured: Option<Measured>) {
        self.reads.answer(token, measured);
        self.waker.wake();
    }

    fn log(&self, level: &str, text: &str) {
        let name = self.page.get().url;
        match level {
            "error" | "reject" => error!("{name}: {text}"),
            "warn" => warn!("{name}: {text}"),
            _ => info!("{name}: {text}"),
        }
    }

    fn emit(&self, payload: &str, ipc_epoch: u64) {
        let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
            Ok(bytes) => bytes,
            Err(error) => {
                error!("vmux_page: ipc payload was not base64: {error}");
                return;
            }
        };
        let Some((id, payload)) = BinIpcEnvelope::decode(&bytes) else {
            error!(
                "vmux_page: ipc payload was not a bin ipc envelope, {} bytes",
                bytes.len()
            );
            return;
        };
        if self.blocked.get() && id != vmux_api::PageReady::id() {
            return;
        }
        if self
            .outbox
            .send(&id, &payload, self.navigation_revision.get(), ipc_epoch)
            .is_err()
        {
            error!("vmux_page: the host outbox is closed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    #[derive(Default)]
    struct RecordedOutbox {
        events: RefCell<Vec<String>>,
        titles: RefCell<Vec<String>>,
        favicons: RefCell<Vec<String>>,
    }

    impl Outbox for RecordedOutbox {
        fn send(
            &self,
            id: &str,
            _bytes: &[u8],
            _navigation_revision: u64,
            _ipc_epoch: u64,
        ) -> Result<(), vmux_ui::hooks::EventListenerError> {
            self.events.borrow_mut().push(id.to_string());
            Ok(())
        }

        fn set_title(&self, title: &str) {
            self.titles.borrow_mut().push(title.to_string());
        }

        fn set_favicon(&self, url: &str) {
            self.favicons.borrow_mut().push(url.to_string());
        }
    }

    struct TestWake;

    impl Wake for TestWake {
        fn wake(&self) {}
    }

    struct TestLifecycle;

    impl LifecycleSink for TestLifecycle {
        fn update(&self, _update: LifecycleUpdate) {}
    }

    #[derive(Default)]
    struct HistoryProbe {
        updates: RefCell<Vec<LifecycleUpdate>>,
    }

    impl LifecycleSink for HistoryProbe {
        fn update(&self, update: LifecycleUpdate) {
            self.updates.borrow_mut().push(update);
        }
    }

    #[component]
    fn TestPage() -> Element {
        rsx! {}
    }

    static TEST_PAGE: Page = Page::pane("vmux://test/", TestPage);

    fn emitted(id: &str) -> String {
        let envelope = BinIpcEnvelope::new(id, &[]);
        let payload = base64::engine::general_purpose::STANDARD.encode(envelope.as_bytes());
        format!("1:1:{payload}")
    }

    impl PageReport<'_> {
        fn described(body: &str) -> String {
            match PageReport::from(body) {
                PageReport::Console { level, text } => format!("console {level} {text}"),
                PageReport::Measured { token, measured } => match measured {
                    Some([a, b, c, d]) => format!("measured {token} {a},{b},{c},{d}"),
                    None => format!("measured {token} gone"),
                },
                PageReport::Link(href) => format!("link {href}"),
                PageReport::Title(title) => format!("title {title}"),
                PageReport::Favicon(href) => format!("favicon {href}"),
                PageReport::Interaction => "interaction".to_string(),
                PageReport::History(direction) => format!("history {direction:?}"),
                PageReport::Emitted(payload) => format!("emitted {payload}"),
                PageReport::Presented(revision) => format!("presented {revision}"),
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
            "title:Session 42",
            "favicon:vmux://history/assets/favicons/history.svg",
            "interaction:",
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
                "title Session 42",
                "favicon vmux://history/assets/favicons/history.svg",
                "interaction",
                "emitted AAAA",
            ]
        );
    }

    #[test]
    fn reports_only_reach_the_navigation_that_created_them() {
        assert_eq!(PageReportEnvelope::payload("3:7:AAAA", 7, 3), Some("AAAA"));
        assert_eq!(PageReportEnvelope::payload("3:6:AAAA", 7, 3), None);
        assert_eq!(PageReportEnvelope::payload("2:7:AAAA", 7, 3), None);
        assert_eq!(PageReportEnvelope::payload("AAAA", 7, 3), None);
    }

    #[test]
    fn a_history_gesture_requires_the_current_document_and_interaction_epoch() {
        let lifecycle = Rc::new(HistoryProbe::default());
        let blocked = Rc::new(Cell::new(false));
        let message = PageMessage::new(
            Rc::new(Cell::new(&TEST_PAGE)),
            Rc::new(RecordedOutbox::default()),
            Rc::new(Cell::new(7)),
            Rc::new(Cell::new(3)),
            blocked.clone(),
            PendingReads::new(lifecycle.clone()),
            Rc::new(TestWake),
            lifecycle.clone(),
        );
        message.receive("https://example.com/", "3:7:history:back");
        message.receive("vmux://test/", "3:6:history:back");
        message.receive("vmux://test/", "2:7:history:back");
        message.receive("vmux://test/", "3:7:history:reload");
        message.receive("vmux://test/", "3:7:history:back");
        blocked.set(true);
        message.receive("vmux://test/", "3:7:history:forward");
        blocked.set(false);
        message.receive("vmux://test/", "3:7:history:forward");
        assert_eq!(
            *lifecycle.updates.borrow(),
            [
                LifecycleUpdate::HistoryRequested(HistoryDirection::Back),
                LifecycleUpdate::HistoryRequested(HistoryDirection::Forward),
            ],
        );
    }

    #[test]
    fn blocked_page_keeps_its_ready_handshake_and_rejects_other_events() {
        let outbox = Rc::new(RecordedOutbox::default());
        let blocked = Rc::new(Cell::new(true));
        let lifecycle = Rc::new(TestLifecycle);
        let reads = PendingReads::new(lifecycle.clone());
        let mut measurement = reads.ask();
        let token = measurement.token();
        let message = PageMessage::new(
            Rc::new(Cell::new(&TEST_PAGE)),
            outbox.clone(),
            Rc::new(Cell::new(1)),
            Rc::new(Cell::new(1)),
            blocked.clone(),
            reads,
            Rc::new(TestWake),
            lifecycle,
        );

        message.receive("vmux://test/", &emitted("action@1"));
        message.receive("vmux://test/", &emitted(vmux_api::PageReady::id()));
        message.receive("vmux://test/", &format!("1:1:measured:{token}:1,2,3,4"));
        message.receive("vmux://test/", "1:1:title:Current session");
        message.receive("vmux://test/", "1:1:favicon:vmux://test/favicon.svg");
        blocked.set(false);
        message.receive("vmux://test/", &emitted("action@1"));

        assert_eq!(
            outbox.events.borrow().as_slice(),
            [
                vmux_api::PageReady::id().to_string(),
                "action@1".to_string()
            ]
        );
        assert_eq!(outbox.titles.borrow().as_slice(), ["Current session"]);
        assert_eq!(
            outbox.favicons.borrow().as_slice(),
            ["vmux://test/favicon.svg"]
        );
        assert!(matches!(
            Pin::new(&mut measurement).poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok([1.0, 2.0, 3.0, 4.0]))
        ));
    }

    #[test]
    fn reports_from_another_document_or_epoch_are_rejected() {
        let outbox = Rc::new(RecordedOutbox::default());
        let lifecycle = Rc::new(TestLifecycle);
        let message = PageMessage::new(
            Rc::new(Cell::new(&TEST_PAGE)),
            outbox.clone(),
            Rc::new(Cell::new(1)),
            Rc::new(Cell::new(2)),
            Rc::new(Cell::new(false)),
            PendingReads::new(lifecycle.clone()),
            Rc::new(TestWake),
            lifecycle,
        );

        message.receive("https://example.com/", &emitted("action@1"));
        message.receive("vmux://test/", &emitted("action@1"));

        assert!(outbox.events.borrow().is_empty());
    }
}
