//! Serving desktop pages from the phone.
//!
//! A shared page speaks one language: it emits typed payloads under an event id and subscribes to
//! ids it wants pushed back. On the desktop those ids cross a process boundary into Bevy. Here they
//! cross the QUIC link instead, and the page cannot tell.
//!
//! So this file is the join: [`send`](PageHost::send) turns a page's intent into a call on the
//! link, and [`listen`](PageHost::listen) says where what comes back is to be delivered.
//!
//! Where a payload is *built* has moved out. An id served by the world registers a listener and
//! nothing else, because a plugin in the page's own crate keeps it current — how a conversation
//! folds is the chat page's knowledge, not this app's. What is left is the ids that cannot be
//! answered without asking the Mac first, and they still run a loop of their own.
//!
//! Ids with no route are refused rather than silently accepted, so a half-served page reports as
//! much instead of rendering empty and looking broken. That refusal is quiet by design: a page
//! whose listener is turned down leaves the feature it drives unrendered, which is why mounting
//! the desktop's chat page against a link that carries no slash commands costs nothing.

use std::cell::Cell;
use std::rc::Rc;

use dioxus::core::ReactiveContext;
use dioxus::prelude::*;
use futures_util::StreamExt;
use vmux_chat::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatCancel, ChatEscape, ChatSubmit, MODEL_STATE_EVENT,
    ModelState, SelectModel, SetAgentEffort,
};
use vmux_chat::prompt::{Attach, Attachments};
use vmux_chat::room::{Reported, Snapshot};
use vmux_start::event::{START_COMMAND_BAR_OPEN_EVENT, StartDataRequest};
use vmux_start::roster::Launcher;
use vmux_team::roster::{Members, Team};

use crate::runtime::World;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostPayload, PageHost, install_host};
use vmux_ui::platform::sleep_ms;
use vmux_wire::command_bar::CommandBarActionEvent;
use vmux_wire::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachPaths, ChatAttachment,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
};
use vmux_wire::room::{
    AgentAttachment, ApprovalRequest, PromptRequest, RemoteEvent, RemoteMediaEntry, RemoteSession,
    RemoteStatus,
};
use vmux_wire::team::TEAM_EVENT;

use crate::remote::next_client_op_id;
use crate::session::Session;
use crate::{Api, ApiError};

/// How often the team roster re-reads the desktop.
///
/// It only moves when an agent starts or finishes, so staleness costs little and a push route has
/// not been worth adding. Everything else here is driven by state the phone already holds, so it
/// needs no interval at all.
const TEAM_POLL_INTERVAL_MS: u32 = 3_000;

/// How long to keep asking for a session's models before giving up on it.
///
/// A conversation the phone itself started is asked about before the Mac has finished registering
/// it, so the first answer is a refusal that resolves on its own within a second or so. Anything
/// still refusing after this many tries is refusing for a reason retrying will not fix.
const MODEL_FETCH_ATTEMPTS: u8 = 5;

const MODEL_RETRY_INTERVAL_MS: u32 = 1_000;

pub(crate) struct MobileHost {
    /// Which installation this host is. A watcher compares it against [`EPOCH`] to find out that
    /// it is serving a page on behalf of a link nobody holds any more.
    epoch: u64,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
}

thread_local! {
    /// Bumped by every [`install`].
    ///
    /// `install_host` replaces the thread-local host but hands back nothing, so a host being
    /// superseded is never told and cannot stop the tasks it started. Those tasks hold a clone of
    /// an `Api` that has since been closed, and the team watcher retries a non-terminal failure
    /// forever — so a re-pair would leave a loop dialling an endpoint nobody is listening on.
    ///
    /// In practice every re-pair passes through `AuthState::Unpaired`, which unmounts the pages
    /// and takes their scope-bound tasks with them. That is a property of how the shell happens to
    /// branch, though, not of anything here, and it is not what a watcher should be relying on.
    static EPOCH: Cell<u64> = const { Cell::new(0) };
}

/// Route shared pages through `api`, and retire whatever was routing them before.
pub(crate) fn install(
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
) {
    let epoch = EPOCH.with(|epoch| {
        let next = epoch.get().wrapping_add(1);
        epoch.set(next);
        next
    });
    install_host(Rc::new(MobileHost {
        epoch,
        api,
        sessions,
        session,
        composer,
    }));
}

/// Whether a later [`install`] has replaced the host a watcher was started by.
fn superseded(epoch: u64) -> bool {
    EPOCH.with(|current| current.get()) != epoch
}

/// The two exchanges the composer opens that are answered by pushing an event back.
///
/// Everything else a page subscribes to is a view of state the phone already holds, so a listener
/// can just watch it. These two are questions: the page asks in `send` and waits for an answer at
/// a listener, and these signals are the join between the two halves.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ComposerExchange {
    media_request: Signal<Option<ChatMediaListRequest>>,
    /// What the last `@`-mention answer offered, so an attach naming a path can describe it.
    ///
    /// The desktop reads the file to answer; the phone never had it, and asking the Mac a second
    /// time for what it just sent would be a round trip to learn nothing.
    offered: Signal<Vec<RemoteMediaEntry>>,
}

pub(crate) fn use_composer_exchange() -> ComposerExchange {
    ComposerExchange {
        media_request: use_signal(|| None),
        offered: use_signal(Vec::new),
    }
}

impl PageHost for MobileHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        if names::<ChatSubmit>(id) {
            return self.submit(decode(bytes)?);
        }
        if names::<ChatCancel>(id) || names::<ChatEscape>(id) {
            // Nothing queues behind a turn on the phone, so there is no queue for escape to flush
            // and the two mean the same thing.
            return self.cancel();
        }
        if names::<ChatApproval>(id) {
            return self.approve(decode(bytes)?);
        }
        if names::<SelectModel>(id) {
            let payload: SelectModel = decode(bytes)?;
            return self.agent_call(move |api, sid| async move {
                let _ = api.select_model(&sid, &payload.model_id).await;
            });
        }
        if names::<SetAgentEffort>(id) {
            let payload: SetAgentEffort = decode(bytes)?;
            return self.agent_call(move |api, sid| async move {
                let _ = api.set_effort(&sid, &payload.level).await;
            });
        }
        if names::<ChatMediaListRequest>(id) {
            let mut request = self.composer.media_request;
            request.set(Some(decode(bytes)?));
            return Ok(());
        }
        if names::<ChatAttachPaths>(id) {
            return self.attach(decode(bytes)?);
        }
        if names::<CommandBarActionEvent>(id) {
            return self.act(decode(bytes)?);
        }
        if names::<StartDataRequest>(id) {
            // The launcher's listener already watches the session and agent lists, so it is
            // current before the page can ask. Accepting says so; refusing would only log.
            return Ok(());
        }
        Err(EventListenerError::Unsupported)
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        match id {
            CHAT_SNAPSHOT_EVENT => {
                World::with(|world| {
                    world.listen(CHAT_SNAPSHOT_EVENT, on_bytes);
                    world.refresh::<Snapshot>();
                });
            }
            // The world serves this one. `StartPagePlugin` keeps the payload current and emits it;
            // registering here only says where it lands, and asks for one now because the page has
            // just mounted and the last change may be long past.
            START_COMMAND_BAR_OPEN_EVENT => {
                World::with(|world| {
                    world.listen(START_COMMAND_BAR_OPEN_EVENT, on_bytes);
                    world.refresh::<Launcher>();
                });
            }
            CHAT_ATTACHMENTS_EVENT => {
                World::with(|world| {
                    world.listen(CHAT_ATTACHMENTS_EVENT, on_bytes);
                    world.refresh::<Attachments>();
                });
            }
            MODEL_STATE_EVENT => self.watch_models(on_bytes),
            CHAT_MEDIA_ENTRIES_EVENT => self.watch_media(on_bytes),
            // Served by the world now, like the launcher: the poll keeps  current and
            //  emits from it. Registering here only says where that lands.
            TEAM_EVENT => {
                self.poll_team();
                World::with(|world| {
                    world.listen(TEAM_EVENT, on_bytes);
                    world.refresh::<Team>();
                });
            }
            _ => return Err(EventListenerError::Unsupported),
        }
        Ok(())
    }
}

/// What a page asks the phone to do.
impl MobileHost {
    fn submit(&self, payload: ChatSubmit) -> Result<(), EventListenerError> {
        // Ahead of any mutation: the optimistic status and the cleared attachments below are only
        // honest if the call they are anticipating is actually going to be made.
        if self.session.sid().is_empty() {
            return Err(EventListenerError::Unsupported);
        }
        let mut attachments = Vec::with_capacity(payload.attachments.len());
        for attachment in payload.attachments {
            attachments.push(AgentAttachment {
                path: attachment.path,
                name: attachment.name,
                mime_type: attachment.mime_type,
                size: attachment.size,
            });
        }
        World::with(|world| world.insert(Attachments::default()));
        // The relay answers a prompt with a status event, but not before the next round trip. The
        // desktop's own page is told immediately, so match it rather than leave the composer
        // looking idle over a turn that has already started.
        Self::report(RemoteStatus::Streaming);
        self.agent_call(move |api, sid| async move {
            let request = PromptRequest {
                client_op_id: next_client_op_id(),
                text: payload.text,
                attachments,
            };
            if let Err(ApiError::Message(message)) = api.send_prompt(&sid, &request).await {
                MobileHost::report(RemoteStatus::Errored(message));
            }
        })
    }

    /// Tell the world where the run has got to, ahead of the relay saying so.
    fn report(status: RemoteStatus) {
        World::with(|world| world.send(Reported(RemoteEvent::Status { status })));
    }

    fn cancel(&self) -> Result<(), EventListenerError> {
        self.agent_call(|api, sid| async move {
            let _ = api.cancel(&sid).await;
        })
    }

    fn approve(&self, payload: ChatApproval) -> Result<(), EventListenerError> {
        // The prompt goes away as the decision is made, not a round trip later.
        World::with(|world| world.send(Reported(RemoteEvent::Approval { approval: None })));
        self.agent_call(move |api, sid| async move {
            let request = ApprovalRequest {
                call_id: payload.call_id,
                decision: payload.decision,
            };
            let _ = api.approve(&sid, &request).await;
        })
    }

    /// Turn `@`-mention paths into the attachments the composer shows as pills.
    ///
    /// The page hands back paths it was offered, so the entries behind them are still here and no
    /// second round trip is needed to describe them.
    fn attach(&self, payload: ChatAttachPaths) -> Result<(), EventListenerError> {
        let offered = self.composer.offered.read();
        let mut resolved = Vec::with_capacity(payload.paths.len());
        for path in &payload.paths {
            for entry in offered.iter() {
                if &entry.path != path || entry.is_dir {
                    continue;
                }
                resolved.push(ChatAttachment {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                    mime_type: entry.mime_type.clone(),
                    size: entry.size,
                    preview_data_url: entry.preview_data_url.clone(),
                });
                break;
            }
        }
        World::with(|world| world.send(Attach(resolved)));
        Ok(())
    }

    /// Take what the launcher was activated on.
    fn act(&self, action: CommandBarActionEvent) -> Result<(), EventListenerError> {
        match action {
            CommandBarActionEvent::Prompt {
                text, target_url, ..
            } => {
                self.session
                    .start_chat(self.api.clone(), self.sessions, text, target_url);
                Ok(())
            }
            CommandBarActionEvent::SwitchTab { index, .. } => {
                let Some(session) = self.sessions.read().get(index).cloned() else {
                    return Err(EventListenerError::Unsupported);
                };
                self.session.open(session);
                Ok(())
            }
            // Nothing was opened, so there is nothing to dismiss — but the launcher is entitled to
            // say the user backed out of it.
            CommandBarActionEvent::Dismiss => Ok(()),
            // A browser, a terminal, a command registry and a space switcher are all things the
            // desktop has and the phone does not.
            CommandBarActionEvent::Open { .. }
            | CommandBarActionEvent::Terminal { .. }
            | CommandBarActionEvent::Command { .. }
            | CommandBarActionEvent::Space { .. } => Err(EventListenerError::Unsupported),
        }
    }

    /// Run `call` against the open session, refusing when there is none.
    fn agent_call<F, Fut>(&self, call: F) -> Result<(), EventListenerError>
    where
        F: FnOnce(Api, String) -> Fut + 'static,
        Fut: std::future::Future<Output = ()> + 'static,
    {
        let sid = self.session.sid();
        if sid.is_empty() {
            return Err(EventListenerError::Unsupported);
        }
        let api = self.api.clone();
        spawn(call(api, sid));
        Ok(())
    }
}

/// What the phone pushes back at a page.
impl MobileHost {
    /// The session's models and effort, re-read whenever the session changes.
    ///
    /// Fetched per session rather than carried on the session row, because the list arrives from
    /// the agent after the session exists and a stale copy would offer models it has since
    /// dropped.
    ///
    /// A refusal is retried rather than dropped. Waiting on the session to *change* is right for a
    /// conversation the phone opened — there is nothing else to wait for — but wrong for one it
    /// just created: the Mac is still registering the session when the page mounts and asks, so the
    /// first answer is `no such session` for a session that exists a moment later. The sid never
    /// changes after that, so a dropped first answer left the composer with no models for the life
    /// of the conversation.
    fn watch_models(&self, mut on_bytes: BytesListener) {
        let (api, session) = (self.api.clone(), self.session);
        let epoch = self.epoch;
        let (rc, mut changed) = ReactiveContext::new();
        spawn(async move {
            loop {
                if superseded(epoch) {
                    return;
                }
                let sid = rc.reset_and_run_in(|| session.sid());
                let mut attempts = MODEL_FETCH_ATTEMPTS;
                while !sid.is_empty() && attempts > 0 {
                    attempts -= 1;
                    let fetched = api.models(&sid).await;
                    // The link can be replaced while a request is in flight, and an answer that
                    // arrives after that belongs to a page nobody is looking at any more.
                    if superseded(epoch) {
                        return;
                    }
                    match fetched {
                        Ok(state) => {
                            if let Some(bytes) = encode(&ModelState {
                                current_model_id: state.selected_id,
                                models: state.models,
                                effort_current: state.effort,
                                effort_levels: state.effort_levels,
                                ..ModelState::default()
                            }) {
                                on_bytes(&bytes);
                            }
                            break;
                        }
                        // Pairing is gone, or the session is. Neither is fixed by asking again.
                        Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                        Err(ApiError::Message(_)) => sleep_ms(MODEL_RETRY_INTERVAL_MS).await,
                    }
                }
                if changed.next().await.is_none() {
                    return;
                }
            }
        });
    }

    fn watch_media(&self, mut on_bytes: BytesListener) {
        let (api, session) = (self.api.clone(), self.session);
        let composer = self.composer;
        let mut offered = composer.offered;
        let epoch = self.epoch;
        let (rc, mut changed) = ReactiveContext::new();
        spawn(async move {
            loop {
                if superseded(epoch) {
                    return;
                }
                let asked = rc.reset_and_run_in(|| composer.media_request.read().clone());
                let sid = session.sid();
                if let Some(request) = asked
                    && !sid.is_empty()
                {
                    let fetched = api.media(&sid, &request.query).await;
                    if superseded(epoch) {
                        return;
                    }
                    if let Ok(found) = fetched {
                        let mut entries = Vec::with_capacity(found.len());
                        for entry in &found {
                            entries.push(ChatMediaEntry {
                                path: entry.path.clone(),
                                name: entry.name.clone(),
                                parent: entry.parent.clone(),
                                mime_type: entry.mime_type.clone(),
                                is_dir: entry.is_dir,
                                preview_data_url: entry.preview_data_url.clone(),
                            });
                        }
                        offered.set(found);
                        if let Some(bytes) = encode(&ChatMediaEntries {
                            request_id: request.request_id,
                            query: request.query,
                            entries,
                        }) {
                            on_bytes(&bytes);
                        }
                    }
                }
                if changed.next().await.is_none() {
                    return;
                }
            }
        });
    }

    /// Keep the world's roster current, and let `TeamPagePlugin` decide what reaches the page.
    ///
    /// The dedupe this used to keep in a `last` local is gone: [`World::insert`] already refuses a
    /// value equal to the one it holds, so an unchanged poll marks nothing changed and the emit
    /// system does not run.
    fn poll_team(&self) {
        let (api, epoch) = (self.api.clone(), self.epoch);
        spawn(async move {
            loop {
                if superseded(epoch) {
                    return;
                }
                let fetched = api.team().await;
                if superseded(epoch) {
                    return;
                }
                match fetched {
                    Ok(members) => {
                        World::with(|world| world.insert(Members(members)));
                    }
                    // Pairing is gone, or there is no such session. Neither is fixed by asking
                    // again every few seconds.
                    Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                    // Anything else is likely the network, which does heal.
                    Err(ApiError::Message(_)) => {}
                }
                sleep_ms(TEAM_POLL_INTERVAL_MS).await;
            }
        });
    }
}

/// Whether `id` names `T`, as [`vmux_ui::hooks::send`] writes it.
///
/// A page emits under `std::any::type_name`, not under the `&str` constants a listener subscribes
/// by. Asking the same expression rather than comparing a copied literal is what keeps moving a
/// payload type a compile error instead of a silently dead arm.
fn names<T: ?Sized>(id: &str) -> bool {
    id == std::any::type_name::<T>()
}

fn decode<T>(bytes: &[u8]) -> Result<T, EventListenerError>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    HostPayload::new(bytes)
        .decode::<T>()
        .ok_or(EventListenerError::SerializePayload)
}

pub(crate) fn encode<T>(payload: &T) -> Option<Vec<u8>>
where
    T: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload).ok()?;
    Some(bytes.to_vec())
}
