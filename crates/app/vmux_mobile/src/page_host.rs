use std::cell::Cell;
use std::rc::Rc;

use dioxus::core::ReactiveContext;
use dioxus::prelude::*;
use futures_util::StreamExt;
use vmux_chat::event::{
    ChatApproval, ChatCancel, ChatEscape, ChatSnapshot, ChatSubmit, ModelState, SelectModel,
    SetAgentEffort,
};
use vmux_chat::model::{Models, Picker};
use vmux_chat::prompt::{Attach, Attachments, Browsed};
use vmux_chat::room::{Reported, Snapshot, Submitted};
use vmux_start::event::StartDataRequest;
use vmux_start::roster::Launcher;
use vmux_team::roster::{Members, Team};

use crate::runtime::World;
use vmux_api::command_bar::{CommandBarOpenEvent, CommandBarRequest};
use vmux_api::prompt_media::{
    ChatAttachPaths, ChatAttachment, ChatAttachmentPreviews, ChatAttachments, ChatMediaEntries,
    ChatMediaListRequest,
};
use vmux_api::room::{
    AgentAttachment, ApprovalRequest, PromptRequest, RemoteEvent, RemoteMediaEntry, RemoteSession,
    RemoteStatus,
};
use vmux_api::team::TeamEvent;
use vmux_api::{BinEvent, BinEventTarget};
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostPayload, PageHost, install_host};
use vmux_ui::platform::sleep_ms;

use crate::remote::next_client_op_id;
use crate::session::Session;
use crate::{Api, ApiError};

const TEAM_POLL_INTERVAL_MS: u32 = 3_000;

const MODEL_FETCH_ATTEMPTS: u8 = 5;

const MODEL_RETRY_INTERVAL_MS: u32 = 1_000;

const MOBILE_PAGE_HOSTS: &[&str] = &["sessions", "agent", "start"];

pub(crate) struct MobileHost {
    epoch: u64,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
}

thread_local! {
    static EPOCH: Cell<u64> = const { Cell::new(0) };
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
    install_host(Rc::new(MobileHost {
        epoch,
        api,
        sessions,
        session,
        composer,
    }));
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
    fn send(
        &self,
        target: BinEventTarget,
        id: &str,
        bytes: &[u8],
    ) -> Result<(), EventListenerError> {
        if !target.accepts_any(MOBILE_PAGE_HOSTS) {
            return Err(EventListenerError::Unsupported);
        }
        match id {
            ChatSubmit::ID => self.submit(decode(bytes)?),
            ChatCancel::ID | ChatEscape::ID => self.cancel(),
            ChatApproval::ID => self.approve(decode(bytes)?),
            SelectModel::ID => {
                let payload: SelectModel = decode(bytes)?;
                self.agent_call(move |api, sid| async move {
                    if let Err(error) = api.select_model(&sid, &payload.model_id).await {
                        tracing::warn!("selecting the model failed: {error:?}");
                    }
                })
            }
            SetAgentEffort::ID => {
                let payload: SetAgentEffort = decode(bytes)?;
                self.agent_call(move |api, sid| async move {
                    if let Err(error) = api.set_effort(&sid, &payload.level).await {
                        tracing::warn!("setting the effort failed: {error:?}");
                    }
                })
            }
            ChatMediaListRequest::ID => {
                let mut request = self.composer.media_request;
                request.set(Some(decode(bytes)?));
                Ok(())
            }
            ChatAttachPaths::ID => self.attach(decode(bytes)?),
            CommandBarRequest::ID => self.act(decode(bytes)?),
            StartDataRequest::ID => Ok(()),
            _ => Err(EventListenerError::Unsupported),
        }
    }

    fn listen(
        &self,
        target: BinEventTarget,
        id: &str,
        on_bytes: BytesListener,
    ) -> Result<(), EventListenerError> {
        if !target.accepts_any(MOBILE_PAGE_HOSTS) {
            return Err(EventListenerError::Unsupported);
        }
        match id {
            ChatSnapshot::ID => {
                World::with(|world| {
                    world.listen(ChatSnapshot::ID, on_bytes);
                    world.refresh::<Snapshot>();
                });
            }
            CommandBarOpenEvent::ID => {
                World::with(|world| {
                    world.listen(CommandBarOpenEvent::ID, on_bytes);
                    world.refresh::<Launcher>();
                });
            }
            ChatAttachments::ID => {
                World::with(|world| {
                    world.listen(ChatAttachments::ID, on_bytes);
                    world.refresh::<Attachments>();
                });
            }
            ChatAttachmentPreviews::ID => {
                World::with(|world| {
                    world.listen(ChatAttachmentPreviews::ID, on_bytes);
                    world.refresh::<Attachments>();
                });
            }
            ModelState::ID => {
                self.poll_models();
                World::with(|world| {
                    world.listen(ModelState::ID, on_bytes);
                    world.refresh::<Picker>();
                });
            }
            ChatMediaEntries::ID => {
                self.poll_media();
                World::with(|world| world.listen(ChatMediaEntries::ID, on_bytes));
            }
            TeamEvent::ID => {
                self.poll_team();
                World::with(|world| {
                    world.listen(TeamEvent::ID, on_bytes);
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

    fn act(&self, action: CommandBarRequest) -> Result<(), EventListenerError> {
        match action {
            CommandBarRequest::Prompt {
                text, target_url, ..
            } => {
                self.session
                    .start_chat(self.api.clone(), self.sessions, text, target_url);
                Ok(())
            }
            CommandBarRequest::SwitchTab { index, .. } => {
                let Some(session) = self.sessions.read().get(index).cloned() else {
                    return Err(EventListenerError::Unsupported);
                };
                self.session.open(session);
                Ok(())
            }
            CommandBarRequest::Dismiss => Ok(()),
            CommandBarRequest::Open { .. }
            | CommandBarRequest::Terminal { .. }
            | CommandBarRequest::Command { .. }
            | CommandBarRequest::Space { .. }
            | CommandBarRequest::Ex { .. }
            | CommandBarRequest::Pick { .. } => Err(EventListenerError::Unsupported),
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
