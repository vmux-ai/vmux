use std::cell::Cell;
use std::rc::Rc;

use dioxus::core::ReactiveContext;
use dioxus::prelude::*;
use futures_util::StreamExt;
use vmux_chat::event::{
    ChatApproval, ChatCancel, ChatComposerEffect, ChatDraftChanged, ChatEscape,
    ChatRemoveAttachment, ChatStop, ChatSubmit, SelectModel, SetAgentEffort,
};
use vmux_chat::host::{
    Attach, Attachments, Browsed, Conversation, Models, PublishComposerEffect, RemoveAttachment,
    Reported, RepublishChatUiState, Submitted,
};
use vmux_chat::state::ChatUiState;
use vmux_start::roster::RepublishLauncher;
use vmux_team::roster::{Members, RepublishTeam};

use crate::runtime::RuntimeHandle;
use vmux_api::BinEvent;
use vmux_api::command_bar::{
    CommandBarUiState, DismissRequest as CommandBarDismissRequest,
    ExRequest as CommandBarExRequest, InvokeRequest as CommandBarInvokeRequest,
    OpenRequest as CommandBarOpenRequest, PickRequest as CommandBarPickRequest,
    PromptRequest as CommandBarPromptRequest, SwitchSpaceRequest, SwitchTabRequest,
    TerminalRequest as CommandBarTerminalRequest,
};
use vmux_api::conversation::{
    AgentAttachment, ApprovalRequest, PromptRequest, RemoteEvent, RemoteMediaEntry, RemoteSession,
    RemoteStatus,
};
use vmux_api::prompt_media::{ChatAttachPaths, ChatAttachment, ChatMediaListRequest};
use vmux_api::team::TeamEvent;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostPayload, PageHost, install_host};
use vmux_ui::platform::Platform;

use crate::remote::{Api, ApiError, ClientOperationIds};
use crate::session::Session;

const TEAM_POLL_INTERVAL_MS: u32 = 3_000;

const MODEL_FETCH_ATTEMPTS: u8 = 5;

const MODEL_RETRY_INTERVAL_MS: u32 = 1_000;

struct MobileHost {
    epoch: u64,
    runtime: RuntimeHandle,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
}

thread_local! {
    static EPOCH: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn install(
    runtime: RuntimeHandle,
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
        runtime,
        api,
        sessions,
        session,
        composer,
    }));
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ComposerExchange {
    media_request: Signal<Option<ChatMediaListRequest>>,
    offered: Signal<Vec<RemoteMediaEntry>>,
    draft: Signal<String>,
    effect_revision: Signal<u64>,
}

pub(super) fn use_composer_exchange() -> ComposerExchange {
    ComposerExchange {
        media_request: use_signal(|| None),
        offered: use_signal(Vec::new),
        draft: use_signal(String::new),
        effect_revision: use_signal(|| 0),
    }
}

impl ComposerExchange {
    fn change_draft(self, text: String) {
        let mut draft = self.draft;
        let mut request = self.media_request;
        draft.set(text.clone());
        let query = vmux_api::prompt_media::InlineMediaQuery::parse(&text)
            .map(|query| query.query.to_string())
            .unwrap_or_default();
        if request
            .peek()
            .as_ref()
            .is_some_and(|request| request.query == query)
        {
            return;
        }
        let request_id = request
            .peek()
            .as_ref()
            .map(|request| request.request_id.wrapping_add(1).max(1))
            .unwrap_or(1);
        request.set(Some(ChatMediaListRequest { request_id, query }));
    }

    fn clear_effect(self) -> Option<ChatComposerEffect> {
        let mut draft = self.draft;
        if draft.peek().is_empty() {
            return None;
        }
        let mut revision = self.effect_revision;
        let next = revision().wrapping_add(1).max(1);
        revision.set(next);
        draft.set(String::new());
        Some(ChatComposerEffect {
            revision: next,
            draft: String::new(),
            focus: true,
        })
    }
}

impl PageHost for MobileHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        match id {
            ChatSubmit::ID => self.submit(Self::decode(bytes)?),
            ChatDraftChanged::ID => {
                let payload: ChatDraftChanged = Self::decode(bytes)?;
                self.composer.change_draft(payload.text);
                Ok(())
            }
            ChatRemoveAttachment::ID => self.remove_attachment(Self::decode(bytes)?),
            ChatCancel::ID | ChatStop::ID => self.cancel(),
            ChatEscape::ID => self.escape(),
            ChatApproval::ID => self.approve(Self::decode(bytes)?),
            SelectModel::ID => {
                let payload: SelectModel = Self::decode(bytes)?;
                self.agent_call(move |api, sid| async move {
                    if let Err(error) = api.select_model(&sid, &payload.model_id).await {
                        tracing::warn!("selecting the model failed: {error:?}");
                    }
                })
            }
            SetAgentEffort::ID => {
                let payload: SetAgentEffort = Self::decode(bytes)?;
                self.agent_call(move |api, sid| async move {
                    if let Err(error) = api.set_effort(&sid, &payload.level).await {
                        tracing::warn!("setting the effort failed: {error:?}");
                    }
                })
            }
            ChatAttachPaths::ID => self.attach(Self::decode(bytes)?),
            CommandBarPromptRequest::ID => self.prompt(Self::decode(bytes)?),
            SwitchTabRequest::ID => self.switch_tab(Self::decode(bytes)?),
            CommandBarDismissRequest::ID => Ok(()),
            CommandBarOpenRequest::ID
            | CommandBarTerminalRequest::ID
            | CommandBarInvokeRequest::ID
            | SwitchSpaceRequest::ID
            | CommandBarExRequest::ID
            | CommandBarPickRequest::ID => Err(EventListenerError::Unsupported),
            _ => Err(EventListenerError::Unsupported),
        }
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        match id {
            ChatUiState::ID => {
                self.poll_models();
                self.poll_media();
                self.runtime
                    .listen(ChatUiState::ID, on_bytes, RepublishChatUiState);
            }
            CommandBarUiState::ID => {
                self.runtime
                    .listen(CommandBarUiState::ID, on_bytes, RepublishLauncher);
            }
            TeamEvent::ID => {
                self.poll_team();
                self.runtime.listen(TeamEvent::ID, on_bytes, RepublishTeam);
            }
            _ => return Err(EventListenerError::Unsupported),
        }
        Ok(())
    }
}

impl MobileHost {
    fn superseded(epoch: u64) -> bool {
        EPOCH.with(|current| current.get()) != epoch
    }

    fn submit(&self, payload: ChatSubmit) -> Result<(), EventListenerError> {
        if self.session.sid().is_empty() {
            return Err(EventListenerError::Unsupported);
        }
        let attachments = self
            .runtime
            .project(|selected: &Attachments| {
                selected
                    .0
                    .iter()
                    .map(|attachment| AgentAttachment {
                        path: attachment.path.clone(),
                        name: attachment.name.clone(),
                        mime_type: attachment.mime_type.clone(),
                        size: attachment.size,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let effect = self.composer.clear_effect();
        self.runtime.send(Submitted);
        if let Some(effect) = effect {
            self.runtime.send(PublishComposerEffect(effect));
        }
        let runtime = self.runtime.clone();
        self.agent_call(move |api, sid| async move {
            let request = PromptRequest {
                client_op_id: ClientOperationIds::next(),
                text: payload.text,
                attachments,
            };
            if let Err(ApiError::Message(message)) = api.send_prompt(&sid, &request).await {
                Self::report(&runtime, RemoteStatus::Errored(message));
            }
        })
    }

    fn remove_attachment(&self, payload: ChatRemoveAttachment) -> Result<(), EventListenerError> {
        self.runtime.send(RemoveAttachment(payload.path));
        Ok(())
    }

    fn report(runtime: &RuntimeHandle, status: RemoteStatus) {
        runtime.send(Reported(RemoteEvent::Status { status }));
    }

    fn cancel(&self) -> Result<(), EventListenerError> {
        self.agent_call(|api, sid| async move {
            if let Err(error) = api.cancel(&sid).await {
                tracing::warn!("cancelling failed: {error:?}");
            }
        })
    }

    fn escape(&self) -> Result<(), EventListenerError> {
        let running = self
            .runtime
            .project(|conversation: &Conversation| {
                matches!(&conversation.status, RemoteStatus::Streaming)
            })
            .unwrap_or_default();
        if !running && let Some(effect) = self.composer.clear_effect() {
            self.runtime.send(PublishComposerEffect(effect));
        }
        self.cancel()
    }

    fn approve(&self, payload: ChatApproval) -> Result<(), EventListenerError> {
        self.runtime
            .send(Reported(RemoteEvent::Approval { approval: None }));
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
        self.runtime.send(Attach(resolved));
        Ok(())
    }

    fn prompt(&self, request: CommandBarPromptRequest) -> Result<(), EventListenerError> {
        self.runtime.send(crate::session::StartChatRequest {
            text: request.text,
            agent_url: request.target_url,
        });
        Ok(())
    }

    fn switch_tab(&self, request: SwitchTabRequest) -> Result<(), EventListenerError> {
        let Some(session) = self.sessions.read().get(request.index).cloned() else {
            return Err(EventListenerError::Unsupported);
        };
        self.runtime.send(crate::session::OpenSession(session));
        Ok(())
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

    fn poll_models(&self) {
        let (api, session) = (self.api.clone(), self.session);
        let runtime = self.runtime.clone();
        let epoch = self.epoch;
        let (rc, mut changed) = ReactiveContext::new();
        spawn(async move {
            loop {
                if Self::superseded(epoch) {
                    return;
                }
                let sid = rc.reset_and_run_in(|| session.sid());
                let mut attempts = MODEL_FETCH_ATTEMPTS;
                while !sid.is_empty() && attempts > 0 {
                    attempts -= 1;
                    let fetched = api.models(&sid).await;
                    if Self::superseded(epoch) {
                        return;
                    }
                    match fetched {
                        Ok(state) => {
                            runtime.send(Models(state));
                            break;
                        }
                        Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                        Err(ApiError::Message(_)) => Platform::sleep(MODEL_RETRY_INTERVAL_MS).await,
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
        let runtime = self.runtime.clone();
        let composer = self.composer;
        let mut offered = composer.offered;
        let epoch = self.epoch;
        let (rc, mut changed) = ReactiveContext::new();
        spawn(async move {
            loop {
                if Self::superseded(epoch) {
                    return;
                }
                let asked = rc.reset_and_run_in(|| composer.media_request.read().clone());
                let sid = session.sid();
                if let Some(request) = asked {
                    if request.query.is_empty() {
                        offered.set(Vec::new());
                        runtime.send(Browsed {
                            request_id: request.request_id,
                            query: request.query,
                            entries: Vec::new(),
                        });
                        if changed.next().await.is_none() {
                            return;
                        }
                        continue;
                    }
                    if sid.is_empty() {
                        if changed.next().await.is_none() {
                            return;
                        }
                        continue;
                    }
                    let fetched = api.media(&sid, &request.query).await;
                    if Self::superseded(epoch) {
                        return;
                    }
                    let current = composer.media_request.peek();
                    if current.as_ref().is_none_or(|current| {
                        current.request_id != request.request_id || current.query != request.query
                    }) {
                        if changed.next().await.is_none() {
                            return;
                        }
                        continue;
                    }
                    if let Ok(found) = fetched {
                        offered.set(found.clone());
                        runtime.send(Browsed {
                            request_id: request.request_id,
                            query: request.query,
                            entries: found,
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
        let runtime = self.runtime.clone();
        spawn(async move {
            loop {
                if Self::superseded(epoch) {
                    return;
                }
                let fetched = api.team().await;
                if Self::superseded(epoch) {
                    return;
                }
                match fetched {
                    Ok(members) => runtime.send(Members(members)),
                    Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                    Err(ApiError::Message(_)) => {}
                }
                Platform::sleep(TEAM_POLL_INTERVAL_MS).await;
            }
        });
    }

    fn decode<T>(bytes: &[u8]) -> Result<T, EventListenerError>
    where
        T: rkyv::Archive,
        T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            >,
    {
        HostPayload::new(bytes)
            .decode::<T>()
            .ok_or(EventListenerError::SerializePayload)
    }
}
