//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod composer;
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
mod turns;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatCancel, ChatClearQueue, ChatEscape, ChatResume,
    ChatSnapshot, ChatSubmit, RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions,
    ResumeListRequest, ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT,
    SlashCommandEntry, SlashCommands,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::turns::group_turns;
#[cfg(not(target_arch = "wasm32"))]
use crate::client::acp::AcpSession;
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{AgentMessages, AgentSession, PromptQueue};
#[cfg(not(target_arch = "wasm32"))]
use crate::events::{AgentApprovalReply, ApprovalDecision};
#[cfg(not(target_arch = "wasm32"))]
use crate::handoff::{
    DEFAULT_CONTEXT_LIMIT, ImportedConversation, build_context, visible_messages,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::run_state::{AgentRunState, AgentTurnMeta};
#[cfg(not(target_arch = "wasm32"))]
use crate::strategy::{AgentStrategies, kind_supports_cross_runtime};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::PageMetadata;
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::agent::{AgentKind, StackSessionHandoff, SwapStackSession};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::team::Profile;
#[cfg(not(target_arch = "wasm32"))]
use vmux_service::client::ServiceClient;
#[cfg(not(target_arch = "wasm32"))]
use vmux_service::protocol::ClientMessage;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

#[cfg(any(test, target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ApprovalDetail {
    label: String,
    value: String,
}

#[cfg(any(test, target_arch = "wasm32"))]
fn approval_details(args_json: &str) -> Vec<ApprovalDetail> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(args_json) else {
        return if args_json.trim().is_empty() {
            Vec::new()
        } else {
            vec![ApprovalDetail {
                label: "Details".to_string(),
                value: args_json.to_string(),
            }]
        };
    };
    let mut details = Vec::new();
    flatten_approval_details("", &value, &mut details);
    details
}

#[cfg(any(test, target_arch = "wasm32"))]
fn flatten_approval_details(
    path: &str,
    value: &serde_json::Value,
    details: &mut Vec<ApprovalDetail>,
) {
    if let serde_json::Value::Object(fields) = value {
        for (name, value) in fields {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            flatten_approval_details(&child_path, value, details);
        }
        return;
    }
    let label = approval_detail_label(path);
    let value = match value {
        serde_json::Value::String(value) => value.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    details.push(ApprovalDetail { label, value });
}

#[cfg(any(test, target_arch = "wasm32"))]
fn approval_detail_label(path: &str) -> String {
    let path = path.strip_prefix("arguments.").unwrap_or(path);
    let label = if path.is_empty() { "details" } else { path };
    label
        .split('.')
        .map(|part| {
            let words = part.replace('_', " ");
            let mut chars = words.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(any(test, target_arch = "wasm32"))]
fn has_collapsible_steps(turn: &event::ChatTurn) -> bool {
    !turn.steps.is_empty()
}

#[cfg(not(target_arch = "wasm32"))]
pub struct AgentChatPagePlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(
            ChatSubmit,
            ChatApproval,
            ChatCancel,
            ChatResume,
            ChatClearQueue,
            ChatEscape,
            ResumeListRequest,
            ResumeSession,
            RuntimeSwitchRequest,
        )>::for_hosts(&["agent"]))
            .add_observer(on_chat_submit)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_chat_escape)
            .add_observer(on_resume_list_request)
            .add_observer(on_resume_session)
            .add_observer(on_runtime_switch_request)
            .add_observer(reset_chat_synced_on_page_ready)
            .add_systems(
                Update,
                (
                    (track_turn_duration, push_chat_to_page).chain(),
                    sync_chat_to_ready_views,
                    drain_resume_list_tasks,
                    drain_resume_handoff_tasks,
                ),
            );
    }
}

/// Record per-turn wall-clock from `AgentRunState` edges (covers page + ACP mutation sites
/// uniformly). Idempotent: the `turn_start` guard tolerates repeated same-state sets and does
/// not reset across a mid-turn `AwaitingApproval`.
#[cfg(not(target_arch = "wasm32"))]
fn track_turn_duration(
    time: Res<Time>,
    mut sessions: Query<(&AgentRunState, &mut AgentTurnMeta), Changed<AgentRunState>>,
) {
    for (state, mut meta) in &mut sessions {
        match state {
            AgentRunState::Streaming => {
                if meta.turn_start.is_none() {
                    meta.turn_start = Some(time.elapsed());
                }
            }
            AgentRunState::Idle | AgentRunState::Errored(_) => {
                if let Some(start) = meta.turn_start.take() {
                    meta.durations
                        .push(time.elapsed().saturating_sub(start).as_secs() as u32);
                }
            }
            AgentRunState::AwaitingApproval { .. } | AgentRunState::Installing { .. } => {}
        }
    }
}

/// Marks a chat-page webview (ACP or Page agent) so the ready→resync path can find it cheaply.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct AgentChatView;

/// Set once the current snapshot has been pushed to a ready chat webview; cleared when the page
/// (re)signals ready (mount or Cmd+R reload) so the transcript is re-pushed instead of blanking.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ChatSynced;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ResumeListTask {
    webview: Entity,
    task: Task<ResumableSessions>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ResumeHandoffTask {
    stack: Entity,
    target_url: String,
    cwd: std::path::PathBuf,
    task: Task<Result<StackSessionHandoff, String>>,
}

/// Push the current transcript + slash commands to any chat webview that is ready but not yet
/// synced. Runs every frame and retries until the webview's emit channel is ready, so the very
/// first snapshot always lands. Re-runs after a reload because [`reset_chat_synced_on_page_ready`]
/// clears `ChatSynced` when the page re-signals ready — without this, Cmd+R blanked the chat
/// (the `Changed`/`Added` pushes never re-fire for an unchanged, already-added session).
#[cfg(not(target_arch = "wasm32"))]
fn sync_chat_to_ready_views(
    pending: Query<
        Entity,
        (
            With<AgentChatView>,
            With<vmux_core::page::PageReady>,
            Without<ChatSynced>,
        ),
    >,
    child_of: Query<&ChildOf>,
    sessions: Query<(
        &AgentMessages,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
        Option<&ImportedConversation>,
    )>,
    acp_sessions: Query<&AcpSession>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for webview in &pending {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let stack = parent.parent();
        let Ok((messages, state, turn_meta, profile, meta, queue, imported)) = sessions.get(stack)
        else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state, turn_meta, profile, meta, queue, imported),
        ));
        let cross = acp_sessions
            .get(stack)
            .ok()
            .and_then(|acp| AgentKind::from_url_segment(&acp.agent_id))
            .map(kind_supports_cross_runtime)
            .unwrap_or(false);
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            SLASH_COMMANDS_EVENT,
            &SlashCommands {
                commands: slash_commands_for(cross),
            },
        ));
        commands.entity(webview).insert(ChatSynced);
    }
}

/// A chat webview re-signals `PageReady` on every (re)mount, including a Cmd+R reload. Clear its
/// `ChatSynced` marker so [`sync_chat_to_ready_views`] re-pushes the full transcript.
#[cfg(not(target_arch = "wasm32"))]
fn reset_chat_synced_on_page_ready(
    trigger: On<BinReceive<vmux_core::page::PageReady>>,
    chat_views: Query<(), With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn snapshot_of(
    messages: &AgentMessages,
    state: &AgentRunState,
    turn_meta: Option<&AgentTurnMeta>,
    profile: Option<&Profile>,
    meta: Option<&PageMetadata>,
    queue: &PromptQueue,
    imported: Option<&ImportedConversation>,
) -> ChatSnapshot {
    let display_messages = visible_messages(imported, &messages.0);
    let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
    let running = matches!(state, AgentRunState::Streaming);
    let items = group_turns(&display_messages, durations, running);
    let messages_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
    let (status, error) = match state {
        AgentRunState::Idle => ("idle", String::new()),
        AgentRunState::Installing { pct, message } => {
            let text = match pct {
                Some(p) => format!("{message} ({p}%)"),
                None => message.clone(),
            };
            ("installing", text)
        }
        AgentRunState::Streaming => ("streaming", String::new()),
        AgentRunState::AwaitingApproval { .. } => ("awaiting", String::new()),
        AgentRunState::Errored(message) => ("errored", message.clone()),
    };
    let (call_id, name, args_json) = match state {
        AgentRunState::AwaitingApproval {
            call_id,
            name,
            args,
        } => (call_id.clone(), name.clone(), args.to_string()),
        _ => (String::new(), String::new(), String::new()),
    };
    let (agent_name, accent_color) = profile
        .map(|p| (p.name.clone(), p.avatar.color.clone()))
        .unwrap_or_default();
    let agent_icon = meta
        .map(|m| m.icon.favicon_url().to_string())
        .unwrap_or_default();
    ChatSnapshot {
        messages_json,
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
        approval_args_json: args_json,
        agent_name,
        agent_icon,
        accent_color,
        handoff_source: imported
            .map(|imported| imported.source_agent.clone())
            .unwrap_or_default(),
        handoff_truncated: imported.is_some_and(|imported| imported.truncated),
        handoff_message_count: imported
            .map(|imported| {
                u32::try_from(group_turns(&imported.messages, &[], false).len()).unwrap_or(u32::MAX)
            })
            .unwrap_or_default(),
        queued: queue.items.iter().cloned().collect(),
        paused: queue.paused,
    }
}

/// Push each changed session's conversation + run-state to its pane webview (the child
/// `Browser` of the session entity).
#[cfg(not(target_arch = "wasm32"))]
fn push_chat_to_page(
    sessions: Query<
        (
            Entity,
            &AgentMessages,
            &AgentRunState,
            Option<&AgentTurnMeta>,
            Option<&Profile>,
            Option<&PageMetadata>,
            &PromptQueue,
            Option<&ImportedConversation>,
        ),
        Or<(
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<AgentTurnMeta>,
            Changed<PromptQueue>,
            Changed<Profile>,
            Changed<ImportedConversation>,
        )>,
    >,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, messages, state, turn_meta, profile, meta, queue, imported) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| is_browser.contains(e)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state, turn_meta, profile, meta, queue, imported),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, &mut AgentRunState)>,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    if let Ok((mut queue, mut state)) = sessions.get_mut(parent.parent()) {
        enqueue_prompt(&mut queue, &mut state, text);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn enqueue_prompt(queue: &mut PromptQueue, state: &mut AgentRunState, text: String) {
    queue.enqueue(text);
    if matches!(state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cancel_session(
    service: Option<&ServiceClient>,
    acp: Option<&AcpSession>,
    page: Option<&AgentSession>,
) {
    let Some(service) = service else {
        return;
    };
    let Some(sid) = acp
        .map(|session| session.sid.clone())
        .or_else(|| page.map(|session| session.sid.clone()))
    else {
        return;
    };
    service.0.send(ClientMessage::AgentCancel { sid });
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_cancel(
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.flush_pending() {
        queue.cancel_flush();
    }
    cancel_session(service.as_deref(), acp, page);
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_resume(
    trigger: On<BinReceive<ChatResume>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.resume();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_clear_queue(
    trigger: On<BinReceive<ChatClearQueue>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_escape(
    trigger: On<BinReceive<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, mut state, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    let running = matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    );
    let flush = if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        false
    } else {
        queue.request_flush()
    };
    if flush && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if running {
        cancel_session(service.as_deref(), acp, page);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_approval(
    trigger: On<BinReceive<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let decision = match payload.decision {
        1 => ApprovalDecision::Allow,
        2 => ApprovalDecision::AllowAlways,
        _ => ApprovalDecision::Deny,
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision,
    });
}

/// A short "2h ago"-style age for a session's last-modified time.
#[cfg(not(target_arch = "wasm32"))]
fn relative_time(mtime: std::time::SystemTime) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

/// The slash commands offered on an ACP pane: `/resume` always, `/cli` only when the agent's
/// runtime shares session ids with its CLI (so the handoff actually continues the conversation).
#[cfg(not(target_arch = "wasm32"))]
fn slash_commands_for(cross_runtime: bool) -> Vec<SlashCommandEntry> {
    let mut v = vec![SlashCommandEntry {
        name: "resume".into(),
        description: "Resume a past session".into(),
    }];
    if cross_runtime {
        v.push(SlashCommandEntry {
            name: "cli".into(),
            description: "Continue this session in the CLI".into(),
        });
    }
    v
}

/// The target url + cwd for an ACP↔CLI runtime handoff of the current session, or `None` when
/// the handoff is unavailable (unknown/non-cross-runtime kind, no session id yet, bad `to`).
#[cfg(not(target_arch = "wasm32"))]
fn runtime_switch_target(
    agent_id: &str,
    resume: Option<&str>,
    cwd: &std::path::Path,
    to: &str,
    acp_ids: &[String],
) -> Option<(String, std::path::PathBuf)> {
    let kind = AgentKind::from_url_segment(agent_id)?;
    if !kind_supports_cross_runtime(kind) {
        return None;
    }
    let sid = resume?;
    let target = match to {
        "cli" => crate::AgentUrl::Cli {
            kind,
            sid: sid.to_string(),
        },
        "acp" => crate::AgentUrl::for_session(kind, sid, true, acp_ids),
        _ => return None,
    };
    Some((target.format(), cwd.to_path_buf()))
}

/// Page → native: `/resume` was opened — reply with the on-disk session list.
#[cfg(not(target_arch = "wasm32"))]
fn resume_entries(
    sessions: Vec<crate::client::cli::strategy::ResumableSession>,
    active_kind: Option<AgentKind>,
    active_name: &str,
) -> Vec<ResumableSessionEntry> {
    sessions
        .into_iter()
        .map(|session| {
            let dir = session
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| session.cwd.to_string_lossy().to_string());
            let agent_name = if Some(session.kind) == active_kind && !active_name.is_empty() {
                active_name.to_string()
            } else {
                session.kind.display_name().to_string()
            };
            ResumableSessionEntry {
                kind: session.kind.as_url_segment().to_string(),
                sid: session.sid,
                cwd: session.cwd.to_string_lossy().to_string(),
                title: session.title,
                subtitle: format!("{} · {}", relative_time(session.mtime), dir),
                agent_name,
                cross_runtime: session.cross_runtime,
            }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn foreign_handoff_target(
    active_agent_id: &str,
    active_kind: Option<AgentKind>,
    source_kind: AgentKind,
) -> Option<String> {
    (active_kind != Some(source_kind)).then(|| {
        crate::AgentUrl::Acp {
            id: active_agent_id.to_string(),
            sid: None,
        }
        .format()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn resume_agent_name(
    profile: Option<&Profile>,
    kind: Option<AgentKind>,
    acp_id: Option<&str>,
) -> String {
    profile
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| kind.map(|kind| kind.display_name().to_string()))
        .or_else(|| acp_id.map(str::to_string))
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn on_resume_list_request(
    trigger: On<BinReceive<ResumeListRequest>>,
    strategies: Option<Res<AgentStrategies>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    agent_sessions: Query<&AgentSession>,
    profiles: Query<&Profile>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let strategies = strategies.map(|s| (*s).clone()).unwrap_or_default();
    let stack = child_of.get(webview).ok().map(ChildOf::parent);
    let acp = stack.and_then(|stack| acp_sessions.get(stack).ok());
    let kind = acp
        .and_then(|acp| AgentKind::from_url_segment(&acp.agent_id))
        .or_else(|| {
            stack.and_then(|stack| agent_sessions.get(stack).ok().map(|session| session.kind))
        });
    let agent_name = resume_agent_name(
        stack.and_then(|stack| profiles.get(stack).ok()),
        kind,
        acp.map(|acp| acp.agent_id.as_str()),
    );
    let task = IoTaskPool::get().spawn(async move {
        let sessions = resume_entries(strategies.list_all_sessions(), kind, &agent_name);
        ResumableSessions { sessions }
    });
    commands.spawn(ResumeListTask { webview, task });
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_resume_list_tasks(
    mut tasks: Query<(Entity, &mut ResumeListTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(sessions) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        commands.trigger(BinHostEmitEvent::from_rkyv(
            task.webview,
            RESUMABLE_SESSIONS_EVENT,
            &sessions,
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_resume_handoff_tasks(
    mut tasks: Query<(Entity, &mut ResumeHandoffTask)>,
    mut states: Query<&mut AgentRunState>,
    mut swap: MessageWriter<SwapStackSession>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(handoff) => {
                swap.write(SwapStackSession {
                    stack: pending.stack,
                    target_url: pending.target_url.clone(),
                    cwd: pending.cwd.clone(),
                    handoff: Some(handoff),
                });
            }
            Err(message) => {
                if let Ok(mut state) = states.get_mut(pending.stack) {
                    *state = AgentRunState::Errored(message);
                }
            }
        }
    }
}

/// Page → native: resume a picked session on this stack, in the current runtime.
#[cfg(not(target_arch = "wasm32"))]
fn on_resume_session(
    trigger: On<BinReceive<ResumeSession>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    mut commands: Commands,
    mut swap: MessageWriter<SwapStackSession>,
) {
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let Some(kind) = AgentKind::from_url_segment(&payload.kind) else {
        return;
    };
    if let Ok(acp) = acp_sessions.get(stack)
        && let Some(target_url) = foreign_handoff_target(
            &acp.agent_id,
            AgentKind::from_url_segment(&acp.agent_id),
            kind,
        )
    {
        let strategies = strategies
            .map(|strategies| (*strategies).clone())
            .unwrap_or_default();
        let source_sid = payload.sid.clone();
        let source_agent = kind.display_name().to_string();
        let cwd = std::path::PathBuf::from(&payload.cwd);
        let task = IoTaskPool::get().spawn(async move {
            let messages = strategies.load_transcript(kind, &source_sid)?;
            let built = build_context(&messages, DEFAULT_CONTEXT_LIMIT);
            let messages_json = serde_json::to_string(&messages)
                .map_err(|err| format!("serialize imported conversation: {err}"))?;
            Ok(StackSessionHandoff {
                source_agent,
                source_kind: kind,
                source_sid,
                messages_json,
                context: built.text,
                truncated: built.truncated,
            })
        });
        commands.spawn(ResumeHandoffTask {
            stack,
            target_url,
            cwd,
            task,
        });
        return;
    }
    let prefer_acp = acp_sessions.get(stack).is_ok();
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let target = crate::AgentUrl::for_session(kind, &payload.sid, prefer_acp, &acp_ids);
    swap.write(SwapStackSession {
        stack,
        target_url: target.format(),
        cwd: std::path::PathBuf::from(&payload.cwd),
        handoff: None,
    });
}

/// Page → native: hand the current ACP session off to the other runtime (the `/cli` fallback).
#[cfg(not(target_arch = "wasm32"))]
fn on_runtime_switch_request(
    trigger: On<BinReceive<RuntimeSwitchRequest>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
    mut swap: MessageWriter<SwapStackSession>,
) {
    let to = trigger.event().payload.to.clone();
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let Ok(acp) = acp_sessions.get(stack) else {
        bevy::log::warn!("runtime switch: current pane is not an ACP session");
        return;
    };
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let Some((target_url, cwd)) = runtime_switch_target(
        &acp.agent_id,
        acp.resume.as_deref(),
        &acp.cwd,
        &to,
        &acp_ids,
    ) else {
        bevy::log::warn!(
            "runtime switch to '{to}' unavailable for ACP agent '{}' (no shared session id yet)",
            acp.agent_id
        );
        return;
    };
    swap.write(SwapStackSession {
        stack,
        target_url,
        cwd,
        handoff: None,
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn runtime_switch_claude_acp_to_cli() {
        let ids = vec!["claude".to_string()];
        let got = runtime_switch_target("claude", Some("sid-9"), Path::new("/w"), "cli", &ids);
        assert_eq!(
            got,
            Some((
                "vmux://agent/claude/cli/sid-9".to_string(),
                std::path::PathBuf::from("/w")
            ))
        );
    }

    #[test]
    fn runtime_switch_requires_session_id() {
        let ids = vec!["claude".to_string()];
        assert_eq!(
            runtime_switch_target("claude", None, Path::new("/w"), "cli", &ids),
            None
        );
    }

    #[test]
    fn runtime_switch_gated_for_non_cross_runtime_kind() {
        let ids = vec!["claude".to_string()];
        assert_eq!(
            runtime_switch_target("codex", Some("s"), Path::new("/w"), "cli", &ids),
            None
        );
    }

    #[test]
    fn slash_commands_include_cli_only_when_cross_runtime() {
        assert_eq!(slash_commands_for(false).len(), 1);
        let with_cli = slash_commands_for(true);
        assert_eq!(with_cli.len(), 2);
        assert_eq!(with_cli[1].name, "cli");
    }

    #[test]
    fn resume_results_include_all_agent_kinds_with_source_labels() {
        use crate::client::cli::strategy::ResumableSession;
        use std::time::SystemTime;

        let session = |kind, sid: &str| ResumableSession {
            kind,
            sid: sid.into(),
            cwd: "/work".into(),
            mtime: SystemTime::UNIX_EPOCH,
            title: sid.into(),
            cross_runtime: kind_supports_cross_runtime(kind),
        };
        let entries = resume_entries(
            vec![
                session(AgentKind::Claude, "claude-1"),
                session(AgentKind::Codex, "codex-1"),
            ],
            Some(AgentKind::Claude),
            "Antigravity",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].agent_name, "Antigravity");
        assert_eq!(entries[1].agent_name, "Codex");
    }

    #[test]
    fn foreign_resume_keeps_active_acp_agent_fresh() {
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Codex,),
            Some("vmux://agent/claude".to_string())
        );
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Claude,),
            None
        );
        assert_eq!(
            foreign_handoff_target("custom-acp", None, AgentKind::Codex),
            Some("vmux://agent/custom-acp".to_string())
        );
    }

    #[test]
    fn snapshot_reports_grouped_imported_item_boundary() {
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_kind: AgentKind::Codex,
            source_sid: "codex-1".into(),
            messages: vec![
                crate::Message::User { text: "one".into() },
                crate::Message::Assistant {
                    blocks: vec![crate::AssistantBlock::ToolUse {
                        call_id: "call-1".into(),
                        name: "run".into(),
                        args: "{}".into(),
                    }],
                },
                crate::Message::ToolResult {
                    call_id: "call-1".into(),
                    content: "two".into(),
                    is_error: false,
                },
            ],
            truncated: false,
            first_prompt: None,
        };
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            &PromptQueue::default(),
            Some(&imported),
        );

        assert_eq!(snapshot.handoff_message_count, 2);
    }

    #[test]
    fn snapshot_includes_approval_tool_and_input() {
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "vmux.run".into(),
                args: serde_json::json!({"command": "echo hi", "focus": true}),
            },
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
        );

        assert_eq!(snapshot.approval_name, "vmux.run");
        assert_eq!(
            snapshot.approval_args_json,
            r#"{"command":"echo hi","focus":true}"#
        );
    }

    #[test]
    fn approval_prompt_renders_structured_tool_input() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("snap.approval_args_json.clone()"));
        assert!(source.contains("approval_details(&args_json)"));
    }

    #[test]
    fn approval_details_parse_nested_json() {
        assert_eq!(
            approval_details(
                r#"{"arguments":{"path":"/tmp/SKILL.md"},"server":"vmux","tool":"read_file"}"#
            ),
            vec![
                ApprovalDetail {
                    label: "Path".into(),
                    value: "/tmp/SKILL.md".into(),
                },
                ApprovalDetail {
                    label: "Server".into(),
                    value: "vmux".into(),
                },
                ApprovalDetail {
                    label: "Tool".into(),
                    value: "read_file".into(),
                },
            ]
        );
        assert!(approval_details("{}").is_empty());
    }

    #[test]
    fn empty_turn_has_no_collapsible_content() {
        let empty = event::ChatTurn::default();
        assert!(!has_collapsible_steps(&empty));

        let populated = event::ChatTurn {
            steps: vec![event::ChatBlock::Thinking("working".into())],
            ..Default::default()
        };
        assert!(has_collapsible_steps(&populated));
    }

    #[test]
    fn disclosure_icons_use_animated_plus_minus() {
        let page = include_str!("chat_page/page.rs");
        let css = include_str!("../../vmux_server/assets/index.css");
        assert!(!page.contains('▸'));
        assert!(page.contains("render_disclosure_icon"));
        assert!(css.contains(".disclosure[open] > summary .disclosure-icon::after"));
    }

    #[test]
    fn submitting_after_error_rearms_prompt_dispatch() {
        let mut queue = PromptQueue::default();
        let mut state = AgentRunState::Errored("failed".into());

        enqueue_prompt(&mut queue, &mut state, "retry".into());

        assert!(matches!(state, AgentRunState::Idle));
        assert_eq!(queue.items.front().map(String::as_str), Some("retry"));
        assert!(!queue.paused);
    }

    #[test]
    fn normal_cancel_overrides_pending_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_cancel);
        let mut queue = PromptQueue::default();
        queue.items.push_back("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatCancel> {
            webview,
            payload: ChatCancel,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn escape_flush_rearms_errored_queue() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.items.push_back("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Idle)
        ));
        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
    }

    #[test]
    fn escape_without_queue_clears_stale_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.items.push_back("queued".into());
        assert!(queue.request_flush());
        queue.items.clear();
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Streaming))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn resume_agent_name_prefers_profile_then_kind_then_id() {
        let profile = Profile::registry("Antigravity", "antigravity");
        assert_eq!(
            resume_agent_name(Some(&profile), Some(AgentKind::Claude), Some("claude")),
            "Antigravity"
        );
        assert_eq!(
            resume_agent_name(None, Some(AgentKind::Claude), Some("claude")),
            "Claude"
        );
        assert_eq!(
            resume_agent_name(None, None, Some("custom-acp")),
            "custom-acp"
        );
    }

    #[test]
    fn resume_list_scan_runs_on_io_task_pool() {
        let source = include_str!("chat_page.rs");
        let handler = source
            .split("fn on_resume_list_request")
            .nth(1)
            .expect("resume-list handler")
            .split("fn on_resume_session")
            .next()
            .expect("resume-list handler body");
        assert!(handler.contains("IoTaskPool::get().spawn"));
        assert!(source.contains("fn drain_resume_list_tasks"));
    }

    #[test]
    fn composer_resume_menu_remains_escapeable_when_empty() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("let session_menu_open = resume_query.is_some();"));
        assert!(source.contains("e.key() == Key::Escape && !command_modifier"));
    }

    #[test]
    fn composer_resume_selector_supports_prompt_filter_and_keyboard_navigation() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("SelectorMode::Resume"));
        assert!(source.contains("filter_sessions"));
        assert!(source.contains("No matching sessions"));
        assert!(source.contains("menu_direction"));
        assert!(source.contains("ScrollLogicalPosition::Nearest"));
        assert!(source.contains("agent-selector-item-{i}"));
    }

    #[test]
    fn composer_resume_rows_render_agent_name() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("session.agent_name"));
        assert!(source.contains("max-w-[40%] shrink-0 truncate text-xs text-muted-foreground"));
    }

    #[test]
    fn composer_captures_global_prompt_input_without_stealing_shortcuts() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("install_global_prompt_input"));
        assert!(source.contains("meta_key() || event.ctrl_key() || event.alt_key()"));
        assert!(source.contains("PromptEdit::Backspace"));
        assert!(source.contains("PromptEdit::Delete"));
        assert!(source.contains("dispatch_keyboard_event"));
    }

    #[test]
    fn composer_slash_items_support_mouse_selection() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("onclick: move |_| run_slash_command"));
        assert!(source.contains("onclick: move |_| select_resume_session"));
    }

    #[test]
    fn composer_escape_defers_queue_state_to_native() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("try_cef_bin_emit_rkyv(&ChatEscape)"));
        assert!(!source.contains("ChatFlush"));
    }

    #[test]
    fn queued_flush_controls_repurpose_composer_button() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("kbd {"));
        assert!(source.contains("if queued.read().is_empty()"));
        assert!(source.contains("title: \"Stop\""));
        assert!(source.contains("rect { x: \"6\", y: \"6\""));
        assert!(source.contains("\"Send all queued prompts now (Esc)\""));
        assert!(source.contains("path { d: \"M12 19V5\" }"));
    }

    #[test]
    fn composer_ctrl_c_cancel_does_not_use_streaming_snapshot() {
        let source = include_str!("chat_page/page.rs");
        let handler = source
            .split("} else if e.modifiers().ctrl()")
            .nth(1)
            .expect("ctrl-c handler")
            .split("},")
            .next()
            .expect("ctrl-c handler body");
        assert!(handler.contains("try_cef_bin_emit_rkyv(&ChatCancel)"));
        assert!(!handler.contains("&& streaming"));
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        use bevy::prelude::*;
        use bevy_cef::prelude::BinReceive;
        use vmux_core::page::PageReady;

        let mut app = App::new();
        app.add_observer(reset_chat_synced_on_page_ready);

        let chat = app.world_mut().spawn((AgentChatView, ChatSynced)).id();
        let other = app.world_mut().spawn(ChatSynced).id();

        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: chat,
            payload: PageReady {},
        });
        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: other,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert!(
            app.world().get::<ChatSynced>(chat).is_none(),
            "a chat view must re-sync (ChatSynced cleared) when the page reloads"
        );
        assert!(
            app.world().get::<ChatSynced>(other).is_some(),
            "a non-chat view must be left untouched"
        );
    }

    fn duration_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, track_turn_duration);
        app
    }

    #[test]
    fn streaming_then_idle_records_one_duration() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        assert!(
            app.world()
                .get::<AgentTurnMeta>(e)
                .unwrap()
                .turn_start
                .is_some()
        );
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::Idle;
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert_eq!(meta.durations.len(), 1);
        assert!(meta.turn_start.is_none());
    }

    #[test]
    fn awaiting_approval_does_not_finalize() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::AwaitingApproval {
            call_id: "c".into(),
            name: "n".into(),
            args: serde_json::Value::Null,
        };
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert!(meta.durations.is_empty());
        assert!(meta.turn_start.is_some());
    }
}
