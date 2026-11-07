use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::{error, warn};
use vmux_api::BinEvent;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

use crate::webview::dom_request::{DomRequest, QueuedDomRequest, RequestQueue};
use crate::webview::element::Element;
use crate::webview::embed::{Embedding, LifecycleSink, LifecycleUpdate, Outbox, Wake};
use crate::webview::event_selection::EventSelection;
use crate::webview::frame::PageFrame;
use crate::webview::measurement::PendingReads;
use crate::{EventOutcome, EventRequest, PageDom};

#[derive(Clone)]
pub(crate) struct Dom {
    page: Rc<RefCell<PageDom>>,
    host: Rc<dyn PageHost>,
    reactor: Rc<tokio::runtime::Runtime>,
    waker: Rc<dyn Wake>,
    selection: Rc<RefCell<EventSelection>>,
    listeners: Listeners,
    requests: RequestQueue,
    reads: PendingReads,
    lifecycle: Rc<dyn LifecycleSink>,
    mounted: Rc<Cell<bool>>,
    parked: Rc<RefCell<Option<wry::RequestAsyncResponder>>>,
    pending_instance: Rc<RefCell<Option<crate::Instance>>>,
    navigation_revision: Rc<Cell<u64>>,
    ipc_epoch: Rc<Cell<u64>>,
    delivered_revision: Rc<Cell<u64>>,
    delivered_frame: Rc<RefCell<Option<DeliveredFrame>>>,
    next_frame_sequence: Rc<Cell<u64>>,
    frame_capability: u64,
    blocked: Rc<Cell<bool>>,
    ready_requested: Rc<Cell<bool>>,
    deferred_events: Rc<RefCell<DeferredEvents>>,
    document_loaded: Rc<Cell<bool>>,
    remount_pending: Rc<Cell<bool>>,
}

const FRAME_APPLIED: &str = "x-vmux-applied-frame";
const FRAME_FAILED: &str = "x-vmux-failed-frame";
const FRAME_SEQUENCE: &str = "x-vmux-frame";
const FRAME_EPOCH: &str = "x-vmux-frame-epoch";
const FRAME_CAPABILITY: &str = "x-vmux-frame-capability";
const NAVIGATION_REVISION: &str = "x-vmux-navigation";
static NEXT_FRAME_CAPABILITY: AtomicU64 = AtomicU64::new(0);

type Listeners = Rc<RefCell<HashMap<String, Vec<BytesListener>>>>;

enum EventAdmission {
    Accepted(EventRequest),
    Deferred(EventRequest),
    Refused,
    Invalid,
}

struct DeliveredFrame {
    sequence: u64,
    revision: u64,
    failures: u8,
    requests: Vec<QueuedDomRequest>,
}

struct DeferredEvent {
    revision: u64,
    epoch: u64,
    event: EventRequest,
    selection: EventSelection,
}

#[derive(Default)]
struct DeferredEvents {
    events: VecDeque<DeferredEvent>,
}

impl DeferredEvents {
    const LIMIT: usize = 256;

    fn push(&mut self, deferred: DeferredEvent) {
        if let Some(index) = self
            .events
            .iter()
            .position(|current| current.event.coalesces_with(&deferred.event))
        {
            self.events[index] = deferred;
            return;
        }
        if self.events.len() == Self::LIMIT {
            self.events.pop_front();
        }
        self.events.push_back(deferred);
    }

    fn take(&mut self, revision: u64, epoch: u64) -> Vec<(EventRequest, EventSelection)> {
        std::mem::take(&mut self.events)
            .into_iter()
            .filter(|deferred| deferred.revision == revision && deferred.epoch == epoch)
            .map(|deferred| (deferred.event, deferred.selection))
            .collect()
    }

    fn clear(&mut self) {
        self.events.clear();
    }
}

struct FramePoll {
    revision: u64,
    epoch: u64,
    capability: u64,
    acknowledged: Option<u64>,
    failed: Option<u64>,
}

impl FramePoll {
    fn from(request: &wry::http::Request<Vec<u8>>) -> Option<Self> {
        let headers = request.headers();
        let revision = headers
            .get(NAVIGATION_REVISION)?
            .to_str()
            .ok()?
            .parse::<u64>()
            .ok()?;
        let epoch = headers
            .get(FRAME_EPOCH)?
            .to_str()
            .ok()?
            .parse::<u64>()
            .ok()?;
        let capability = headers
            .get(FRAME_CAPABILITY)?
            .to_str()
            .ok()?
            .parse::<u64>()
            .ok()?;
        let acknowledged = Self::sequence(headers, FRAME_APPLIED);
        let failed = Self::sequence(headers, FRAME_FAILED);
        if acknowledged.is_some() && failed.is_some() {
            return None;
        }
        Some(Self {
            revision,
            epoch,
            capability,
            acknowledged,
            failed,
        })
    }

    fn sequence(headers: &wry::http::HeaderMap, name: &str) -> Option<u64> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|sequence| *sequence != 0)
    }

    fn accepted_by(&self, dom: &Dom) -> bool {
        if self.epoch != dom.ipc_epoch.get() || self.capability != dom.frame_capability {
            return false;
        }
        if self.revision == dom.navigation_revision.get() {
            return true;
        }
        self.revision == 0
            && self.acknowledged.is_none()
            && self.failed.is_none()
            && dom.delivered_frame.borrow().is_none()
    }

    fn acknowledge(&self, dom: &Dom) -> bool {
        if !self.accepted_by(dom) {
            return false;
        }
        let Some(sequence) = self.acknowledged else {
            return false;
        };
        let revision = {
            let mut delivered = dom.delivered_frame.borrow_mut();
            if delivered
                .as_ref()
                .is_none_or(|frame| frame.sequence != sequence)
            {
                return false;
            }
            delivered.take().unwrap().revision
        };
        dom.page_flushed();
        dom.lifecycle
            .update(LifecycleUpdate::FrameApplied(revision));
        true
    }
}

impl Dom {
    pub(crate) fn mount(
        component: crate::PageComponent,
        instance: crate::Instance,
        embed: &Embedding,
        blocked: Rc<Cell<bool>>,
    ) -> Self {
        let listeners: Listeners = Rc::new(RefCell::new(HashMap::new()));
        let requests =
            RequestQueue::new(embed.ipc_epoch.clone(), embed.navigation_revision.clone());
        let reads = PendingReads::new(embed.lifecycle.clone());
        let selection: Rc<RefCell<EventSelection>> = Rc::default();
        let navigation_revision = embed.navigation_revision.clone();
        let ready_requested = Rc::new(Cell::new(false));
        let host: Rc<dyn PageHost> = Rc::new(SurfaceHost {
            outbox: embed.outbox.clone(),
            listeners: listeners.clone(),
            requests: requests.clone(),
            reads: reads.clone(),
            selection: selection.clone(),
            lifecycle: embed.lifecycle.clone(),
            navigation_revision: navigation_revision.clone(),
            ipc_epoch: embed.ipc_epoch.clone(),
            blocked: blocked.clone(),
            ready_requested: ready_requested.clone(),
            waker: embed.waker.clone(),
        });
        embed.lifecycle.update(LifecycleUpdate::Mounted);
        let _host = HostScope::enter(host.clone());

        Self {
            page: Rc::new(RefCell::new(PageDom::mount(component, instance))),
            host,
            reactor: Self::reactor(),
            waker: embed.waker.clone(),
            selection,
            listeners,
            requests,
            reads,
            lifecycle: embed.lifecycle.clone(),
            mounted: Rc::new(Cell::new(false)),
            parked: Rc::new(RefCell::new(None)),
            pending_instance: Rc::new(RefCell::new(None)),
            navigation_revision,
            ipc_epoch: embed.ipc_epoch.clone(),
            delivered_revision: Rc::new(Cell::new(0)),
            delivered_frame: Rc::new(RefCell::new(None)),
            next_frame_sequence: Rc::new(Cell::new(0)),
            frame_capability: NEXT_FRAME_CAPABILITY
                .fetch_add(1, Ordering::AcqRel)
                .wrapping_add(1)
                .max(1),
            blocked,
            ready_requested,
            deferred_events: Rc::default(),
            document_loaded: Rc::new(Cell::new(false)),
            remount_pending: Rc::new(Cell::new(false)),
        }
    }

    fn reactor() -> Rc<tokio::runtime::Runtime> {
        thread_local! {
            static REACTOR: Rc<tokio::runtime::Runtime> = Rc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_time()
                    .thread_name("vmux-page")
                    .build()
                    .expect("a reactor for a page's timers"),
            );
        }

        REACTOR.with(Rc::clone)
    }

    pub(crate) fn reads(&self) -> PendingReads {
        self.reads.clone()
    }

    pub(crate) fn navigation_revision(&self) -> Rc<Cell<u64>> {
        self.navigation_revision.clone()
    }

    pub(crate) fn frame_capability(&self) -> u64 {
        self.frame_capability
    }

    pub(crate) fn interaction_epoch_changed(&self, previous_epoch: u64) {
        self.cancel_parked_poll();
        self.clear_transient_requests();
        if self.blocked.get() {
            self.deferred_events.borrow_mut().clear();
            self.waker.wake();
            return;
        }
        let revision = self.navigation_revision.get();
        let deferred = self
            .deferred_events
            .borrow_mut()
            .take(revision, previous_epoch);
        for (event, selection) in deferred {
            self.handle_event(event, selection);
        }
        self.waker.wake();
    }

    pub(crate) fn remount(&self, component: crate::PageComponent, instance: crate::Instance) {
        let revision = self.next_navigation_revision();
        self.restart_rendering();
        self.listeners.borrow_mut().clear();
        self.lifecycle.update(LifecycleUpdate::ListenersCleared);
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        *self.page.borrow_mut() = PageDom::mount(component, instance);
        self.ready_requested.set(false);
        self.pending_instance.borrow_mut().take();
        self.deferred_events.borrow_mut().clear();
        self.lifecycle.update(LifecycleUpdate::Remounted);
        self.lifecycle.update(LifecycleUpdate::Navigated(revision));
        self.waker.wake();
    }

    pub(crate) fn update_instance(&self, instance: crate::Instance) {
        let revision = self.next_navigation_revision();
        self.cancel_parked_poll();
        self.clear_transient_requests();
        self.deferred_events.borrow_mut().clear();
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        if let Ok(mut page) = self.page.try_borrow_mut() {
            self.pending_instance.borrow_mut().take();
            page.update_instance(instance);
        } else {
            self.pending_instance.borrow_mut().replace(instance);
        }
        self.ready_requested.set(true);
        self.lifecycle.update(LifecycleUpdate::Navigated(revision));
        self.waker.wake();
    }

    pub(crate) fn document_loaded(&self) {
        if !self.document_loaded.replace(true) {
            self.waker.wake();
            return;
        }
        let revision = self.next_navigation_revision();
        self.restart_rendering();
        self.deferred_events.borrow_mut().clear();
        self.ready_requested.set(false);
        self.lifecycle.update(LifecycleUpdate::Remounted);
        self.lifecycle.update(LifecycleUpdate::Navigated(revision));
        self.waker.wake();
    }

    fn next_navigation_revision(&self) -> u64 {
        let revision = self.navigation_revision.get().wrapping_add(1).max(1);
        self.navigation_revision.set(revision);
        revision
    }

    fn page_flushed(&self) {
        let _host = HostScope::enter(self.host.clone());
        if let Ok(mut page) = self.page.try_borrow_mut() {
            page.flushed();
        }
    }

    fn next_batch(&self) -> Option<Vec<u8>> {
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let mut page = self.page.try_borrow_mut().ok()?;
        if let Some(instance) = self.pending_instance.borrow_mut().take() {
            page.update_instance(instance);
        }
        let task_waker = self.waker.task_waker();
        let batch = if self.mounted.get() {
            page.render_with_waker(&task_waker)
        } else {
            self.mounted.set(true);
            let batch = page.rebuild_with_waker(&task_waker);
            Some(batch)
        };
        drop(page);
        if self.ready_requested.replace(false) {
            self.lifecycle
                .update(LifecycleUpdate::Ready(self.navigation_revision.get()));
        }
        batch
    }

    fn rebuild_batch(&self) -> Vec<u8> {
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        self.reads.clear();
        *self.selection.borrow_mut() = EventSelection::default();
        let mut page = self.page.borrow_mut();
        if let Some(instance) = self.pending_instance.borrow_mut().take() {
            page.update_instance(instance);
        }
        let task_waker = self.waker.task_waker();
        let batch = page.rebuild_with_waker(&task_waker);
        self.mounted.set(true);
        drop(page);
        if self.ready_requested.replace(false) {
            self.lifecycle
                .update(LifecycleUpdate::Ready(self.navigation_revision.get()));
        }
        batch
    }

    pub(crate) fn serve_edits(
        &self,
        request: &wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let Some(poll) = FramePoll::from(request) else {
            warn!(
                "vmux_page: invalid edits poll revision={:?} epoch={:?} capability={:?}",
                request.headers().get(NAVIGATION_REVISION),
                request.headers().get(FRAME_EPOCH),
                request.headers().get(FRAME_CAPABILITY),
            );
            self.refuse_poll(responder);
            return;
        };
        if !poll.accepted_by(self) {
            warn!(
                "vmux_page: stale edits poll revision={} epoch={} capability={} expected_revision={} expected_epoch={} expected_capability={}",
                poll.revision,
                poll.epoch,
                poll.capability,
                self.navigation_revision.get(),
                self.ipc_epoch.get(),
                self.frame_capability,
            );
            self.refuse_poll(responder);
            return;
        }
        if poll.acknowledged.is_some() && !poll.acknowledge(self) {
            self.refuse_poll(responder);
            return;
        }
        if let Some(sequence) = poll.failed {
            let failure = {
                let mut delivered = self.delivered_frame.borrow_mut();
                let Some(frame) = delivered
                    .as_mut()
                    .filter(|frame| frame.sequence == sequence)
                else {
                    drop(delivered);
                    self.refuse_poll(responder);
                    return;
                };
                frame.failures = frame.failures.saturating_add(1);
                (frame.revision, frame.failures, frame.requests.clone())
            };
            if failure.1 >= 3 {
                self.restart_rendering();
                self.lifecycle
                    .update(LifecycleUpdate::FrameFailed(failure.0));
                self.fail(responder);
                return;
            }
            let mut requests = vec![self.requests.stamp(DomRequest::Remount)];
            requests.extend(failure.2);
            requests.extend(self.requests.take());
            let frame = PageFrame::new(requests, self.rebuild_batch());
            self.lifecycle.update(LifecycleUpdate::FramePending);
            self.respond_attempt(responder, frame, sequence, failure.1);
            return;
        }
        if let Some(stale) = self.parked.borrow_mut().take() {
            self.cancel(stale);
        }
        if self.delivered_frame.borrow().is_some() {
            self.cancel(responder);
            return;
        }
        *self.parked.borrow_mut() = Some(responder);
        self.lifecycle.update(LifecycleUpdate::FramePending);
        self.flush_to_page();
    }

    pub(crate) fn flush_to_page(&self) {
        if self.parked.borrow().is_none() || self.delivered_frame.borrow().is_some() {
            return;
        }
        let Some(frame) = self.next_frame() else {
            return;
        };
        let Some(responder) = self.parked.borrow_mut().take() else {
            return;
        };

        self.respond(responder, frame);
    }

    fn next_frame(&self) -> Option<PageFrame> {
        let edits = self.next_batch();
        if self.remount_pending.get() && edits.is_none() {
            return None;
        }
        let mut requests = self.requests.take();
        if self.remount_pending.replace(false) {
            requests.insert(0, self.requests.stamp(DomRequest::Remount));
        }
        let revision_changed = self.delivered_revision.get() != self.navigation_revision.get();
        if edits.is_none() && requests.is_empty() && !revision_changed {
            return None;
        }
        Some(PageFrame::new(requests, edits.unwrap_or_default()))
    }

    pub(crate) fn cancel_pending_reads(&self) {
        self.reads.clear();
    }

    pub(crate) fn answer_event(
        &self,
        request: &wry::http::Request<Vec<u8>>,
        responder: wry::RequestAsyncResponder,
    ) {
        let headers = request.headers();
        let selection = EventSelection::from(headers);
        let outcome = match self.admit_event(headers) {
            EventAdmission::Accepted(event) => {
                let outcome = self.handle_event(event, selection);
                self.flush_to_page();
                outcome
            }
            EventAdmission::Deferred(event) => {
                self.deferred_events.borrow_mut().push(DeferredEvent {
                    revision: self.navigation_revision.get(),
                    epoch: self.ipc_epoch.get(),
                    event,
                    selection,
                });
                EventOutcome::unreadable()
            }
            EventAdmission::Refused => EventOutcome::refused(),
            EventAdmission::Invalid => EventOutcome::unreadable(),
        };
        let body = outcome.response_bytes();
        let response = wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()));

        responder.respond(response);
    }

    fn accepts_event(&self, headers: &wry::http::HeaderMap) -> bool {
        let revision_matches = headers
            .get(NAVIGATION_REVISION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            == Some(self.navigation_revision.get());
        let epoch_matches = headers
            .get("x-vmux-ipc-epoch")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            == Some(self.ipc_epoch.get());
        revision_matches && epoch_matches
    }

    fn admit_event(&self, headers: &wry::http::HeaderMap) -> EventAdmission {
        if !self.accepts_event(headers) {
            return EventAdmission::Refused;
        }
        let Some(payload) = headers
            .get(EventRequest::HEADER)
            .and_then(|value| value.to_str().ok())
        else {
            return if self.blocked.get() {
                EventAdmission::Refused
            } else {
                EventAdmission::Invalid
            };
        };
        let event = match EventRequest::from_header(payload) {
            Ok(event) => event,
            Err(error) => {
                warn!("vmux_page: {error}");
                return if self.blocked.get() {
                    EventAdmission::Refused
                } else {
                    EventAdmission::Invalid
                };
            }
        };
        if self.blocked.get() {
            return if event.deferred_while_blocked() {
                EventAdmission::Deferred(event)
            } else {
                EventAdmission::Refused
            };
        }
        EventAdmission::Accepted(event)
    }

    fn cancel(&self, responder: wry::RequestAsyncResponder) {
        self.refuse_poll(responder);
    }

    fn refuse_poll(&self, responder: wry::RequestAsyncResponder) {
        let sequence = self
            .delivered_frame
            .borrow()
            .as_ref()
            .map_or(0, |frame| frame.sequence);
        let response = wry::http::Response::builder()
            .status(wry::http::StatusCode::PRECONDITION_FAILED)
            .header(
                NAVIGATION_REVISION,
                self.navigation_revision.get().to_string(),
            )
            .header(FRAME_EPOCH, self.ipc_epoch.get().to_string())
            .header(FRAME_CAPABILITY, self.frame_capability.to_string())
            .header(FRAME_SEQUENCE, sequence.to_string())
            .body(Vec::new())
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()));
        responder.respond(response);
    }

    fn fail(&self, responder: wry::RequestAsyncResponder) {
        let response = wry::http::Response::builder()
            .status(wry::http::StatusCode::CONFLICT)
            .body(Vec::new())
            .unwrap_or_else(|_| wry::http::Response::new(Vec::new()));
        responder.respond(response);
    }

    fn invalidate_delivered_frame(&self) -> bool {
        self.cancel_parked_poll();
        let abandoned = self.delivered_frame.borrow_mut().take().is_some();
        self.delivered_revision.set(0);
        if !abandoned {
            return false;
        }
        self.page_flushed();
        self.rebuild_from_scratch();
        true
    }

    fn cancel_parked_poll(&self) {
        if let Some(stale) = self.parked.borrow_mut().take() {
            self.cancel(stale);
        }
    }

    fn restart_rendering(&self) {
        if !self.invalidate_delivered_frame() {
            self.rebuild_from_scratch();
        }
    }

    fn rebuild_from_scratch(&self) {
        self.clear_transient_requests();
        self.remount_pending.set(true);
        self.mounted.set(false);
    }

    fn clear_transient_requests(&self) {
        self.requests.take();
        self.reads.clear();
        *self.selection.borrow_mut() = EventSelection::default();
    }

    fn respond(&self, responder: wry::RequestAsyncResponder, frame: PageFrame) {
        let sequence = self.next_frame_sequence.get().wrapping_add(1).max(1);
        self.next_frame_sequence.set(sequence);
        self.respond_attempt(responder, frame, sequence, 0);
    }

    fn respond_attempt(
        &self,
        responder: wry::RequestAsyncResponder,
        frame: PageFrame,
        sequence: u64,
        failures: u8,
    ) {
        let revision = self.navigation_revision.get();
        let epoch = self.ipc_epoch.get();
        let requests = frame
            .requests()
            .iter()
            .filter(|request| request.retryable())
            .cloned()
            .collect();
        self.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence,
            revision,
            failures,
            requests,
        });
        Self::send(
            responder,
            frame,
            Some((revision, sequence, epoch, self.frame_capability)),
        );
        self.delivered_revision.set(revision);
        self.lifecycle
            .update(LifecycleUpdate::FrameDelivered(revision));
    }

    fn send(
        responder: wry::RequestAsyncResponder,
        frame: PageFrame,
        identity: Option<(u64, u64, u64, u64)>,
    ) {
        let mut builder = wry::http::Response::builder()
            .header(wry::http::header::CONTENT_TYPE, "application/octet-stream");
        if let Some((revision, sequence, epoch, capability)) = identity {
            builder = builder.header(NAVIGATION_REVISION, revision.to_string());
            builder = builder
                .header(FRAME_SEQUENCE, sequence.to_string())
                .header(FRAME_EPOCH, epoch.to_string())
                .header(FRAME_CAPABILITY, capability.to_string());
        }
        let built = builder.body(frame.into_body());
        match built {
            Ok(response) => responder.respond(response),
            Err(error) => {
                error!("vmux_page: a frame would not build a response: {error}");
                responder.respond(wry::http::Response::new(Vec::new()));
            }
        }
    }

    fn handle_event(&self, event: EventRequest, selection: EventSelection) -> EventOutcome {
        let event = event.into_event();
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let Ok(mut page) = self.page.try_borrow_mut() else {
            warn!("vmux_page: an event arrived while the page was rendering");
            return EventOutcome::unreadable();
        };

        *self.selection.borrow_mut() = selection;
        let element = Element::new(event.element, self.requests.clone(), self.reads.clone());
        let outcome = page.handle(event, element);
        *self.selection.borrow_mut() = EventSelection::default();
        drop(page);
        self.waker.wake();
        self.lifecycle.update(LifecycleUpdate::UiEventHandled);

        outcome
    }

    pub(crate) fn deliver(&self, id: &str, payload: &[u8]) -> bool {
        let _reactor = self.reactor.enter();
        let _host = HostScope::enter(self.host.clone());
        let Ok(mut borrowed) = self.listeners.try_borrow_mut() else {
            warn!("vmux_page: a host emit arrived while the page was registering listeners");
            return false;
        };
        let Some(mut registered) = borrowed.remove(id) else {
            return false;
        };
        drop(borrowed);
        if registered.is_empty() {
            return false;
        }

        registered.retain_mut(|listener| listener(payload));
        self.waker.wake();
        self.lifecycle.update(LifecycleUpdate::HostEventDelivered);

        if let Ok(mut borrowed) = self.listeners.try_borrow_mut() {
            if let Some(mut added) = borrowed.remove(id) {
                registered.append(&mut added);
            }
            if !registered.is_empty() {
                borrowed.insert(id.to_string(), registered);
            }
        }
        true
    }
}

struct SurfaceHost {
    outbox: Rc<dyn Outbox>,
    listeners: Listeners,
    requests: RequestQueue,
    reads: PendingReads,
    selection: Rc<RefCell<EventSelection>>,
    lifecycle: Rc<dyn LifecycleSink>,
    navigation_revision: Rc<Cell<u64>>,
    ipc_epoch: Rc<Cell<u64>>,
    blocked: Rc<Cell<bool>>,
    ready_requested: Rc<Cell<bool>>,
    waker: Rc<dyn Wake>,
}

impl SurfaceHost {
    fn request(&self, request: DomRequest) {
        self.requests.push(request);
    }
}

impl PageHost for SurfaceHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        if id == vmux_api::PageReady::id() {
            tracing::info!(
                "vmux_page: page ready requested revision={} epoch={}",
                self.navigation_revision.get(),
                self.ipc_epoch.get(),
            );
            self.ready_requested.set(true);
            self.waker.wake();
            return Ok(());
        }
        if self.blocked.get() {
            return Err(EventListenerError::Unsupported);
        }
        self.outbox.send(
            id,
            bytes,
            self.navigation_revision.get(),
            self.ipc_epoch.get(),
        )
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        let mut listeners = self
            .listeners
            .try_borrow_mut()
            .map_err(|_| EventListenerError::Unsupported)?;
        listeners.entry(id.to_string()).or_default().push(on_bytes);
        self.lifecycle.update(LifecycleUpdate::ListenerRegistered);

        Ok(())
    }

    fn focus_element(&self, element_id: &str) {
        self.request(DomRequest::Focus {
            element: element_id.to_string(),
        });
    }

    fn show_element_popover(&self, element_id: &str) {
        self.request(DomRequest::ShowPopover {
            element: element_id.to_string(),
        });
    }

    fn show_dropdown_popover(&self, element_id: &str) {
        self.lifecycle.update(LifecycleUpdate::DropdownOpened);
        self.show_element_popover(element_id);
    }

    fn scroll_element_into_view(&self, element_id: &str) {
        self.request(DomRequest::ScrollIntoView {
            element: element_id.to_string(),
        });
    }

    fn scroll_element_into_view_vertical(&self, element_id: &str) {
        self.request(DomRequest::ScrollIntoViewVertical {
            element: element_id.to_string(),
        });
    }

    fn scroll_element_to(&self, element_id: &str, top: f64) {
        self.request(DomRequest::ScrollTo {
            element: element_id.to_string(),
            top,
            behavior: "instant",
        });
    }

    fn scroll_element_to_smooth(&self, element_id: &str, top: f64) {
        self.request(DomRequest::ScrollTo {
            element: element_id.to_string(),
            top,
            behavior: "smooth",
        });
    }

    fn animate_height(&self, element_id: &str, duration_ms: u32) {
        let duration_ms = vmux_ui::motion::Motion::bounded(duration_ms);
        self.request(DomRequest::AnimateHeight {
            element: element_id.to_string(),
            duration_ms,
        });
    }

    fn animate_layout(&self, element_id: &str, duration_ms: u32) {
        let duration_ms = vmux_ui::motion::Motion::bounded(duration_ms);
        self.request(DomRequest::AnimateLayout {
            element: element_id.to_string(),
            duration_ms,
        });
    }

    fn preserve_scroll(&self) {
        self.request(DomRequest::PreserveScroll);
    }

    fn reveal_first_rendered(&self, element_ids: &[&str], centered: bool) {
        self.request(DomRequest::RevealElement {
            elements: element_ids.iter().map(|id| id.to_string()).collect(),
            block: if centered { "center" } else { "nearest" },
        });
    }

    fn text_offset_at(&self, element_id: &str, x: f64, y: f64) -> TextOffsetAnswer {
        let measurement = self.reads.ask();
        self.request(DomRequest::TextOffsetAtPoint {
            element: element_id.to_string(),
            token: measurement.token(),
            x,
            y,
        });

        Box::pin(async move {
            let [offset, _, _, _] = measurement.await.ok()?;

            Some(offset.max(0.0) as u32)
        })
    }

    fn section_at_top(&self, container: &str) -> TextOffsetAnswer {
        let measurement = self.reads.ask();
        self.request(DomRequest::SectionAtTop {
            element: container.to_string(),
            token: measurement.token(),
        });
        Box::pin(async move {
            let [section, _, _, _] = measurement.await.ok()?;
            Some(section.max(0.0) as u32)
        })
    }

    fn scroll_section(
        &self,
        container: &str,
        element: &str,
        behavior: dioxus_html::ScrollBehavior,
    ) {
        self.request(DomRequest::ScrollSection {
            container: container.to_string(),
            element: element.to_string(),
            behavior: if behavior == dioxus_html::ScrollBehavior::Smooth {
                "smooth"
            } else {
                "instant"
            },
        });
    }

    fn select_element_text(&self, element_id: &str) {
        self.request(DomRequest::SelectAll {
            element: element_id.to_string(),
        });
    }

    fn toggle_media(&self, element_id: &str) {
        self.request(DomRequest::ToggleMedia {
            element: element_id.to_string(),
        });
    }

    fn offer_element_text(&self, element_id: &str) {
        self.request(DomRequest::OfferText {
            element: element_id.to_string(),
        });
    }

    fn write_to_clipboard(&self, text: &str) {
        vmux_clipboard::Clipboard::write(text.to_string());
    }

    fn event_field_selection(&self, element_id: &str) -> (usize, usize) {
        self.selection.borrow().in_field(element_id)
    }

    fn event_document_has_selection(&self) -> bool {
        self.selection.borrow().in_document()
    }

    fn resolves_keys(&self) -> bool {
        true
    }

    fn place_caret(&self, element_id: &str, byte: usize) {
        self.request(DomRequest::PlaceCaret {
            element: element_id.to_string(),
            byte,
        });
    }

    fn caret_to_end(&self, element_id: &str) {
        self.request(DomRequest::CaretToEnd {
            element: element_id.to_string(),
        });
    }

    fn run_script(&self, script: String) {
        self.request(DomRequest::RunScript { script });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    use base64::Engine;
    use dioxus::prelude::*;
    use dioxus_core::ElementId;
    use dioxus_html::{EventData, HtmlEvent, SerializedMouseData};
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::PageHost;

    use super::{
        DeferredEvent, DeferredEvents, DeliveredFrame, Dom, EventAdmission, FRAME_APPLIED,
        FRAME_CAPABILITY, FRAME_EPOCH, FRAME_FAILED, FramePoll, NAVIGATION_REVISION, SurfaceHost,
    };
    use crate::webview::dom_request::{DomRequest, RequestQueue};
    use crate::webview::event_selection::EventSelection;
    use crate::webview::measurement::PendingReads;
    use crate::{
        AssetReply, Assets, Embedding, EventRequest, Instance, LifecycleSink, LifecycleUpdate,
        Outbox, Wake,
    };

    struct TestOutbox;

    impl Outbox for TestOutbox {
        fn send(&self, _: &str, _: &[u8], _: u64, _: u64) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    struct TestAssets;

    impl Assets for TestAssets {
        fn fetch(&self, _: &str, reply: AssetReply) {
            reply.fail("unused")
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

    struct ReadyLifecycle(Rc<Cell<bool>>);

    impl LifecycleSink for ReadyLifecycle {
        fn update(&self, update: LifecycleUpdate) {
            if matches!(update, LifecycleUpdate::Ready(_)) {
                self.0.set(true);
            }
        }
    }

    struct RecordingLifecycle(Rc<RefCell<Vec<LifecycleUpdate>>>);

    impl LifecycleSink for RecordingLifecycle {
        fn update(&self, update: LifecycleUpdate) {
            self.0.borrow_mut().push(update);
        }
    }

    #[component]
    fn First() -> Element {
        rsx! { div { "first page" } }
    }

    #[component]
    fn Second() -> Element {
        rsx! { div { "second page" } }
    }

    fn embedding() -> Embedding {
        Embedding {
            outbox: Rc::new(TestOutbox),
            assets: Rc::new(TestAssets),
            waker: Rc::new(TestWake),
            lifecycle: Rc::new(TestLifecycle),
            navigation_revision: Rc::new(Cell::new(1)),
            ipc_epoch: Rc::new(Cell::new(1)),
        }
    }

    fn event_headers(event: HtmlEvent, revision: u64) -> wry::http::HeaderMap {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&event).expect("an event serializes"));
        let mut headers = wry::http::HeaderMap::new();
        headers.insert("dioxus-data", encoded.parse().unwrap());
        headers.insert("x-vmux-navigation", revision.to_string().parse().unwrap());
        headers.insert("x-vmux-ipc-epoch", "1".parse().unwrap());
        headers
    }

    fn click_event() -> HtmlEvent {
        HtmlEvent {
            element: ElementId(1),
            name: "click".to_string(),
            bubbles: true,
            data: EventData::Mouse(SerializedMouseData::default()),
        }
    }

    fn frame_poll(dom: &Dom, revision: u64, header: &str, sequence: u64) -> FramePoll {
        let request = wry::http::Request::builder()
            .header(NAVIGATION_REVISION, revision.to_string())
            .header(FRAME_EPOCH, dom.ipc_epoch.get().to_string())
            .header(FRAME_CAPABILITY, dom.frame_capability.to_string())
            .header(header, sequence.to_string())
            .body(Vec::new())
            .unwrap();

        FramePoll::from(&request).unwrap()
    }

    #[test]
    fn remount_replaces_the_document_without_replacing_the_surface() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        let first_render = dom.next_batch().unwrap();
        assert!(String::from_utf8_lossy(&first_render).contains("first page"));
        dom.page_flushed();

        dom.remount(Second, Instance::default());

        let second_frame = dom.next_frame().unwrap();
        assert!(matches!(
            second_frame.requests()[0].request(),
            DomRequest::Remount
        ));
        let second_render = second_frame.into_body();
        assert!(String::from_utf8_lossy(&second_render).contains("second page"));
        assert!(!String::from_utf8_lossy(&second_render).contains("first page"));
    }

    #[test]
    fn remount_rejects_events_from_the_previous_navigation() {
        let blocked = Rc::new(Cell::new(false));
        let dom = Dom::mount(First, Instance::default(), &embedding(), blocked.clone());
        let current = event_headers(click_event(), 1);
        assert!(matches!(
            dom.admit_event(&current),
            EventAdmission::Accepted(_)
        ));

        blocked.set(true);
        assert!(matches!(dom.admit_event(&current), EventAdmission::Refused));
        blocked.set(false);

        dom.remount(Second, Instance::default());

        assert!(matches!(dom.admit_event(&current), EventAdmission::Refused));
        assert!(matches!(
            dom.admit_event(&event_headers(click_event(), 2)),
            EventAdmission::Accepted(_)
        ));
    }

    #[test]
    fn blocked_lifecycle_event_waits_for_interaction_restore() {
        let blocked = Rc::new(Cell::new(true));
        let dom = Dom::mount(First, Instance::default(), &embedding(), blocked);
        let mounted = event_headers(
            HtmlEvent {
                element: ElementId(1),
                name: "mounted".to_string(),
                bubbles: false,
                data: EventData::Mounted,
            },
            1,
        );

        assert!(matches!(
            dom.admit_event(&mounted),
            EventAdmission::Deferred(_)
        ));
    }

    #[test]
    fn deferred_events_are_coalesced_bounded_and_identity_scoped() {
        let event = HtmlEvent {
            element: ElementId(1),
            name: "mounted".to_string(),
            bubbles: false,
            data: EventData::Mounted,
        };
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&event).unwrap());
        let mut coalesced = DeferredEvents::default();
        coalesced.push(DeferredEvent {
            revision: 1,
            epoch: 1,
            event: EventRequest::from_header(&encoded).unwrap(),
            selection: EventSelection::default(),
        });
        coalesced.push(DeferredEvent {
            revision: 2,
            epoch: 2,
            event: EventRequest::from_header(&encoded).unwrap(),
            selection: EventSelection::default(),
        });

        assert_eq!(coalesced.events.len(), 1);
        assert!(coalesced.take(1, 1).is_empty());

        let mut bounded = DeferredEvents::default();
        for index in 0..=DeferredEvents::LIMIT {
            let event = HtmlEvent {
                element: ElementId(index),
                name: "mounted".to_string(),
                bubbles: false,
                data: EventData::Mounted,
            };
            let encoded = base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&event).unwrap());
            bounded.push(DeferredEvent {
                revision: 1,
                epoch: 1,
                event: EventRequest::from_header(&encoded).unwrap(),
                selection: EventSelection::default(),
            });
        }

        assert_eq!(bounded.events.len(), DeferredEvents::LIMIT);
    }

    #[test]
    fn interaction_epoch_change_preserves_state_listeners_after_an_unacknowledged_frame() {
        let blocked = Rc::new(Cell::new(true));
        let dom = Dom::mount(First, Instance::default(), &embedding(), blocked.clone());
        let _ = dom.next_batch().unwrap();
        let delivered = Rc::new(Cell::new(0));
        let observed = delivered.clone();
        dom.host
            .listen(
                "state@1",
                Box::new(move |_| {
                    observed.set(observed.get() + 1);
                    true
                }),
            )
            .unwrap();
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 1,
            revision: 1,
            failures: 0,
            requests: Vec::new(),
        });
        blocked.set(false);
        dom.ipc_epoch.set(2);

        dom.interaction_epoch_changed(1);

        assert!(dom.deliver("state@1", &[]));
        assert_eq!(delivered.get(), 1);

        dom.remount(First, Instance::default());

        assert!(!dom.deliver("state@1", &[]));
        assert_eq!(delivered.get(), 1);
    }

    #[vmux_api::ui_state(Default)]
    struct PaintState {
        text: String,
    }

    #[component]
    fn PopulatedPage() -> Element {
        let state = vmux_ui::hooks::use_ui_state::<PaintState>().state;
        let state = state.read();
        rsx! { div {
            if state.text.is_empty() { "placeholder" } else { "{state.text}" }
        } }
    }

    #[test]
    fn an_unacknowledged_interaction_change_preserves_displayed_data_without_another_host_snapshot()
    {
        let blocked = Rc::new(Cell::new(true));
        let dom = Dom::mount(
            PopulatedPage,
            Instance::default(),
            &embedding(),
            blocked.clone(),
        );
        let _ = dom.next_batch().unwrap();
        for _ in 0..4 {
            dom.page_flushed();
            let _ = dom.next_batch();
        }
        let frame = vmux_api::UiStateFrame::<PaintState, PaintState>::Snapshot(PaintState {
            text: "already rendered data".to_string(),
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
        assert!(dom.deliver(PaintState::id(), &bytes));
        dom.page_flushed();
        let populated = dom.next_batch().unwrap();
        assert!(String::from_utf8_lossy(&populated).contains("already rendered data"));
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 2,
            revision: 1,
            failures: 0,
            requests: Vec::new(),
        });
        blocked.set(false);
        dom.ipc_epoch.set(2);
        dom.interaction_epoch_changed(1);

        assert!(dom.next_batch().is_none());
        let acknowledged = frame_poll(&dom, 1, FRAME_APPLIED, 2);
        assert!(acknowledged.acknowledge(&dom));
        assert!(dom.next_batch().is_none());
        let _ = dom.next_frame();
        dom.page_flushed();

        dom.remount(PopulatedPage, Instance::default());
        let replacement = dom.next_batch().unwrap();
        let replacement = String::from_utf8_lossy(&replacement);
        assert!(replacement.contains("placeholder"));
        assert!(!replacement.contains("already rendered data"));
    }

    #[test]
    fn first_document_load_keeps_an_unacknowledged_initial_frame() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        let _ = dom.next_batch().unwrap();
        assert!(dom.page.borrow().awaiting_flush());

        dom.document_loaded();

        assert_eq!(dom.navigation_revision.get(), 1);
        assert!(dom.requests.take().is_empty());
        assert!(dom.mounted.get());
        assert!(dom.page.borrow().awaiting_flush());
        dom.page_flushed();
        assert!(dom.next_batch().is_none());
    }

    #[test]
    fn first_document_load_before_the_initial_poll_keeps_the_initial_render() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );

        dom.document_loaded();

        assert_eq!(dom.navigation_revision.get(), 1);
        assert!(dom.requests.take().is_empty());
        let initial = dom.next_batch().unwrap();
        assert!(String::from_utf8_lossy(&initial).contains("first page"));
    }

    #[test]
    fn document_reload_rebuilds_the_current_page() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        let _ = dom.next_batch().unwrap();
        dom.page_flushed();
        dom.document_loaded();

        dom.document_loaded();

        assert_eq!(dom.navigation_revision.get(), 2);
        let rebuilt = dom.next_batch().unwrap();
        assert!(String::from_utf8_lossy(&rebuilt).contains("first page"));
    }

    #[test]
    fn malformed_event_is_refused_while_the_surface_is_blocked() {
        let blocked = Rc::new(Cell::new(true));
        let dom = Dom::mount(First, Instance::default(), &embedding(), blocked);
        let mut headers = wry::http::HeaderMap::new();
        headers.insert("x-vmux-navigation", "1".parse().unwrap());
        headers.insert("x-vmux-ipc-epoch", "1".parse().unwrap());

        assert!(matches!(dom.admit_event(&headers), EventAdmission::Refused));
    }

    #[test]
    fn frame_acknowledgement_identifies_the_revision_that_was_applied() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        dom.navigation_revision.set(7);
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 9,
            revision: 7,
            failures: 0,
            requests: Vec::new(),
        });
        let request = frame_poll(&dom, 7, FRAME_APPLIED, 9);
        let stale = frame_poll(&dom, 7, FRAME_APPLIED, 8);

        assert!(request.accepted_by(&dom));
        assert_eq!(request.acknowledged, Some(9));
        assert_eq!(stale.acknowledged, Some(8));
        assert!(
            !dom.delivered_frame
                .borrow()
                .as_ref()
                .is_some_and(|frame| { frame.sequence == stale.acknowledged.unwrap() })
        );
    }

    #[test]
    fn frame_failure_count_only_applies_to_the_delivered_frame() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        dom.navigation_revision.set(7);
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 9,
            revision: 7,
            failures: 0,
            requests: Vec::new(),
        });
        let failed = frame_poll(&dom, 7, FRAME_FAILED, 9);
        let stale = frame_poll(&dom, 7, FRAME_FAILED, 8);

        assert!(failed.accepted_by(&dom));
        assert_eq!(failed.failed, Some(9));
        assert_eq!(stale.failed, Some(8));
        assert_eq!(dom.delivered_frame.borrow().as_ref().unwrap().failures, 0);
    }

    #[test]
    fn updating_an_instance_requests_ready_for_the_new_navigation() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let embedding = Embedding {
            outbox: Rc::new(TestOutbox),
            assets: Rc::new(TestAssets),
            waker: Rc::new(TestWake),
            lifecycle: Rc::new(RecordingLifecycle(updates.clone())),
            navigation_revision: Rc::new(Cell::new(1)),
            ipc_epoch: Rc::new(Cell::new(1)),
        };
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding,
            Rc::new(Cell::new(false)),
        );
        let _ = dom.next_batch();
        updates.borrow_mut().clear();

        dom.update_instance(Instance::default());
        let _ = dom.next_batch();

        assert_eq!(
            updates.borrow().as_slice(),
            [LifecycleUpdate::Navigated(2), LifecycleUpdate::Ready(2)]
        );
    }

    #[derive(Clone, Default)]
    struct SharedLayout {
        mounts: Rc<Cell<usize>>,
        expanded: Rc<RefCell<Option<Signal<bool>>>>,
        observed: Rc<Cell<bool>>,
    }

    #[component]
    fn StatefulPage() -> Element {
        let layout = use_context::<SharedLayout>();
        let mounted = layout.mounts.clone();
        use_hook(move || mounted.set(mounted.get() + 1));
        let expanded = use_signal(|| false);
        layout.expanded.borrow_mut().replace(expanded);
        layout.observed.set(expanded());
        let route = consume_context::<String>();
        rsx! { aside { "shared layout" } main { "{route}" } span { "{expanded}" } }
    }

    #[test]
    fn navigation_and_focus_preserve_shared_layout_until_the_inflight_frame_is_acknowledged() {
        let layout = SharedLayout::default();
        let initial = layout.clone();
        let dom = Dom::mount(
            StatefulPage,
            Instance::from(move |scope: crate::PageScope<'_>| {
                scope.provide(initial.clone());
                scope.provide("first".to_string());
            }),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        let _ = dom.next_batch().unwrap();
        layout.expanded.borrow_mut().as_mut().unwrap().set(true);
        dom.page_flushed();
        let _ = dom.next_batch().unwrap();
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 9,
            revision: 1,
            failures: 0,
            requests: Vec::new(),
        });
        let next = layout.clone();
        dom.update_instance(Instance::from(move |scope: crate::PageScope<'_>| {
            scope.provide(next.clone());
            scope.provide("second".to_string());
        }));
        dom.ipc_epoch.set(2);
        dom.interaction_epoch_changed(1);
        assert!(dom.next_batch().is_none());
        assert!(matches!(
            dom.admit_event(&event_headers(
                HtmlEvent {
                    element: ElementId(1),
                    name: "click".to_string(),
                    bubbles: true,
                    data: EventData::Mouse(SerializedMouseData::default()),
                },
                1
            )),
            EventAdmission::Refused
        ));
        let mut acknowledgement = frame_poll(&dom, 2, FRAME_APPLIED, 9);
        acknowledgement.capability += 1;
        assert!(!acknowledgement.acknowledge(&dom));
        acknowledgement.capability = dom.frame_capability;
        acknowledgement.acknowledged = Some(8);
        assert!(!acknowledgement.acknowledge(&dom));
        acknowledgement.acknowledged = Some(9);
        assert!(acknowledgement.acknowledge(&dom));
        let frame = dom.next_frame().unwrap();
        assert!(
            frame
                .requests()
                .iter()
                .all(|queued| !matches!(queued.request(), DomRequest::Remount))
        );
        assert!(String::from_utf8_lossy(&frame.into_body()).contains("second"));
        assert_eq!(layout.mounts.get(), 1);
        assert!(layout.observed.get());
    }

    #[test]
    fn inactive_listener_does_not_leave_a_deliverable_empty_entry() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        dom.host.listen("state", Box::new(|_| false)).unwrap();

        assert!(dom.deliver("state", &[]));
        assert!(!dom.listeners.borrow().contains_key("state"));
        assert!(!dom.deliver("state", &[]));
    }

    #[test]
    fn a_pending_document_rebuild_keeps_its_remount_after_navigation_and_input_identity_change() {
        let dom = Dom::mount(
            First,
            Instance::default(),
            &embedding(),
            Rc::new(Cell::new(false)),
        );
        let _ = dom.next_frame().unwrap();
        dom.delivered_frame.borrow_mut().replace(DeliveredFrame {
            sequence: 1,
            revision: 1,
            failures: 0,
            requests: Vec::new(),
        });
        dom.restart_rendering();
        dom.ipc_epoch.set(2);
        dom.interaction_epoch_changed(1);
        dom.update_instance(Instance::default());
        dom.ipc_epoch.set(3);
        dom.interaction_epoch_changed(2);
        let body = dom.next_frame().unwrap().into_body();
        let length = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
        let requests: Vec<serde_json::Value> =
            serde_json::from_slice(&body[4..4 + length]).unwrap();
        let remounts = requests
            .iter()
            .filter(|request| request["kind"] == "remount")
            .collect::<Vec<_>>();
        assert_eq!(remounts.len(), 1);
        assert_eq!(remounts[0]["epoch"], 3);
        assert_eq!(remounts[0]["revision"], 2);
        assert!(body.len() > 4 + length);
        assert!(!dom.remount_pending.get());
    }

    #[test]
    fn ready_waits_until_the_current_render_has_registered_its_listeners() {
        let ready = Rc::new(Cell::new(false));
        let lifecycle: Rc<dyn LifecycleSink> = Rc::new(ReadyLifecycle(ready.clone()));
        let ready_requested = Rc::new(Cell::new(false));
        let blocked = Rc::new(Cell::new(true));
        let host = SurfaceHost {
            outbox: Rc::new(TestOutbox),
            listeners: Rc::new(RefCell::new(HashMap::new())),
            requests: RequestQueue::default(),
            reads: PendingReads::new(lifecycle.clone()),
            selection: Rc::default(),
            lifecycle,
            navigation_revision: Rc::new(Cell::new(1)),
            ipc_epoch: Rc::new(Cell::new(1)),
            blocked,
            ready_requested: ready_requested.clone(),
            waker: Rc::new(TestWake),
        };

        assert!(matches!(
            host.send("action@1", &[]),
            Err(EventListenerError::Unsupported)
        ));
        host.send(vmux_api::PageReady::id(), &[]).unwrap();

        assert!(!ready.get());
        assert!(ready_requested.get());
    }
}
