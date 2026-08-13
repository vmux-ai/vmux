#![allow(non_snake_case)]

mod credentials;
mod native_transition;
mod page_host;
mod qr_scanner;
mod quic_api;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_chat::format::composer::{SelectorMode, filter_models, selector_mode};
use vmux_chat::page::agent::StatusDot;
use vmux_chat::page::approval::ApprovalPanel;
use vmux_chat::page::composer::ComposerStatus;
use vmux_chat::page::composer::options::{EffortMenu, ModelMenu, ModelPill};
use vmux_chat::transcript::{AssistantTurn, ChatItemRow, MD_CSS, WorkingIndicator};
use vmux_service::chat::group_turns_tail;
use vmux_start::results::CommandBarResultItem;
use vmux_start::row::ResultRow;
use vmux_ui::components::prompt_box::{PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_composer::{
    PromptComposer, PromptComposerAction, PromptComposerAttachment,
};
use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{MenuDirection, move_selection};
use vmux_ui::i18n::translate;
use vmux_wire::PageIcon;
use vmux_wire::chat::{ChatItem, latest_tool_location};
use vmux_wire::prompt_media::ChatAttachment;
use vmux_wire::protocol::{AgentAction, SharedAgentCommand, SharedMessage, SharedResponse};
use vmux_wire::room::{
    AgentAttachment, ApprovalRequest, AssistantBlock, ClientOpId, Message, ModelOptionEntry,
    NewChatRequest, PromptRequest, RemoteAgent, RemoteApproval, RemoteEvent, RemoteMediaEntry,
    RemoteModelState, RemoteSession, RemoteStatus, RoomEvent, RoomId, inline_media_query,
    replace_inline_media_query,
};

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.out.css");
static OPENED_URLS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static NEXT_CLIENT_OP_ID: AtomicU64 = AtomicU64::new(0);

fn next_client_op_id() -> ClientOpId {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_CLIENT_OP_ID.fetch_add(1, Ordering::Relaxed);
    ClientOpId::new(format!("mobile:{timestamp}:{sequence}"))
}

/// Set when the app comes back to the foreground.
///
/// iOS tears down the UDP socket while suspended without closing the QUIC connection, so a
/// connection that still looks alive after a resume usually is not. Reconnecting on the next call
/// is cheaper than discovering it through a stalled request.
static RESUMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `background` from `crates/vmux_ui/assets/theme.css`, as the webview wants it.
///
/// oklch(0.88 0 0) and oklch(0.145 0 0), converted to sRGB — the same two values
/// `packaging/ios/Assets.xcassets/LaunchBackground.colorset` carries for the launch screen, so
/// the launch screen and the first webview frame are the same colour.
const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

/// The webview's own background, which is what shows before the document loads at all.
///
/// Without this the first frame after the launch screen is plain white — a stylesheet cannot
/// reach it, because there is no document yet. It has to be one colour decided up front, before
/// there is any UIKit environment to consult, so this reads the appearance from
/// `currentTraitCollection`, which UIKit documents as meaningful only inside a trait-environment
/// callback and this is not one.
///
/// It does answer correctly here, checked both ways: a dark cold start produced a dark first
/// frame, where an `Unspecified` reading would have fallen through to a light one, and forcing
/// the reading to light produced a light frame in dark mode. There is no second line of defence
/// behind it — an inline media query in the document was tried and does not repaint over this —
/// so if the reading is ever wrong, the wrong colour shows until the app paints.
#[cfg(target_os = "ios")]
fn webview_background() -> (u8, u8, u8, u8) {
    use objc2_ui_kit::{UITraitCollection, UIUserInterfaceStyle};

    let style = unsafe { UITraitCollection::currentTraitCollection().userInterfaceStyle() };
    if style == UIUserInterfaceStyle::Dark {
        DARK_BACKGROUND
    } else {
        LIGHT_BACKGROUND
    }
}

#[cfg(not(target_os = "ios"))]
fn webview_background() -> (u8, u8, u8, u8) {
    LIGHT_BACKGROUND
}

fn main() {
    let config = dioxus::mobile::Config::new()
        .with_background_color(webview_background())
        .with_custom_event_handler(|event, _| {
            use dioxus::mobile::tao::event::Event;
            match event {
                Event::Opened { urls } => {
                    let mut opened = OPENED_URLS
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    opened.extend(
                        urls.iter()
                            .filter(|url| url.scheme() == "vmux" && url.host_str() == Some("pair"))
                            .map(ToString::to_string),
                    );
                }
                Event::Resumed => RESUMED.store(true, std::sync::atomic::Ordering::Release),
                _ => {}
            }
        });
    dioxus::LaunchBuilder::mobile().with_cfg(config).launch(App);
}

/// Whether the app has resumed since this was last asked.
fn take_resumed() -> bool {
    RESUMED.swap(false, std::sync::atomic::Ordering::AcqRel)
}

#[derive(Clone, Copy, PartialEq)]
enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct MobileRoomProjection {
    room_id: Option<RoomId>,
    through_seq: u64,
    events: Vec<RoomEvent>,
}

impl MobileRoomProjection {
    /// Fold the replayed log into rendered chat items, with any streaming delta as the tail of
    /// the live turn.
    ///
    /// The phone has no imported history and no per-turn durations, so it always asks for the
    /// whole transcript and lets every turn resolve its duration to `None`.
    fn chat_items(&self, live_delta: &str, running: bool) -> Vec<ChatItem> {
        let mut messages = Vec::with_capacity(self.events.len() + 1);
        for event in &self.events {
            messages.push(event.message.clone());
        }
        if !live_delta.is_empty() {
            messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(live_delta.to_string())],
            });
        }
        group_turns_tail(&[], &messages, &[], running, usize::MAX).items
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Credentials {
    base_url: String,
    token: String,
    /// SHA-256 of the desktop's QUIC certificate, pinned when dialling it.
    ///
    /// Defaulted rather than required so a pairing written by an older build still deserialises.
    /// It is refused on use instead, which tells the phone to scan again rather than silently
    /// forgetting the Mac.
    #[serde(default)]
    fingerprint: String,
}

#[derive(Clone)]
struct Api {
    quic: crate::quic_api::QuicApi,
}

enum ApiError {
    Unauthorized,
    /// No such session on the Mac. Asking again will not conjure one.
    NotFound,
    Message(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str(&translate("mobile-error-pairing-expired")),
            Self::NotFound => f.write_str(&translate("mobile-error-not-offered")),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl Api {
    /// Fails when the pairing carries no certificate fingerprint.
    ///
    /// There is nothing to fall back to: the Mac is reached by pinning that certificate, so a
    /// pairing without one names a desktop this build cannot dial. Re-pairing is the only fix.
    fn new(credentials: Credentials) -> Result<Self, ApiError> {
        let Some(endpoint) = quic_endpoint(&credentials) else {
            return Err(ApiError::Message(translate(
                "mobile-error-pairing-outdated",
            )));
        };
        Ok(Self {
            quic: crate::quic_api::QuicApi::new(endpoint),
        })
    }

    /// Drop any live QUIC connection so the next call redials.
    async fn reset_transport(&self) {
        self.quic.reset().await;
    }

    async fn agents(&self) -> Result<Vec<RemoteAgent>, ApiError> {
        broker_json(&self.quic, SharedAgentCommand::ListAgents).await
    }

    async fn sessions(&self) -> Result<Vec<RemoteSession>, ApiError> {
        match self.quic.request(SharedMessage::ListSessions).await {
            Ok(SharedResponse::Sessions(sessions)) => Ok(sessions),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    /// The models this session can run, and its current effort level.
    async fn models(&self, sid: &str) -> Result<RemoteModelState, ApiError> {
        broker_json(
            &self.quic,
            SharedAgentCommand::ListModels {
                sid: sid.to_string(),
            },
        )
        .await
    }

    /// Switch the session to another of its models.
    async fn select_model(&self, sid: &str, model_id: &str) -> Result<(), ApiError> {
        self.command(SharedAgentCommand::SelectModel {
            sid: sid.to_string(),
            model_id: model_id.to_string(),
        })
        .await
    }

    /// Set how hard the session's agent is asked to think. An empty level restores its default.
    async fn set_effort(&self, sid: &str, level: &str) -> Result<(), ApiError> {
        self.command(SharedAgentCommand::SetEffort {
            sid: sid.to_string(),
            level: level.to_string(),
        })
        .await
    }

    async fn command(&self, command: SharedAgentCommand) -> Result<(), ApiError> {
        self.applied(
            self.quic
                .request(SharedMessage::AgentCommand(command))
                .await,
        )
    }

    async fn team(&self) -> Result<Vec<vmux_wire::team::TeamMemberRow>, ApiError> {
        broker_json(&self.quic, SharedAgentCommand::ListTeam).await
    }

    /// Subscribe to a session's events.
    async fn subscribe(&self, sid: &str) -> Result<crate::quic_api::Subscription, ApiError> {
        self.quic.subscribe(sid).await.map_err(Into::into)
    }

    /// Submit a prompt to a running session.
    async fn send_prompt(&self, sid: &str, request: &PromptRequest) -> Result<(), ApiError> {
        let message = SharedMessage::agent(
            sid,
            AgentAction::Input {
                text: request.text.clone(),
                context: None,
                attachments: request.attachments.clone(),
            },
        );
        self.applied(self.quic.request(message).await)
    }

    /// Open a new chat on the desktop.
    async fn create_chat(&self, request: &NewChatRequest) -> Result<(), ApiError> {
        let command = SharedAgentCommand::NewAgentChat {
            client_op_id: request.client_op_id.clone(),
            prompt: request.text.clone(),
            agent_url: request.agent_url.clone(),
        };
        self.applied(
            self.quic
                .request(SharedMessage::AgentCommand(command))
                .await,
        )
    }

    /// Interrupt the session's in-flight turn.
    async fn cancel(&self, sid: &str) -> Result<(), ApiError> {
        let message = SharedMessage::agent(sid, AgentAction::Cancel);
        self.applied(self.quic.request(message).await)
    }

    /// Answer a pending tool approval.
    async fn approve(&self, sid: &str, request: &ApprovalRequest) -> Result<(), ApiError> {
        let message = SharedMessage::agent(
            sid,
            AgentAction::Approve {
                call_id: request.call_id.clone(),
                decision: request.decision,
            },
        );
        self.applied(self.quic.request(message).await)
    }

    /// A replay is success, not failure: the desktop recognised the op and declined to run it
    /// twice, which is exactly what the idempotency key is for.
    fn applied(
        &self,
        outcome: Result<SharedResponse, crate::quic_api::QuicError>,
    ) -> Result<(), ApiError> {
        match outcome {
            Ok(SharedResponse::Ok | SharedResponse::AlreadyApplied) => Ok(()),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    async fn media(&self, sid: &str, query: &str) -> Result<Vec<RemoteMediaEntry>, ApiError> {
        let request = SharedMessage::agent(
            sid,
            AgentAction::ListMedia {
                query: query.to_string(),
            },
        );
        match self.quic.request(request).await {
            Ok(SharedResponse::Media(entries)) => Ok(entries),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }
}

/// Project a shared event onto the shape the pages already render.
///
/// The desktop used to do this before serialising to SSE. Doing it here instead keeps the wire
/// typed — `RemoteEvent` is now a rendering concern of this app, not a thing any peer sends.
fn remote_event_from_shared(event: vmux_wire::protocol::SharedEvent) -> Option<RemoteEvent> {
    use vmux_wire::protocol::SharedEvent as Shared;
    match event {
        Shared::AgentDelta { sid, text } => Some(RemoteEvent::Delta {
            room_id: vmux_wire::room::RoomId::for_session(&sid),
            text,
        }),
        Shared::AgentRunStatusChanged { status, .. } => Some(RemoteEvent::Status {
            status: RemoteStatus::from(&status),
        }),
        Shared::AgentAwaitingApproval {
            call_id,
            name,
            args_json,
            ..
        } => Some(RemoteEvent::Approval {
            approval: Some(RemoteApproval {
                call_id,
                name,
                args_json,
            }),
        }),
        Shared::AgentApprovalResolved { .. } => Some(RemoteEvent::Approval { approval: None }),
        Shared::AgentMessagesSnapshot { sid, messages_json } => {
            let messages: Vec<vmux_wire::room::Message> =
                serde_json::from_str(&messages_json).ok()?;
            let room_id = vmux_wire::room::RoomId::for_session(&sid);
            let events = vmux_wire::room::RoomEvent::from_messages(&sid, 0, &messages);
            Some(RemoteEvent::Snapshot {
                room_id,
                through_seq: events.len() as u64,
                events,
            })
        }
        Shared::Session { session } => Some(RemoteEvent::Session { session }),
        // The daemon resolves these into Session before they reach a client; reaching here means
        // an older desktop that predates that, and there is nothing renderable to derive.
        Shared::AcpAgentInfo { .. }
        | Shared::AcpWorkspaceChanged { .. }
        | Shared::AcpModelInfo { .. } => None,
    }
}

/// Build the QUIC endpoint from a pairing, when it carried a fingerprint.
///
/// The device id is derived from the pairing address rather than stored separately: the relay
/// routes by port, so this only labels the hello the desktop reads.
fn quic_endpoint(credentials: &Credentials) -> Option<crate::quic_api::Endpoint> {
    if credentials.fingerprint.is_empty() {
        return None;
    }
    let parsed = Url::parse(&credentials.base_url).ok()?;
    let host = parsed.host_str()?;
    let port = parsed.port().unwrap_or(443);
    // The relay routes by port, not by name — a phone's packets reach exactly one desktop because
    // of which port they arrived on. This id only labels the hello the desktop reads.
    Some(crate::quic_api::Endpoint {
        address: format!("{host}:{port}"),
        token: credentials.token.clone(),
        fingerprint: credentials.fingerprint.clone(),
        device_id: vmux_remote::DeviceId::new(format!("{host}:{port}")),
    })
}

/// GUI-held state comes back as JSON the desktop forwarded verbatim, so it is parsed here rather
/// than re-typed on the wire — the shape belongs to the page that renders it.
async fn broker_json<T: serde::de::DeserializeOwned>(
    quic: &crate::quic_api::QuicApi,
    command: SharedAgentCommand,
) -> Result<T, ApiError> {
    match quic.request(SharedMessage::AgentCommand(command)).await {
        Ok(SharedResponse::BrokerJson(json)) => {
            serde_json::from_str(&json).map_err(|error| ApiError::Message(error.to_string()))
        }
        Ok(_) => Err(ApiError::Message(translate(
            "mobile-error-unexpected-answer",
        ))),
        Err(error) => Err(error.into()),
    }
}

impl From<crate::quic_api::QuicError> for ApiError {
    fn from(error: crate::quic_api::QuicError) -> Self {
        use crate::quic_api::QuicError;
        use vmux_wire::protocol::SharedFailure;
        match error {
            QuicError::Unauthorized => Self::Unauthorized,
            QuicError::Refused(SharedFailure::NotFound) => Self::NotFound,
            other => Self::Message(other.to_string()),
        }
    }
}

/// The model and effort pickers under the composer.
///
/// Fetched per session rather than carried on [`RemoteSession`], because the list arrives from the
/// agent after the session exists and a stale copy would offer models it has since dropped.
#[component]
fn ComposerOptions(
    state: Signal<RemoteModelState>,
    sid: String,
    api: Signal<Option<Api>>,
    mut draft: Signal<String>,
) -> Element {
    let current = state();
    if current.models.is_empty() && current.effort_levels.is_empty() {
        return rsx! {
            div { class: "truncate text-[10px] text-muted-foreground/55", "Enter to send" }
        };
    }
    let current_name = current
        .models
        .iter()
        .find(|model| model.id == current.selected_id)
        .map(|model| model.name.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
            ModelPill {
                name: current_name,
                // The software keyboard is up whenever the composer has focus, so `/model` filters
                // here exactly as it does on the desktop.
                on_open: move |_| draft.set("/model ".to_string()),
            }
            EffortMenu {
                levels: current.effort_levels.clone(),
                selected: current.effort.clone(),
                on_select: {
                    let sid = sid.clone();
                    move |level: String| {
                        let (sid, level) = (sid.clone(), level);
                        let Some(client) = api.peek().clone() else { return };
                        state.write().effort = level.clone();
                        spawn(async move {
                            let _ = client.set_effort(&sid, &level).await;
                        });
                    }
                },
            }
        }
    }
}

/// The session's models and effort, re-read whenever the session changes.
///
/// Fetched per session rather than carried on [`RemoteSession`], because the list arrives from the
/// agent after the session exists and a stale copy would offer models it has since dropped.
fn use_remote_model_state(sid: String, api: Signal<Option<Api>>) -> Signal<RemoteModelState> {
    let mut state = use_signal(RemoteModelState::default);
    use_effect(use_reactive!(|sid| {
        // Read reactively: pairing can finish after a session is selected, and a peek here would
        // leave the pickers empty until the next session change.
        let Some(client) = api() else {
            return;
        };
        if sid.is_empty() {
            state.set(RemoteModelState::default());
            return;
        }
        spawn(async move {
            if let Ok(fetched) = client.models(&sid).await {
                state.set(fetched);
            }
        });
    }));
    state
}

fn submit_remote_prompt(
    api: Signal<Option<Api>>,
    sid: String,
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<RemoteMediaEntry>>,
    mut status: Signal<RemoteStatus>,
) {
    let Some(client) = api() else { return };
    let text = draft.peek().trim().to_string();
    let selected = attachments.peek().clone();
    if sid.is_empty() || (text.is_empty() && selected.is_empty()) {
        return;
    }
    let attachments_to_submit = selected
        .into_iter()
        .filter(|attachment| !attachment.is_dir)
        .map(|attachment| AgentAttachment {
            path: attachment.path,
            name: attachment.name,
            mime_type: attachment.mime_type,
            size: attachment.size,
        })
        .collect();
    draft.set(String::new());
    attachments.set(Vec::new());
    status.set(RemoteStatus::Streaming);
    spawn(async move {
        if let Err(ApiError::Message(message)) = client
            .send_prompt(
                &sid,
                &PromptRequest {
                    client_op_id: next_client_op_id(),
                    text,
                    attachments: attachments_to_submit,
                },
            )
            .await
        {
            status.set(RemoteStatus::Errored(message));
        }
    });
}

fn insert_media_token(mut draft: Signal<String>) {
    let mut value = draft.peek().clone();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push('@');
    draft.set(value);
}

fn select_remote_media_entry(
    entry: &RemoteMediaEntry,
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<RemoteMediaEntry>>,
    mut selected: Signal<usize>,
) {
    let value = draft.peek().clone();
    let Some(query) = inline_media_query(&value) else {
        return;
    };
    let replacement = if entry.is_dir {
        format!("@{}/", entry.reference())
    } else {
        let mut next = attachments.peek().clone();
        if !next.iter().any(|attached| attached.path == entry.path) {
            next.push(entry.clone());
            attachments.set(next);
        }
        String::new()
    };
    draft.set(replace_inline_media_query(&value, query, &replacement));
    selected.set(0);
}

#[allow(clippy::too_many_arguments)]
fn start_new_chat(
    api: Signal<Option<Api>>,
    mut sessions: Signal<Vec<RemoteSession>>,
    current: Signal<Option<RemoteSession>>,
    room: Signal<MobileRoomProjection>,
    live_delta: Signal<String>,
    status: Signal<RemoteStatus>,
    approval: Signal<Option<RemoteApproval>>,
    connected: Signal<bool>,
    stream_generation: Signal<u64>,
    mut draft: Signal<String>,
    mut error: Signal<String>,
    mut creating: Signal<bool>,
    agent_url: Option<String>,
) {
    let text = draft.peek().trim().to_string();
    let Some(client) = api() else { return };
    if text.is_empty() || creating() {
        return;
    }
    let known = sessions
        .read()
        .iter()
        .map(|session| session.sid.clone())
        .collect::<std::collections::HashSet<_>>();
    creating.set(true);
    error.set(String::new());
    spawn(async move {
        match client
            .create_chat(&NewChatRequest {
                client_op_id: next_client_op_id(),
                text,
                agent_url,
            })
            .await
        {
            Ok(()) => {
                for _ in 0..40 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    if let Ok(next) = client.sessions().await {
                        let created = next
                            .iter()
                            .find(|session| !known.contains(&session.sid))
                            .cloned();
                        sessions.set(next);
                        if let Some(created) = created {
                            draft.set(String::new());
                            creating.set(false);
                            open_session(
                                client,
                                created,
                                current,
                                room,
                                live_delta,
                                status,
                                approval,
                                connected,
                                stream_generation,
                            );
                            return;
                        }
                    }
                }
                error.set(translate("mobile-error-stack-missing"));
            }
            Err(ApiError::Unauthorized) => {
                error.set(translate("mobile-error-pairing-lost"));
            }
            Err(other) => error.set(other.to_string()),
        }
        creating.set(false);
    });
}

fn leave_session(
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
    mut connected: Signal<bool>,
    mut generation: Signal<u64>,
) {
    let dismissing = native_transition::NativeSheet::close();
    generation.set(generation().wrapping_add(1));
    current.set(None);
    room.set(MobileRoomProjection::default());
    live_delta.set(String::new());
    status.set(RemoteStatus::Idle);
    approval.set(None);
    connected.set(false);
    dismissing.finish();
}

#[component]
fn App() -> Element {
    rsx! {
        AppHead {}
        AppBody {}
    }
}

/// Everything below the head.
///
/// Split out so [`AppHead`] mounts exactly once. Dioxus records an inserted stylesheet href in a
/// root context and skips that href ever after, so a head rendered inside a branch loses its
/// stylesheet the first time the branch changes, and nothing puts it back.
#[component]
fn AppBody() -> Element {
    native_transition::install(&dioxus::mobile::window());
    qr_scanner::install(&dioxus::mobile::window());
    let mut auth = use_signal(|| AuthState::Loading);
    let mut pair_url = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut api = use_signal(|| None::<Api>);
    let mut sessions = use_signal(Vec::<RemoteSession>::new);
    let mut agents = use_signal(Vec::<RemoteAgent>::new);
    let selected_agent = use_signal(|| Option::<String>::None);
    let current = use_signal(|| None::<RemoteSession>);
    let room = use_signal(MobileRoomProjection::default);
    let live_delta = use_signal(String::new);
    let status = use_signal(|| RemoteStatus::Idle);
    let mut approval = use_signal(|| None::<RemoteApproval>);
    let mut draft = use_signal(String::new);
    let mut attachments = use_signal(Vec::<RemoteMediaEntry>::new);
    let mut media_entries = use_signal(Vec::<RemoteMediaEntry>::new);
    let mut media_loading = use_signal(|| false);
    let mut media_generation = use_signal(|| 0_u64);
    let mut media_selected = use_signal(|| 0_usize);
    let mut attachment_sid = use_signal(String::new);
    let connected = use_signal(|| false);
    // Whether the Mac is answering, as opposed to whether this device is paired.
    // Conflating the two let the header claim Connected while every request timed out.
    let mut reachable = use_signal(|| false);
    let mut stream_generation = use_signal(|| 0_u64);
    let mut pending_pair_url = use_signal(|| None::<String>);
    let mut deep_link_received = use_signal(|| false);
    let mut pairing = use_signal(|| false);
    let mut team_open = use_signal(|| false);
    let mut new_chat_draft = use_signal(String::new);
    let new_chat_error = use_signal(String::new);
    let creating_chat = use_signal(|| false);

    // Pinned to the bottom as the transcript grows. Through the mounted handle rather than by
    // reaching into the DOM, so the renderer decides how a scroll actually happens.
    let mut transcript_view = use_signal(|| None::<Event<MountedData>>);
    use_effect(move || {
        let _ = room.read().events.len();
        let _ = live_delta.read().len();
        let Some(view) = transcript_view() else {
            return;
        };
        spawn(async move {
            let Ok(size) = view.get_scroll_size().await else {
                return;
            };
            let bottom = PixelsVector2D::new(0.0, size.height);
            let _ = view.scroll(bottom, ScrollBehavior::Instant).await;
        });
    });

    use_effect(move || {
        let sid = current().map(|session| session.sid).unwrap_or_default();
        if *attachment_sid.peek() == sid {
            return;
        }
        attachment_sid.set(sid);
        attachments.set(Vec::new());
        media_entries.set(Vec::new());
        media_loading.set(false);
    });

    use_effect(move || {
        let value = draft();
        let query = inline_media_query(&value).map(|query| query.query.to_string());
        let sid = current().map(|session| session.sid).unwrap_or_default();
        let client = api();
        let generation = media_generation.peek().wrapping_add(1);
        media_generation.set(generation);
        media_selected.set(0);
        let (Some(query), Some(client)) = (query, client) else {
            media_entries.set(Vec::new());
            media_loading.set(false);
            return;
        };
        if sid.is_empty() {
            media_entries.set(Vec::new());
            media_loading.set(false);
            return;
        }
        media_loading.set(true);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            if *media_generation.peek() != generation {
                return;
            }
            let result = client.media(&sid, &query).await;
            if *media_generation.peek() != generation {
                return;
            }
            match result {
                Ok(entries) => media_entries.set(entries),
                Err(_) => media_entries.set(Vec::new()),
            }
            media_loading.set(false);
        });
    });

    // Shared pages reach the desktop through the installed host, so it has to exist before one
    // mounts. Keying off the signal covers every path that pairs, not just the resume-on-launch one.
    use_effect(move || {
        if let Some(client) = api() {
            page_host::install(client);
        }
    });

    use_future(move || async move {
        if let Some(opened) = take_opened_url() {
            deep_link_received.set(true);
            pair_url.set(opened.clone());
            pending_pair_url.set(Some(opened));
            auth.set(AuthState::Unpaired);
            return;
        }
        let Some(credentials) = credentials::StoredCredentials::load() else {
            if deep_link_received() {
                return;
            }
            auth.set(AuthState::Unpaired);
            return;
        };
        if deep_link_received() {
            return;
        }
        pair_url.set(pairing_url(&credentials));
        let client = match Api::new(credentials) {
            Ok(client) => client,
            Err(reason) => {
                credentials::StoredCredentials::clear();
                error.set(reason.to_string());
                auth.set(AuthState::Unpaired);
                return;
            }
        };
        // Stored credentials already answer "is this paired?", so paint the start page now and let
        // reachability resolve behind it. Waiting on the first round trip meant a spinner for as
        // long as the dial takes to give up, which is the whole dial timeout when the Mac is off.
        api.set(Some(client.clone()));
        auth.set(AuthState::Paired);
        match client.sessions().await {
            Ok(next) => {
                sessions.set(next);
                agents.set(client.agents().await.unwrap_or_default());
                reachable.set(true);
            }
            Err(ApiError::Unauthorized) => {
                credentials::StoredCredentials::clear();
                error.set(translate("mobile-error-pairing-expired"));
                auth.set(AuthState::Unpaired);
            }
            Err(other) => error.set(other.to_string()),
        }
    });

    use_future(move || async move {
        loop {
            if let Some(opened) = take_opened_url() {
                deep_link_received.set(true);
                pair_url.set(opened.clone());
                pending_pair_url.set(Some(opened));
                error.set(String::new());
                auth.set(AuthState::Unpaired);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    use_future(move || async move {
        loop {
            if let Some(result) = qr_scanner::take_result() {
                match result {
                    Ok(scanned) => {
                        pair_url.set(scanned.clone());
                        error.set(String::new());
                        pending_pair_url.set(Some(scanned));
                    }
                    Err(message) => error.set(message),
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let pending = pending_pair_url.write().take();
            let Some(input) = pending else {
                continue;
            };
            let credentials = match parse_pairing_url(&input) {
                Ok(credentials) => credentials,
                Err(message) => {
                    pairing.set(false);
                    error.set(message);
                    auth.set(AuthState::Unpaired);
                    continue;
                }
            };
            pairing.set(true);
            error.set(String::new());
            let client = match Api::new(credentials.clone()) {
                Ok(client) => client,
                Err(reason) => {
                    pairing.set(false);
                    error.set(reason.to_string());
                    auth.set(AuthState::Unpaired);
                    continue;
                }
            };
            match client.sessions().await {
                Ok(next) => {
                    credentials::StoredCredentials::save(&credentials);
                    pair_url.set(pairing_url(&credentials));
                    api.set(Some(client.clone()));
                    sessions.set(next);
                    auth.set(AuthState::Paired);
                }
                Err(ApiError::Unauthorized) => {
                    error.set(translate("mobile-error-token-rejected"));
                    auth.set(AuthState::Unpaired);
                }
                Err(other) => {
                    error.set(other.to_string());
                    auth.set(AuthState::Unpaired);
                }
            }
            pairing.set(false);
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if auth() != AuthState::Paired {
                continue;
            }
            let Some(client) = api() else {
                continue;
            };
            match client.sessions().await {
                Ok(next) => {
                    sessions.set(next);
                    reachable.set(true);
                    error.set(String::new());
                }
                Err(ApiError::Unauthorized) => {
                    reachable.set(false);
                    credentials::StoredCredentials::clear();
                    api.set(None);
                    error.set(translate("mobile-error-pairing-expired"));
                    auth.set(AuthState::Unpaired);
                }
                Err(other) => {
                    reachable.set(false);
                    error.set(other.to_string());
                }
            }
        }
    });

    if auth() == AuthState::Loading {
        return rsx! {
            div { class: "flex h-dvh items-center justify-center bg-background text-foreground",
                div { class: "h-8 w-8 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            }
        };
    }

    if team_open() {
        // vmux_team::page::Page is the desktop's team page, unmodified — it reads TEAM_EVENT off
        // the installed host exactly as it does inside CEF. Only the way back is ours.
        return rsx! {
            div { class: "flex h-dvh flex-col bg-background text-foreground",
                div { class: "flex items-center gap-1 border-b border-border px-2 pt-[env(safe-area-inset-top)]",
                    button {
                        class: "rounded-lg px-3 py-2 text-sm text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| team_open.set(false),
                        {translate("mobile-chat-back")}
                    }
                }
                div { class: "min-h-0 flex-1", vmux_team::page::Page {} }
            }
        };
    }

    if current().is_none() {
        return rsx! {
            MobileStartPage {
                paired: auth() == AuthState::Paired,
                reachable: reachable(),
                sessions: sessions(),
                agents: agents(),
                draft: new_chat_draft(),
                error: new_chat_error(),
                creating: creating_chat(),
                pair_value: pair_url(),
                pair_error: error(),
                pairing: pairing(),
                on_draft: move |value| new_chat_draft.set(value),
                on_submit: move |_| start_new_chat(
                    api,
                    sessions,
                    current,
                    room,
                    live_delta,
                    status,
                    approval,
                    connected,
                    stream_generation,
                    new_chat_draft,
                    new_chat_error,
                    creating_chat,
                    selected_agent(),
                ),
                on_start_agent: move |url: String| start_new_chat(
                    api,
                    sessions,
                    current,
                    room,
                    live_delta,
                    status,
                    approval,
                    connected,
                    stream_generation,
                    new_chat_draft,
                    new_chat_error,
                    creating_chat,
                    Some(url),
                ),
                on_open: move |session| {
                    let Some(client) = api() else { return };
                    open_session(
                        client,
                        session,
                        current,
                        room,
                        live_delta,
                        status,
                        approval,
                        connected,
                        stream_generation,
                    );
                },
                on_pair_value: move |value| pair_url.set(value),
                on_pair: move |_| {
                    pending_pair_url.set(Some(pair_url()));
                },
                on_scan: move |_| {
                    error.set(String::new());
                    if let Err(message) = qr_scanner::open() {
                        error.set(message);
                    }
                },
                on_disconnect: move |_| {
                    credentials::StoredCredentials::clear();
                    stream_generation.set(stream_generation().wrapping_add(1));
                    api.set(None);
                    sessions.set(Vec::new());
                    auth.set(AuthState::Unpaired);
                },
                on_open_team: move |_| team_open.set(true),
            }
        };
    }

    let current_value = current();
    // The session says which agent it is by name; the icon lives on the agent list the phone
    // already fetches, so no extra round trip and nothing new on the wire.
    let matched_agent = current_value.as_ref().and_then(|session| {
        agents()
            .into_iter()
            .find(|agent| agent.name == session.name)
    });
    let agent_icon = matched_agent
        .as_ref()
        .map(|agent| agent.icon.clone())
        .unwrap_or_default();
    // Derived rather than sent: agent_accent is a pure function of the agent id and already lives
    // in the shared crate, so the phone reaches the same colours the desktop does without the
    // wire carrying a theme.
    let agent_segment = matched_agent
        .as_ref()
        .map(|agent| agent.id.as_str())
        .unwrap_or_default();
    let accent = vmux_ui::agent_accent::agent_accent(agent_segment);
    // What the desktop paints --agent-accent with: the agent's avatar colour, which is a pure
    // function of its URL segment. Deriving it rather than sending it keeps the two in step.
    let accent_css = vmux_wire::avatar::agent_color(agent_segment);
    let selected_sid = current_value
        .as_ref()
        .map(|session| session.sid.clone())
        .unwrap_or_default();
    let is_streaming = matches!(status(), RemoteStatus::Streaming);
    // The words ComposerStatus matches on. Same component as the desktop, so the mapping has to
    // land on the same strings rather than on RemoteStatus's own names.
    let status_word = match status() {
        RemoteStatus::Streaming => "streaming",
        RemoteStatus::Errored(_) => "errored",
        RemoteStatus::Interrupted => "interrupted",
        RemoteStatus::Idle => "idle",
    };
    let draft_value = draft();
    let can_send = current_value.is_some()
        && (!draft_value.trim().is_empty() || !attachments.read().is_empty());
    let prompt_action = if is_streaming {
        PromptComposerAction::Stop
    } else {
        PromptComposerAction::Send
    };
    let prompt_attachments = attachments
        .read()
        .iter()
        .enumerate()
        .map(|(index, attachment)| PromptComposerAttachment {
            key: format!("remote-attachment-{}", attachment.path),
            name: attachment.name.clone(),
            label: file_extension_label(&attachment.name),
            preview_data_url: attachment.preview_data_url.clone(),
            remove_index: Some(index),
        })
        .collect::<Vec<_>>();
    let prompt_media_options = media_entries
        .read()
        .iter()
        .map(|entry| PromptMediaOption {
            key: format!("remote-media-{}", entry.path),
            name: entry.name.clone(),
            display_path: entry.display_path(),
            preview_data_url: entry.preview_data_url.clone(),
            label: file_extension_label(&entry.name),
            is_dir: entry.is_dir,
        })
        .collect::<Vec<_>>();
    let media_menu_open = inline_media_query(&draft_value).is_some();
    let mut model_state = use_remote_model_state(selected_sid.clone(), api);
    let mut model_selected = use_signal(|| 0usize);
    let model_matches = match selector_mode(&draft_value) {
        SelectorMode::Models(query) => Some(filter_models(&model_state().models, query)),
        _ => None,
    };
    let submit_sid = selected_sid.clone();
    let cancel_sid = selected_sid.clone();
    let approval_sid = selected_sid.clone();
    let approval_value = approval();
    let live_delta_value = live_delta();
    let room_value = room();
    let transcript_items = room_value.chat_items(&live_delta_value, is_streaming);
    let latest_tool = latest_tool_location(&transcript_items);
    let activity = vmux_wire::chat::activity_counts(&transcript_items);
    let attachment_previews = use_signal(HashMap::<String, ChatAttachment>::new);

    rsx! {
        div {
            class: "flex h-dvh min-h-0 flex-col bg-background text-foreground",
            style: "--agent-accent:{accent_css};",
            style { dangerous_inner_html: MD_CSS }
            header { class: "flex shrink-0 items-center gap-3 border-b border-border bg-background/95 px-3 pb-2 pt-[calc(0.5rem+env(safe-area-inset-top))] backdrop-blur-xl sm:px-5",
                button {
                    class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-accent text-lg text-accent-foreground active:bg-accent/70",
                    onclick: move |_| leave_session(
                        current,
                        room,
                        live_delta,
                        status,
                        approval,
                        connected,
                        stream_generation,
                    ),
                    aria_label: translate("mobile-chat-back-to-stacks"),
                    svg {
                        class: "h-5 w-5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "m15 18-6-6 6-6" }
                    }
                }
                if !agent_icon.is_empty() {
                    Favicon {
                        favicon_url: agent_icon.clone(),
                        url: String::new(),
                        class: "h-8 w-8 shrink-0 rounded-lg".to_string(),
                        globe_class: "h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                    }
                }
                div { class: "min-w-0 flex-1",
                    if let Some(session) = current_value.as_ref() {
                        div { class: "truncate text-sm font-semibold", "{session.name}" }
                        div { class: "mt-1 flex items-center gap-1.5 truncate text-[11px] text-muted-foreground",
                            StatusDot {
                                status: status_word.to_string(),
                                size_class: "h-2 w-2 shrink-0".to_string(),
                            }
                            span { "{session.runtime}" }
                            if let Some(model) = session.model.as_ref() {
                                span { "· {model}" }
                            }
                            span { "· {cwd_name(&session.cwd)}" }
                        }
                    } else {
                        div { class: "text-sm font-semibold", "Vmux" }
                        div { class: "mt-1 text-[11px] text-muted-foreground", {translate("mobile-chat-no-session")} }
                    }
                }
                div { class: if connected() { "h-2 w-2 rounded-full bg-success" } else { "h-2 w-2 rounded-full bg-muted-foreground/50" } }
            }

            main {
                class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-5 sm:px-4 md:px-6",
                onmounted: move |event| transcript_view.set(Some(event)),
                if transcript_items.is_empty() && !is_streaming {
                    div { class: "flex h-full items-center justify-center px-8 text-center text-sm leading-6 text-muted-foreground",
                        {translate("mobile-chat-no-messages")}
                    }
                }
                div { class: "mx-auto flex w-full max-w-none flex-col gap-5 md:max-w-3xl",
                    for (index, item) in transcript_items.iter().cloned().enumerate() {
                        ChatItemRow {
                            key: "{index}",
                            absolute_index: index,
                            item,
                            attachment_previews,
                            latest_tool_block: (latest_tool.map(|(i, _)| i) == Some(index))
                                .then(|| latest_tool.map(|(_, b)| b))
                                .flatten(),
                        }
                    }
                    if is_streaming {
                        div { class: "flex flex-col",
                            AssistantTurn { WorkingIndicator {} }
                        }
                    }
                    if let RemoteStatus::Errored(message) = status() {
                        div { class: "mb-4 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs text-destructive", "{message}" }
                    }
                }
            }

            if let Some(pending) = approval_value {
                div { class: "shrink-0",
                    ApprovalPanel {
                        tool: pending.name.clone(),
                        args_json: pending.args_json.clone(),
                        on_answer: move |decision| {
                            let Some(client) = api() else { return };
                            approval.set(None);
                            let call_id = pending.call_id.clone();
                            let sid = approval_sid.clone();
                            spawn(async move {
                                let _ = client
                                    .approve(&sid, &ApprovalRequest { call_id, decision })
                                    .await;
                            });
                        },
                    }
                }
            }

            div {
                class: "shrink-0 border-t border-border bg-background/95 px-2.5 pb-[calc(0.625rem+env(safe-area-inset-bottom))] pt-2.5 backdrop-blur-xl sm:px-4 md:px-6",
                div { class: "relative mx-auto w-full max-w-none md:max-w-3xl",
                    if let Some(models) = model_matches {
                        PromptPopup {
                            placement: PromptPopupPlacement::Upward,
                            ModelMenu {
                                models,
                                current_model_id: model_state().selected_id.clone(),
                                selected: model_selected(),
                                on_hover: move |index| model_selected.set(index),
                                on_select: move |model: ModelOptionEntry| {
                                    let sid = selected_sid.clone();
                                    let Some(client) = api.peek().clone() else { return };
                                    model_state.write().selected_id = model.id.clone();
                                    draft.set(String::new());
                                    model_selected.set(0);
                                    spawn(async move {
                                        let _ = client.select_model(&sid, &model.id).await;
                                    });
                                },
                            }
                        }
                    }
                    if media_menu_open {
                        PromptPopup {
                            placement: PromptPopupPlacement::Upward,
                            PromptMediaOptions {
                                items: prompt_media_options,
                                selected: media_selected(),
                                loading: media_loading(),
                                on_hover: move |index| media_selected.set(index),
                                on_select: move |index| {
                                    if let Some(entry) = media_entries.peek().get(index).cloned() {
                                        select_remote_media_entry(
                                            &entry,
                                            draft,
                                            attachments,
                                            media_selected,
                                        );
                                    }
                                },
                            }
                        }
                    }
                    PromptComposer {
                        value: draft_value.clone(),
                        attachments: prompt_attachments,
                        footer: rsx! {
                            div { class: "flex min-w-0 items-center justify-between gap-1",
                                ComposerOptions {
                                    state: model_state,
                                    sid: submit_sid.clone(),
                                    api,
                                    draft,
                                }
                                ComposerStatus {
                                    status: status_word.to_string(),
                                    active_subagents: activity.0,
                                    active_tasks: activity.1,
                                }
                            }
                        },
                        placeholder: if current_value.is_some() { translate("mobile-chat-placeholder") } else { translate("mobile-chat-no-session") },
                        accent_bg: accent.accent_bg.to_string(),
                        accent_color: accent_css.clone(),
                        accent_gradient: accent.grad.to_string(),
                        autofocus: true,
                        disabled: current_value.is_none(),
                        action: prompt_action,
                        action_title: if is_streaming { translate("mobile-chat-stop") } else { translate("mobile-chat-send") },
                        action_enabled: if is_streaming { true } else { can_send },
                        on_input: move |value| draft.set(value),
                        on_keydown: {
                            let sid = submit_sid.clone();
                            move |event: KeyboardEvent| {
                                let value = draft.peek().clone();
                                // The model picker is a draft-filtered popup like the media one, so
                                // it has to claim the same keys before Enter reaches the submit
                                // path below and sends "/model …" to the agent as a prompt.
                                if let SelectorMode::Models(query) = selector_mode(&value) {
                                    let matches = filter_models(&model_state.peek().models, query);
                                    if let Some(direction) = MenuDirection::of(&event.data()) {
                                        event.prevent_default();
                                        model_selected.set(move_selection(
                                            model_selected(),
                                            matches.len(),
                                            direction,
                                        ));
                                        return;
                                    }
                                    match event.key() {
                                        Key::Enter if !event.modifiers().shift() => {
                                            event.prevent_default();
                                            if let Some(model) =
                                                matches.get(model_selected()).cloned()
                                            {
                                                let sid = sid.clone();
                                                if let Some(client) = api.peek().clone() {
                                                    model_state.write().selected_id =
                                                        model.id.clone();
                                                    draft.set(String::new());
                                                    model_selected.set(0);
                                                    spawn(async move {
                                                        let _ = client
                                                            .select_model(&sid, &model.id)
                                                            .await;
                                                    });
                                                }
                                            }
                                            return;
                                        }
                                        Key::Escape => {
                                            event.prevent_default();
                                            draft.set(String::new());
                                            model_selected.set(0);
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                let media_open = inline_media_query(&value).is_some();
                                if media_open {
                                    match event.key() {
                                        Key::ArrowDown => {
                                            event.prevent_default();
                                            let len = media_entries.peek().len();
                                            if len > 0 {
                                                media_selected.set((media_selected() + 1) % len);
                                            }
                                            return;
                                        }
                                        Key::ArrowUp => {
                                            event.prevent_default();
                                            let len = media_entries.peek().len();
                                            if len > 0 {
                                                media_selected.set((media_selected() + len - 1) % len);
                                            }
                                            return;
                                        }
                                        Key::Enter if !event.modifiers().shift() => {
                                            event.prevent_default();
                                            if let Some(entry) = media_entries
                                                .peek()
                                                .get(media_selected())
                                                .cloned()
                                            {
                                                select_remote_media_entry(
                                                    &entry,
                                                    draft,
                                                    attachments,
                                                    media_selected,
                                                );
                                            }
                                            return;
                                        }
                                        Key::Escape => {
                                            event.prevent_default();
                                            if let Some(query) = inline_media_query(&value) {
                                                draft.set(replace_inline_media_query(
                                                    &value,
                                                    query,
                                                    "",
                                                ));
                                            }
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                if event.key() == Key::Enter
                                    && !event.modifiers().shift()
                                    && !is_streaming
                                {
                                    event.prevent_default();
                                    submit_remote_prompt(
                                        api,
                                        sid.clone(),
                                        draft,
                                        attachments,
                                        status,
                                    );
                                }
                            }
                        },
                        on_paste: move |_| {},
                        on_attach: move |_| insert_media_token(draft),
                        on_remove_attachment: move |index| {
                            let mut next = attachments.peek().clone();
                            if index < next.len() {
                                next.remove(index);
                                attachments.set(next);
                            }
                        },
                        on_action: {
                            let send_sid = submit_sid.clone();
                            let stop_sid = cancel_sid.clone();
                            move |_| {
                                if is_streaming {
                                    let Some(client) = api() else { return };
                                    let sid = stop_sid.clone();
                                    spawn(async move {
                                        let _ = client.cancel(&sid).await;
                                    });
                                } else {
                                    submit_remote_prompt(
                                        api,
                                        send_sid.clone(),
                                        draft,
                                        attachments,
                                        status,
                                    );
                                }
                            }
                        },
                    }
                }
            }
        }

    }
}

#[derive(Props, Clone, PartialEq)]
struct MobileStartPageProps {
    paired: bool,
    reachable: bool,
    sessions: Vec<RemoteSession>,
    agents: Vec<RemoteAgent>,
    draft: String,
    error: String,
    creating: bool,
    pair_value: String,
    pair_error: String,
    pairing: bool,
    on_draft: EventHandler<String>,
    on_submit: EventHandler<()>,
    on_open: EventHandler<RemoteSession>,
    on_start_agent: EventHandler<String>,
    on_pair_value: EventHandler<String>,
    on_pair: EventHandler<()>,
    on_scan: EventHandler<()>,
    on_disconnect: EventHandler<()>,
    on_open_team: EventHandler<()>,
}

#[component]
fn MobileStartPage(props: MobileStartPageProps) -> Element {
    let can_submit = !props.creating && !props.draft.trim().is_empty();
    let submit_from_key = props.on_submit;
    let submit_from_action = props.on_submit;
    let on_open = props.on_open;
    let on_start_agent = props.on_start_agent;

    rsx! {
        div {
            class: "relative isolate flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            header { class: "flex shrink-0 items-center gap-2 px-4 pb-3 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-6",
                span { class: "text-sm font-semibold tracking-tight text-foreground", "Vmux" }
                span { class: if props.paired { "ml-auto flex items-center gap-1.5 rounded-full border border-success/20 bg-success/[0.08] px-2.5 py-1 text-[10px] font-medium text-success" } else { "ml-auto flex items-center gap-1.5 rounded-full border border-border bg-muted px-2.5 py-1 text-[10px] font-medium text-muted-foreground" },
                    span { class: if props.paired { "h-1.5 w-1.5 rounded-full bg-success" } else { "h-1.5 w-1.5 rounded-full bg-muted-foreground" } }
                    {if props.reachable { translate("mobile-status-connected") } else if props.paired { translate("mobile-status-reaching") } else { translate("mobile-status-disconnected") }}
                }
                if props.paired {
                    button {
                        class: "ml-2 rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| props.on_open_team.call(()),
                        {translate("mobile-start-team")}
                    }
                    button {
                        class: "rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| props.on_disconnect.call(()),
                        {translate("mobile-pair-disconnect")}
                    }
                }
            }
            main { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-[calc(2rem+env(safe-area-inset-bottom))] pt-14 sm:px-6 md:pt-20",
                StartHero {
                    mark: rsx! {
                        div { class: "flex h-11 w-11 items-center justify-center rounded-2xl border border-border bg-gradient-to-br from-violet-500/80 to-cyan-400/80 text-sm font-bold text-white shadow-lg shadow-violet-950/40", "V" }
                    },
                    if props.paired {
                        div { class: "w-full",
                            PromptComposer {
                                value: props.draft.clone(),
                                placeholder: translate("mobile-start-search-placeholder"),
                                accent_color: "#a78bfa".to_string(),
                                accent_gradient: "from-violet-500 to-violet-700".to_string(),
                                autofocus: true,
                                show_attach: false,
                                disabled: props.creating,
                                action: PromptComposerAction::Send,
                                action_title: if props.creating { translate("mobile-start-starting") } else { translate("mobile-start-new-chat") },
                                action_enabled: can_submit,
                                on_input: move |value| props.on_draft.call(value),
                                on_keydown: move |event: KeyboardEvent| {
                                    if event.key() == Key::Enter && !event.modifiers().shift() {
                                        event.prevent_default();
                                        submit_from_key.call(());
                                    }
                                },
                                on_paste: move |_| {},
                                on_attach: move |_| {},
                                on_remove_attachment: move |_| {},
                                on_action: move |_| submit_from_action.call(()),
                            }
                            if !props.error.is_empty() {
                                div { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
                            }
                        }
                        section { class: "mt-6 w-full",
                            div { class: "mb-3 flex items-center gap-2 px-1",
                                h2 { class: "text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground", {translate("mobile-start-stacks")} }
                                span { class: "rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground", "{props.sessions.len()}" }
                            }
                            div { class: "overflow-hidden rounded-2xl border border-border bg-card",
                                if props.sessions.is_empty() {
                                    div { class: "px-4 py-8 text-center text-sm text-muted-foreground", {translate("mobile-start-no-stacks")} }
                                }
                                for (index, session) in props.sessions.iter().cloned().enumerate() {
                                    ResultRow {
                                        key: "{session.sid}",
                                        index,
                                        item: session_result_item(&session),
                                        selected: false,
                                        on_activate: {
                                            let next = session.clone();
                                            move |_| on_open.call(next.clone())
                                        },
                                        on_hover: move |_| {},
                                    }
                                }
                            }
                        }
                        if !props.agents.is_empty() {
                            section { class: "mt-6 w-full",
                                div { class: "mb-3 flex items-center gap-2 px-1",
                                    h2 { class: "text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground", "Start a chat" }
                                }
                                div { class: "overflow-hidden rounded-2xl border border-border bg-card",
                                    for (index, agent) in props.agents.iter().cloned().enumerate() {
                                        ResultRow {
                                            key: "{agent.id}",
                                            index,
                                            item: agent_result_item(&agent),
                                            selected: false,
                                            on_activate: {
                                                let url = agent.url.clone();
                                                move |_| on_start_agent.call(url.clone())
                                            },
                                            on_hover: move |_| {},
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        PairCard {
                            value: props.pair_value.clone(),
                            error: props.pair_error.clone(),
                            pairing: props.pairing,
                            on_value: props.on_pair_value,
                            on_pair: props.on_pair,
                            on_scan: props.on_scan,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AppHead() -> Element {
    rsx! {
        document::Title { "Vmux" }
        // This replaces the shell's own viewport tag, so it has to carry that tag's zoom lock
        // forward as well as viewport-fit. Dropping maximum-scale is what let focusing an input
        // zoom the page, which no font size on the input can prevent.
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: TAILWIND_CSS }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PairCardProps {
    value: String,
    error: String,
    pairing: bool,
    on_value: EventHandler<String>,
    on_pair: EventHandler<()>,
    on_scan: EventHandler<()>,
}

#[component]
fn PairCard(props: PairCardProps) -> Element {
    let mut show_link = use_signal(|| !props.value.trim().is_empty());
    let unavailable = use_hook(|| match qr_scanner::ScannerSupport::detect() {
        qr_scanner::ScannerSupport::Available => None,
        qr_scanner::ScannerSupport::Unavailable(reason) => Some(reason),
    });

    rsx! {
        div { class: "w-full",
            div { class: "mb-5 text-center",
                h2 { class: "text-base font-semibold text-foreground", {translate("mobile-pair-title")} }
                p { class: "mt-1 text-xs leading-5 text-muted-foreground", {translate("mobile-pair-subtitle")} }
            }
            button {
                class: "flex h-14 w-full items-center justify-center gap-2.5 rounded-2xl bg-primary text-sm font-semibold text-primary-foreground shadow-xl shadow-black/20 disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none active:scale-[0.99] active:bg-primary/90",
                r#type: "button",
                disabled: unavailable.is_some(),
                onclick: move |_| props.on_scan.call(()),
                svg {
                    class: "h-5 w-5",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 5a2 2 0 0 1 2-2h2" }
                    path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                    path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                    path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                    rect { width: "5", height: "5", x: "7", y: "7", rx: "1" }
                    path { d: "M17 7v.01" }
                    path { d: "M17 12v5" }
                    path { d: "M12 17h5" }
                }
                {translate("mobile-pair-scan")}
            }
            button {
                class: "mx-auto mt-4 block rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground active:bg-accent active:text-accent-foreground",
                r#type: "button",
                onclick: move |_| show_link.set(!show_link()),
                {if show_link() { translate("mobile-pair-hide-link") } else { translate("mobile-pair-show-link") }}
            }
            if let Some(reason) = unavailable.clone() {
                p { class: "mt-3 text-center text-xs leading-5 text-muted-foreground", "{reason}" }
            }
            if show_link() {
                form {
                    class: "mt-2 flex items-center gap-2 rounded-2xl border border-border bg-muted p-1.5",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 bg-transparent px-3 font-mono text-base text-foreground outline-none placeholder:text-muted-foreground",
                        r#type: "url",
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: translate("mobile-pair-link-placeholder"),
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    button {
                        class: "h-10 shrink-0 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground disabled:opacity-50 active:bg-secondary/80",
                        r#type: "submit",
                        disabled: props.pairing,
                        {if props.pairing { translate("mobile-pair-connecting") } else { translate("mobile-pair-connect") }}
                    }
                }
            }
            if !props.error.is_empty() {
                p { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
            }
        }
    }
}

/// Fold the room's event log into the shared transcript model. The desktop gets this from
/// `group_turns` on the daemon side; the relay does not pre-group yet, so mobile folds locally.
#[allow(clippy::too_many_arguments)]
fn open_session(
    api: Api,
    session: RemoteSession,
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
    mut connected: Signal<bool>,
    mut generation: Signal<u64>,
) {
    native_transition::NativeSheet::open();
    let sid = session.sid.clone();
    current.set(Some(session.clone()));
    room.set(MobileRoomProjection {
        room_id: Some(session.room_id.clone()),
        ..MobileRoomProjection::default()
    });
    live_delta.set(String::new());
    status.set(session.status.clone());
    approval.set(session.approval.clone());
    connected.set(false);
    let next_generation = generation().wrapping_add(1);
    generation.set(next_generation);
    spawn(async move {
        loop {
            if generation() != next_generation {
                return;
            }
            // A connection that survived a suspend is a connection whose socket the OS already
            // closed. Drop it before dialling rather than waiting for a request to stall.
            if take_resumed() {
                api.reset_transport().await;
            }
            match api.subscribe(&sid).await {
                Ok(mut subscription) => {
                    connected.set(true);
                    while let Some(event) = subscription.next().await {
                        if generation() != next_generation {
                            return;
                        }
                        let Some(event) = remote_event_from_shared(event) else {
                            continue;
                        };
                        let refresh_now = matches!(&event, RemoteEvent::Approval { .. });
                        apply_remote_event(event, current, room, live_delta, status, approval);
                        if refresh_now {
                            tokio::task::yield_now().await;
                        }
                    }
                }
                // Pairing is gone, or the stream route is — reconnecting brings back neither.
                Err(ApiError::Unauthorized | ApiError::NotFound) => return,
                Err(ApiError::Message(_)) => {}
            }
            connected.set(false);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

fn apply_remote_event(
    event: RemoteEvent,
    mut current: Signal<Option<RemoteSession>>,
    mut room: Signal<MobileRoomProjection>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
) {
    match event {
        RemoteEvent::Session { session } => {
            if room
                .peek()
                .room_id
                .as_ref()
                .is_some_and(|room_id| room_id != &session.room_id)
            {
                room.set(MobileRoomProjection::default());
            }
            status.set(session.status.clone());
            approval.set(session.approval.clone());
            current.set(Some(session));
        }
        RemoteEvent::Snapshot {
            room_id,
            through_seq,
            events,
        } => {
            let matches_session = current
                .peek()
                .as_ref()
                .is_none_or(|session| session.room_id == room_id);
            let has_newer_projection = {
                let projection = room.peek();
                projection.room_id.as_ref() == Some(&room_id)
                    && projection.through_seq > through_seq
            };
            if matches_session && !has_newer_projection {
                room.set(MobileRoomProjection {
                    room_id: Some(room_id),
                    through_seq,
                    events,
                });
                live_delta.set(String::new());
            }
        }
        RemoteEvent::Delta { room_id, text } => {
            let accepts_delta = room
                .peek()
                .room_id
                .as_ref()
                .is_none_or(|current| current == &room_id);
            if accepts_delta {
                if room.peek().room_id.is_none() {
                    room.write().room_id = Some(room_id);
                }
                live_delta.write().push_str(&text);
            }
        }
        RemoteEvent::Status { status: next } => {
            if !matches!(next, RemoteStatus::Streaming) {
                approval.set(None);
            }
            status.set(next);
        }
        RemoteEvent::Approval { approval: next } => approval.set(next),
    }
}

fn parse_pairing_url(input: &str) -> Result<Credentials, String> {
    let input = input.trim();
    if input.starts_with("vmux://") {
        let parsed = Url::parse(input).map_err(|_| translate("mobile-url-invalid"))?;
        if parsed.scheme() != "vmux" || parsed.host_str() != Some("pair") {
            return Err(translate("mobile-url-invalid"));
        }
        let params = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        let base_url = params
            .get("base")
            .map(|value| value.to_string())
            .ok_or_else(|| translate("mobile-url-no-address"))?;
        let token = params
            .get("token")
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| translate("mobile-url-no-token"))?;
        let base = Url::parse(&base_url).map_err(|_| translate("mobile-url-bad-address"))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(translate("mobile-url-scheme"));
        }
        // Absent when the desktop has no QUIC listener yet, which leaves the phone on HTTP
        // rather than failing to pair.
        let fingerprint = params
            .get("fp")
            .map(|value| value.to_string())
            .unwrap_or_default();
        let base_url = normalized_pairing_base(base)?;
        if base_url.is_empty() {
            return Err(translate("mobile-url-no-address"));
        }
        return Ok(Credentials {
            base_url,
            token,
            fingerprint,
        });
    }
    let start = input
        .find("https://")
        .or_else(|| input.find("http://"))
        .ok_or_else(|| translate("mobile-url-paste-full"))?;
    let candidate = input[start..].split_whitespace().next().unwrap_or_default();
    let parsed = Url::parse(candidate).map_err(|_| translate("mobile-url-invalid"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(translate("mobile-url-scheme"));
    }
    let token = parsed
        .fragment()
        .and_then(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .find(|(name, _)| name == "token")
                .map(|(_, value)| value.into_owned())
        })
        .filter(|token| !token.is_empty())
        .ok_or_else(|| translate("mobile-url-no-token"))?;
    let fingerprint = parsed
        .fragment()
        .and_then(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .find(|(name, _)| name == "fp")
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_default();
    let base_url = normalized_pairing_base(parsed)?;
    if base_url.is_empty() {
        return Err(translate("mobile-url-no-address"));
    }
    Ok(Credentials {
        base_url,
        token,
        fingerprint,
    })
}

fn normalized_pairing_base(mut url: Url) -> Result<String, String> {
    url.set_fragment(None);
    url.set_query(None);
    if url.origin().ascii_serialization() == "null" {
        return Ok(String::new());
    }
    let mut value = url.to_string();
    while value.ends_with('/') {
        value.pop();
    }
    Ok(value)
}

fn take_opened_url() -> Option<String> {
    OPENED_URLS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .pop()
}

fn pairing_url(credentials: &Credentials) -> String {
    format!("{}/#token={}", credentials.base_url, credentials.token)
}

fn cwd_name(cwd: &str) -> String {
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

fn file_extension_label(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_uppercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "FILE".to_string())
}

/// Present an installed agent as a launcher result, matching the desktop's agent rows.
fn agent_result_item(agent: &RemoteAgent) -> CommandBarResultItem {
    CommandBarResultItem::Page {
        url: agent.url.clone(),
        title: agent.name.clone(),
        icon: if agent.icon.is_empty() {
            PageIcon::None
        } else {
            PageIcon::Favicon(agent.icon.clone())
        },
        shortcut: String::new(),
        prompt_target: true,
    }
}

/// Present a relayed session as a launcher result, so the phone and the desktop draw the same row.
fn session_result_item(session: &RemoteSession) -> CommandBarResultItem {
    let mut location = format!("{} \u{b7} {}", session.runtime, cwd_name(&session.cwd));
    if let Some(model) = session.model.as_deref() {
        location.push_str(" \u{b7} ");
        location.push_str(model);
    }
    CommandBarResultItem::Stack {
        title: if session.title.is_empty() {
            session.name.clone()
        } else {
            session.title.clone()
        },
        url: format!("vmux://agent/{}", session.sid),
        icon: PageIcon::default(),
        pane_id: 0,
        tab_index: 0,
        location,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::chat::ChatBlock;

    /// The fingerprint is the whole basis for trusting the desktop's certificate. If it were
    /// dropped while parsing, the phone would silently fall back to an unpinned connection —
    /// a downgrade with no visible symptom, so both pairing shapes are covered.
    #[test]
    fn a_pairing_link_carries_the_certificate_fingerprint() {
        let expected = "c620a502885ddf230420184cc3a1b190792c14c1049ab76a6a63596054a1025e";

        let pasted = parse_pairing_url(&format!(
            "https://mac.example.ts.net/#token=secret&fp={expected}"
        ))
        .unwrap();
        let deep_link = parse_pairing_url(&format!(
            "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net&token=secret&fp={expected}"
        ))
        .unwrap();

        assert_eq!(pasted.fingerprint, expected);
        assert_eq!(deep_link.fingerprint, expected);
        assert_eq!(
            pasted.token, "secret",
            "the token must survive alongside it"
        );
    }

    /// A link with no fingerprint parses but cannot be used: there is no unpinned transport left
    /// to fall back to. It has to fail here, at the point of use, rather than at parse time —
    /// that is what lets the phone say "scan again" instead of "malformed link".
    #[test]
    fn a_link_without_a_fingerprint_parses_but_cannot_be_dialled() {
        let credentials = parse_pairing_url("https://mac.example.ts.net/#token=secret").unwrap();

        assert!(credentials.fingerprint.is_empty());
        assert_eq!(credentials.token, "secret");
        assert!(
            Api::new(credentials).is_err(),
            "an unpinned pairing must be refused, not silently downgraded"
        );
    }

    #[test]
    fn parses_pairing_url() {
        assert_eq!(
            parse_pairing_url("paste into Vmux: https://mac.example.ts.net/#token=secret").unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
            }
        );
    }

    #[test]
    fn parses_pairing_deep_link() {
        assert_eq!(
            parse_pairing_url(
                "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret"
            )
            .unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net:54821".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
            }
        );
    }

    #[test]
    fn pairing_url_preserves_relay_path() {
        assert_eq!(
            parse_pairing_url("http://localhost:8787/r/device-1/#token=secret").unwrap(),
            Credentials {
                base_url: "http://localhost:8787/r/device-1".to_string(),
                token: "secret".to_string(),
                fingerprint: String::new(),
            }
        );
    }

    impl MobileRoomProjection {
        fn sample() -> Self {
            Self {
                room_id: None,
                through_seq: 0,
                events: RoomEvent::from_messages(
                    "s",
                    0,
                    &[
                        Message::user("hello"),
                        Message::Assistant {
                            blocks: vec![AssistantBlock::Thinking("working".to_string())],
                        },
                        Message::ToolResult {
                            call_id: "tool-1".to_string(),
                            content: "done".to_string(),
                            is_error: false,
                        },
                        Message::Assistant {
                            blocks: vec![AssistantBlock::Text("answer".to_string())],
                        },
                    ],
                ),
            }
        }
    }

    #[test]
    fn groups_agent_activity_into_one_turn() {
        let items = MobileRoomProjection::sample().chat_items("", false);

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], ChatItem::User { .. }));
        assert!(matches!(
            &items[1],
            ChatItem::Turn(turn) if turn.blocks.len() == 3 && !turn.running
        ));
    }

    #[test]
    fn streaming_delta_extends_the_live_turn() {
        let items = MobileRoomProjection::sample().chat_items("partial", true);

        let ChatItem::Turn(turn) = &items[1] else {
            panic!("expected a turn");
        };
        assert!(turn.running);
        assert_eq!(
            turn.blocks.last(),
            Some(&ChatBlock::Text("partial".to_string()))
        );
    }
}
