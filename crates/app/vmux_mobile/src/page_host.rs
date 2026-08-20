//! Serving desktop pages from the phone.
//!
//! A shared page speaks one language: it emits typed payloads under an event id and subscribes to
//! ids it wants pushed back. On the desktop those ids cross a process boundary into Bevy. Here they
//! cross the QUIC link instead, and the page cannot tell.
//!
//! What the desktop answers from a daemon holding the whole session, the phone answers from a
//! session row and a folded room log. So this file is the join: [`listen`](PageHost::listen) turns
//! the phone's state into the payloads a page expects to be pushed, and [`send`](PageHost::send)
//! turns a page's intent into a call on the link.
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
use vmux_start::event::{START_COMMAND_BAR_OPEN_EVENT, StartDataRequest};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostPayload, PageHost, install_host};
use vmux_ui::platform::sleep_ms;
use vmux_wire::command_bar::{
    CommandBarActionEvent, CommandBarOpenEvent, CommandBarPage, CommandBarTab, OpenId,
};
use vmux_wire::icon::PageIcon;
use vmux_wire::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachPaths, ChatAttachment,
    ChatAttachments, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
};
use vmux_wire::room::{
    AgentAttachment, ApprovalRequest, PromptRequest, RemoteAgent, RemoteMediaEntry, RemoteSession,
    RemoteStatus,
};
use vmux_wire::team::{TEAM_EVENT, TeamEvent, TeamMemberRow};

use crate::api::next_client_op_id;
use crate::session::Session;
use crate::{Api, ApiError};

/// How often the team roster re-reads the desktop.
///
/// It only moves when an agent starts or finishes, so staleness costs little and a push route has
/// not been worth adding. Everything else here is driven by state the phone already holds, so it
/// needs no interval at all.
const TEAM_POLL_INTERVAL_MS: u32 = 3_000;

pub(crate) struct MobileHost {
    /// Which installation this host is. A watcher compares it against [`EPOCH`] to find out that
    /// it is serving a page on behalf of a link nobody holds any more.
    epoch: u64,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    agents: Signal<Vec<RemoteAgent>>,
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
    agents: Signal<Vec<RemoteAgent>>,
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
        agents,
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
    attached: Signal<Vec<ChatAttachment>>,
}

pub(crate) fn use_composer_exchange() -> ComposerExchange {
    ComposerExchange {
        media_request: use_signal(|| None),
        offered: use_signal(Vec::new),
        attached: use_signal(Vec::new),
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
                let (session, agents) = (self.session, self.agents);
                watch(self.epoch, on_bytes, move || {
                    encode(&session.snapshot(&agents.read()))
                });
            }
            START_COMMAND_BAR_OPEN_EVENT => {
                let (sessions, agents) = (self.sessions, self.agents);
                watch(self.epoch, on_bytes, move || {
                    encode(&launcher(&sessions.read(), &agents.read()))
                });
            }
            CHAT_ATTACHMENTS_EVENT => {
                let attached = self.composer.attached;
                watch(self.epoch, on_bytes, move || {
                    let attachments = attached.read().clone();
                    if attachments.is_empty() {
                        return None;
                    }
                    encode(&ChatAttachments { attachments })
                });
            }
            MODEL_STATE_EVENT => self.watch_models(on_bytes),
            CHAT_MEDIA_ENTRIES_EVENT => self.watch_media(on_bytes),
            TEAM_EVENT => self.watch_team(on_bytes),
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
        let mut status = self.session.status;
        let mut attached = self.composer.attached;
        let mut attachments = Vec::with_capacity(payload.attachments.len());
        for attachment in payload.attachments {
            attachments.push(AgentAttachment {
                path: attachment.path,
                name: attachment.name,
                mime_type: attachment.mime_type,
                size: attachment.size,
            });
        }
        attached.set(Vec::new());
        // The relay answers a prompt with a status event, but not before the next round trip. The
        // desktop's own page is told immediately, so match it rather than leave the composer
        // looking idle over a turn that has already started.
        status.set(RemoteStatus::Streaming);
        self.agent_call(move |api, sid| async move {
            let request = PromptRequest {
                client_op_id: next_client_op_id(),
                text: payload.text,
                attachments,
            };
            if let Err(ApiError::Message(message)) = api.send_prompt(&sid, &request).await {
                status.set(RemoteStatus::Errored(message));
            }
        })
    }

    fn cancel(&self) -> Result<(), EventListenerError> {
        self.agent_call(|api, sid| async move {
            let _ = api.cancel(&sid).await;
        })
    }

    fn approve(&self, payload: ChatApproval) -> Result<(), EventListenerError> {
        let mut approval = self.session.approval;
        approval.set(None);
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
        let mut attached = self.composer.attached;
        let offered = self.composer.offered.read();
        let mut next = attached.peek().clone();
        for path in &payload.paths {
            if next.iter().any(|held| &held.path == path) {
                continue;
            }
            for entry in offered.iter() {
                if &entry.path != path || entry.is_dir {
                    continue;
                }
                next.push(ChatAttachment {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                    mime_type: entry.mime_type.clone(),
                    size: entry.size,
                    preview_data_url: entry.preview_data_url.clone(),
                });
                break;
            }
        }
        attached.set(next);
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
                self.session.open(self.api.clone(), session);
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
                if !sid.is_empty()
                    && let Ok(state) = api.models(&sid).await
                    && let Some(bytes) = encode(&ModelState {
                        current_model_id: state.selected_id,
                        models: state.models,
                        effort_current: state.effort,
                        effort_levels: state.effort_levels,
                        ..ModelState::default()
                    })
                {
                    on_bytes(&bytes);
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
                    && let Ok(found) = api.media(&sid, &request.query).await
                {
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
                if changed.next().await.is_none() {
                    return;
                }
            }
        });
    }

    fn watch_team(&self, mut on_bytes: BytesListener) {
        let (api, epoch) = (self.api.clone(), self.epoch);
        spawn(async move {
            let mut last: Option<Vec<TeamMemberRow>> = None;
            loop {
                if superseded(epoch) {
                    return;
                }
                match api.team().await {
                    Ok(members) => {
                        if last.as_ref() != Some(&members) {
                            let payload = TeamEvent {
                                members: members.clone(),
                            };
                            if let Some(bytes) = encode(&payload) {
                                on_bytes(&bytes);
                            }
                            last = Some(members);
                        }
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

/// Describe the desktop the way the shared launcher expects to be told about it.
///
/// Sessions become the open-stack rows and agents the prompt targets, which is the same shape the
/// desktop contributes — so the launcher ranks, filters and renders them without knowing one list
/// came over a relay.
fn launcher(sessions: &[RemoteSession], agents: &[RemoteAgent]) -> CommandBarOpenEvent {
    let mut tabs = Vec::with_capacity(sessions.len());
    for (index, session) in sessions.iter().enumerate() {
        let cwd = vmux_ui::file_icon::FilePath(&session.cwd).name();
        tabs.push(CommandBarTab {
            title: session.name.clone(),
            url: format!("vmux://agent/{sid}", sid = session.sid),
            pane_id: 0,
            // What comes back on activation, so it has to index the list this was built from.
            tab_index: index as u32,
            is_active: false,
            location: format!("{runtime} · {cwd}", runtime = session.runtime),
        });
    }
    let mut pages = Vec::with_capacity(agents.len());
    for agent in agents {
        pages.push(CommandBarPage {
            host: agent.id.clone(),
            url: agent.url.clone(),
            title: agent.name.clone(),
            keywords: Vec::new(),
            icon: PageIcon::favicon(agent.icon.clone()),
            shortcut: String::new(),
            prompt_target: true,
        });
    }
    CommandBarOpenEvent {
        // Documented as the start page's live-refresh id: reusing it is what stops each refresh
        // reading as a reopen and clobbering what is being typed.
        open_id: OpenId::NONE,
        tabs,
        pages,
        ..CommandBarOpenEvent::default()
    }
}

/// Push what `build` returns whenever a signal it read changes.
///
/// The desktop pushes because a daemon told it something. The phone has no such prompt: what a
/// page needs to know is already in a signal, so the listener is a subscription to that signal
/// rather than a poll. Scope-bound, so it stops when the page that subscribed goes away.
fn watch(
    epoch: u64,
    mut on_bytes: BytesListener,
    mut build: impl FnMut() -> Option<Vec<u8>> + 'static,
) {
    let (rc, mut changed) = ReactiveContext::new();
    spawn(async move {
        loop {
            if superseded(epoch) {
                return;
            }
            if let Some(bytes) = rc.reset_and_run_in(&mut build) {
                on_bytes(&bytes);
            }
            if changed.next().await.is_none() {
                return;
            }
        }
    });
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

fn encode<T>(payload: &T) -> Option<Vec<u8>>
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

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::room::RoomId;

    /// `RemoteSession` is another crate's type, so the fixture that builds one cannot be an
    /// inherent method however much it wants to be.
    trait Sample {
        fn sample(name: &str) -> Self;
    }

    impl Sample for RemoteSession {
        fn sample(name: &str) -> Self {
            Self {
                sid: format!("sid-{name}"),
                room_id: RoomId::for_session(name),
                title: String::new(),
                name: name.to_string(),
                runtime: "acp".to_string(),
                model: None,
                cwd: "/tmp/work".to_string(),
                status: RemoteStatus::Idle,
                approval: None,
                created_at_ms: 0,
            }
        }
    }

    /// The launcher hands an activated row back by index alone, so the list it was built from is
    /// the only thing that can name the session again. A row whose index stops addressing its own
    /// session opens the wrong conversation — silently, and only for whoever has two of them.
    #[test]
    fn every_offered_session_is_addressed_by_the_index_it_comes_back_as() {
        let sessions = vec![
            RemoteSession::sample("alpha"),
            RemoteSession::sample("beta"),
            RemoteSession::sample("gamma"),
        ];

        let offered = launcher(&sessions, &[]);

        assert_eq!(offered.tabs.len(), 3);
        for tab in &offered.tabs {
            let addressed = &sessions[tab.tab_index as usize];
            assert_eq!(tab.title, addressed.name);
        }
    }

    /// An agent has to arrive as something the launcher will send a prompt to. Contributed without
    /// this flag it renders as an ordinary row that opens a url, and the phone has no browser to
    /// open one in — so the agent would be listed and unreachable.
    #[test]
    fn every_offered_agent_accepts_a_prompt() {
        let agents = vec![RemoteAgent {
            id: "claude".to_string(),
            name: "Claude".to_string(),
            url: "vmux://agent/claude".to_string(),
            icon: String::new(),
        }];

        let offered = launcher(&[], &agents);

        assert_eq!(offered.pages.len(), 1);
        assert!(offered.pages[0].prompt_target);
        assert_eq!(offered.pages[0].url, "vmux://agent/claude");
    }

    /// Every refresh reuses the one id documented as "not a reopen". A real id here would reset
    /// the palette's input on each poll, deleting whatever was half-typed.
    #[test]
    fn a_refresh_does_not_read_as_a_reopen() {
        let offered = launcher(&[], &[]);

        assert!(!offered.open_id.is_open());
    }
}
