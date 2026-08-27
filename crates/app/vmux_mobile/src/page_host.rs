use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::core::ReactiveContext;
use dioxus::prelude::*;
use futures_util::StreamExt;
use vmux_chat::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatCancel, ChatEscape, ChatSubmit, MODEL_STATE_EVENT,
    SelectModel, SetAgentEffort,
};
use vmux_chat::model::{Models, Picker};
use vmux_chat::prompt::{Attach, Attachments, Browsed};
use vmux_chat::room::{Reported, Snapshot, Submitted};
use vmux_start::event::{START_COMMAND_BAR_OPEN_EVENT, StartDataRequest};
use vmux_start::roster::Launcher;
use vmux_team::roster::{Members, Team};

use crate::nav::Open;
use crate::runtime::World;
use crate::screen::Shown;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostPayload, PageHost};
use vmux_ui::platform::sleep_ms;
use vmux_wire::command_bar::CommandBarActionEvent;
use vmux_wire::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachPaths, ChatAttachment,
    ChatMediaListRequest,
};
use vmux_wire::room::{
    AgentAttachment, ApprovalRequest, PromptRequest, RemoteEvent, RemoteMediaEntry, RemoteSession,
    RemoteStatus,
};
use vmux_wire::team::TEAM_EVENT;

use crate::remote::next_client_op_id;
use crate::session::Session;
use crate::{Api, ApiError};

const TEAM_POLL_INTERVAL_MS: u32 = 3_000;

const MODEL_FETCH_ATTEMPTS: u8 = 5;

const MODEL_RETRY_INTERVAL_MS: u32 = 1_000;

pub(crate) struct MobileHost {
    epoch: u64,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
}

thread_local! {
    static EPOCH: Cell<u64> = const { Cell::new(0) };

    static INSTALLED: RefCell<Option<Rc<MobileHost>>> = const { RefCell::new(None) };
}

pub(crate) fn installed() -> Option<Rc<MobileHost>> {
    INSTALLED.with_borrow(Clone::clone)
}

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
    let host = Rc::new(MobileHost {
        epoch,
        api,
        sessions,
        session,
        composer,
    });
    INSTALLED.with_borrow_mut(|slot| *slot = Some(host));
}

fn superseded(epoch: u64) -> bool {
    EPOCH.with(|current| current.get()) != epoch
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ComposerExchange {
    media_request: Signal<Option<ChatMediaListRequest>>,
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
            return self.cancel();
        }
        if names::<ChatApproval>(id) {
            return self.approve(decode(bytes)?);
        }
        if names::<SelectModel>(id) {
            let payload: SelectModel = decode(bytes)?;
            return self.agent_call(move |api, sid| async move {
                if let Err(error) = api.select_model(&sid, &payload.model_id).await {
                    tracing::warn!("selecting the model failed: {error:?}");
                }
            });
        }
        if names::<SetAgentEffort>(id) {
            let payload: SetAgentEffort = decode(bytes)?;
            return self.agent_call(move |api, sid| async move {
                if let Err(error) = api.set_effort(&sid, &payload.level).await {
                    tracing::warn!("setting the effort failed: {error:?}");
                }
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
            MODEL_STATE_EVENT => {
                self.poll_models();
                World::with(|world| {
                    world.listen(MODEL_STATE_EVENT, on_bytes);
                    world.refresh::<Picker>();
                });
            }
            CHAT_MEDIA_ENTRIES_EVENT => {
                self.poll_media();
                World::with(|world| world.listen(CHAT_MEDIA_ENTRIES_EVENT, on_bytes));
            }
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

impl MobileHost {
    fn submit(&self, payload: ChatSubmit) -> Result<(), EventListenerError> {
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
        World::with(|world| world.send(Submitted));
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

    fn report(status: RemoteStatus) {
        World::with(|world| world.send(Reported(RemoteEvent::Status { status })));
    }

    fn cancel(&self) -> Result<(), EventListenerError> {
        self.agent_call(|api, sid| async move {
            if let Err(error) = api.cancel(&sid).await {
                tracing::warn!("cancelling failed: {error:?}");
            }
        })
    }

    fn approve(&self, payload: ChatApproval) -> Result<(), EventListenerError> {
        World::with(|world| world.send(Reported(RemoteEvent::Approval { approval: None })));
        self.agent_call(move |api, sid| async move {
            let request = ApprovalRequest {
                call_id: payload.call_id,
                decision: payload.decision,
            };
            if let Err(error) = api.approve(&sid, &request).await {
                tracing::warn!("approving failed: {error:?}");
            }
        })
    }

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
                self.session.attach(session);
                Ok(())
            }
            CommandBarActionEvent::Open { value, .. } => {
                crate::runtime::World::with(|world| world.send(Open(Shown::addressed(&value))));
                Ok(())
            }
            CommandBarActionEvent::Dismiss => Ok(()),
            CommandBarActionEvent::Terminal { .. }
            | CommandBarActionEvent::Command { .. }
            | CommandBarActionEvent::Space { .. } => Err(EventListenerError::Unsupported),
        }
    }

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

impl MobileHost {
    fn poll_models(&self) {
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
                    if superseded(epoch) {
                        return;
                    }
                    match fetched {
                        Ok(state) => {
                            World::with(|world| world.insert(Models(state)));
                            break;
                        }
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

    fn poll_media(&self) {
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
                        offered.set(found.clone());
                        World::with(|world| {
                            world.insert(Browsed {
                                request_id: request.request_id,
                                query: request.query,
                                entries: found,
                            });
                        });
                    }
                }
                if changed.next().await.is_none() {
                    return;
                }
            }
        });
    }

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
                    Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                    Err(ApiError::Message(_)) => {}
                }
                sleep_ms(TEAM_POLL_INTERVAL_MS).await;
            }
        });
    }
}

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
