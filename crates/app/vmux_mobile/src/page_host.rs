use std::cell::Cell;
use std::rc::Rc;

use bevy_ecs::change_detection::DetectChangesMut;
use dioxus::core::ReactiveContext;
use dioxus::prelude::*;
use futures_util::StreamExt;
use vmux_chat::event::{
    ChatApproval, ChatCancel, ChatEscape, ChatSubmit, SelectModel, SetAgentEffort,
};
use vmux_chat::model::{Models, Picker};
use vmux_chat::prompt::{Attach, Attachments, Browsed, Media};
use vmux_chat::room::{Reported, Snapshot, Submitted};
use vmux_chat::state::ChatUiState;
use vmux_start::event::StartDataRequest;
use vmux_start::roster::Launcher;
use vmux_team::roster::{Members, Team};

use crate::runtime::{PageListeners, RuntimeHandle};
use vmux_api::command_bar::{
    CommandBarUiState, DismissRequest as CommandBarDismissRequest,
    ExRequest as CommandBarExRequest, InvokeRequest as CommandBarInvokeRequest,
    OpenRequest as CommandBarOpenRequest, PickRequest as CommandBarPickRequest,
    PromptRequest as CommandBarPromptRequest, SwitchSpaceRequest, SwitchTabRequest,
    TerminalRequest as CommandBarTerminalRequest,
};
use vmux_api::prompt_media::{ChatAttachPaths, ChatAttachment, ChatMediaListRequest};
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
    runtime: RuntimeHandle,
    api: Api,
    sessions: Signal<Vec<RemoteSession>>,
    session: Session,
    composer: ComposerExchange,
}

thread_local! {
    static EPOCH: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn install(
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
            ChatSubmit::ID => submit(self, decode(bytes)?),
            ChatCancel::ID | ChatEscape::ID => cancel(self),
            ChatApproval::ID => approve(self, decode(bytes)?),
            SelectModel::ID => {
                let payload: SelectModel = decode(bytes)?;
                agent_call(self, move |api, sid| async move {
                    if let Err(error) = api.select_model(&sid, &payload.model_id).await {
                        tracing::warn!("selecting the model failed: {error:?}");
                    }
                })
            }
            SetAgentEffort::ID => {
                let payload: SetAgentEffort = decode(bytes)?;
                agent_call(self, move |api, sid| async move {
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
            ChatAttachPaths::ID => attach(self, decode(bytes)?),
            CommandBarPromptRequest::ID => prompt(self, decode(bytes)?),
            SwitchTabRequest::ID => switch_tab(self, decode(bytes)?),
            CommandBarDismissRequest::ID => Ok(()),
            CommandBarOpenRequest::ID
            | CommandBarTerminalRequest::ID
            | CommandBarInvokeRequest::ID
            | SwitchSpaceRequest::ID
            | CommandBarExRequest::ID
            | CommandBarPickRequest::ID => Err(EventListenerError::Unsupported),
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
            ChatUiState::ID => {
                poll_models(self);
                poll_media(self);
                let mut runtime = self.runtime.borrow_mut();
                let world = runtime.app.world_mut();
                world
                    .non_send_mut::<PageListeners>()
                    .0
                    .insert(ChatUiState::ID.to_string(), on_bytes);
                mark_changed::<Snapshot>(world);
                mark_changed::<Attachments>(world);
                mark_changed::<Media>(world);
                mark_changed::<Picker>(world);
            }
            CommandBarUiState::ID => {
                let mut runtime = self.runtime.borrow_mut();
                let world = runtime.app.world_mut();
                world
                    .non_send_mut::<PageListeners>()
                    .0
                    .insert(CommandBarUiState::ID.to_string(), on_bytes);
                mark_changed::<Launcher>(world);
            }
            TeamEvent::ID => {
                poll_team(self);
                let mut runtime = self.runtime.borrow_mut();
                let world = runtime.app.world_mut();
                world
                    .non_send_mut::<PageListeners>()
                    .0
                    .insert(TeamEvent::ID.to_string(), on_bytes);
                mark_changed::<Team>(world);
            }
            _ => return Err(EventListenerError::Unsupported),
        }
        Ok(())
    }
}

fn submit(host: &MobileHost, payload: ChatSubmit) -> Result<(), EventListenerError> {
    if host.session.sid().is_empty() {
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
    host.runtime
        .borrow_mut()
        .app
        .world_mut()
        .write_message(Submitted);
    let runtime = host.runtime.clone();
    agent_call(host, move |api, sid| async move {
        let request = PromptRequest {
            client_op_id: next_client_op_id(),
            text: payload.text,
            attachments,
        };
        if let Err(ApiError::Message(message)) = api.send_prompt(&sid, &request).await {
            report(&runtime, RemoteStatus::Errored(message));
        }
    })
}

fn report(runtime: &RuntimeHandle, status: RemoteStatus) {
    runtime
        .borrow_mut()
        .app
        .world_mut()
        .write_message(Reported(RemoteEvent::Status { status }));
}

fn cancel(host: &MobileHost) -> Result<(), EventListenerError> {
    agent_call(host, |api, sid| async move {
        if let Err(error) = api.cancel(&sid).await {
            tracing::warn!("cancelling failed: {error:?}");
        }
    })
}

fn approve(host: &MobileHost, payload: ChatApproval) -> Result<(), EventListenerError> {
    host.runtime
        .borrow_mut()
        .app
        .world_mut()
        .write_message(Reported(RemoteEvent::Approval { approval: None }));
    agent_call(host, move |api, sid| async move {
        let request = ApprovalRequest {
            call_id: payload.call_id,
            decision: payload.decision,
        };
        if let Err(error) = api.approve(&sid, &request).await {
            tracing::warn!("approving failed: {error:?}");
        }
    })
}

fn attach(host: &MobileHost, payload: ChatAttachPaths) -> Result<(), EventListenerError> {
    let offered = host.composer.offered.read();
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
    host.runtime
        .borrow_mut()
        .app
        .world_mut()
        .write_message(Attach(resolved));
    Ok(())
}

fn prompt(host: &MobileHost, request: CommandBarPromptRequest) -> Result<(), EventListenerError> {
    crate::session::start_chat(
        host.runtime.clone(),
        host.api.clone(),
        host.sessions,
        request.text,
        request.target_url,
    );
    Ok(())
}

fn switch_tab(host: &MobileHost, request: SwitchTabRequest) -> Result<(), EventListenerError> {
    let Some(session) = host.sessions.read().get(request.index).cloned() else {
        return Err(EventListenerError::Unsupported);
    };
    host.runtime
        .borrow_mut()
        .app
        .world_mut()
        .write_message(crate::session::OpenSession(session));
    Ok(())
}

fn agent_call<F, Fut>(host: &MobileHost, call: F) -> Result<(), EventListenerError>
where
    F: FnOnce(Api, String) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    let sid = host.session.sid();
    if sid.is_empty() {
        return Err(EventListenerError::Unsupported);
    }
    let api = host.api.clone();
    spawn(call(api, sid));
    Ok(())
}

fn poll_models(host: &MobileHost) {
    let (api, session) = (host.api.clone(), host.session);
    let runtime = host.runtime.clone();
    let epoch = host.epoch;
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
                        runtime.borrow_mut().app.insert_resource(Models(state));
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

fn poll_media(host: &MobileHost) {
    let (api, session) = (host.api.clone(), host.session);
    let runtime = host.runtime.clone();
    let composer = host.composer;
    let mut offered = composer.offered;
    let epoch = host.epoch;
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
                    runtime.borrow_mut().app.insert_resource(Browsed {
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

fn poll_team(host: &MobileHost) {
    let (api, epoch) = (host.api.clone(), host.epoch);
    let runtime = host.runtime.clone();
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
                    runtime.borrow_mut().app.insert_resource(Members(members));
                }
                Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                Err(ApiError::Message(_)) => {}
            }
            sleep_ms(TEAM_POLL_INTERVAL_MS).await;
        }
    });
}

fn mark_changed<R: bevy_ecs::resource::Resource<Mutability = bevy_ecs::component::Mutable>>(
    world: &mut bevy_ecs::world::World,
) {
    if let Some(mut resource) = world.get_resource_mut::<R>() {
        resource.set_changed();
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
