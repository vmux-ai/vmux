//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

pub(crate) mod composer;
pub mod event;

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
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatCancel, ChatClearQueue, ChatResume, ChatSnapshot,
    ChatSubmit, RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions,
    ResumeListRequest, ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT,
    SlashCommandEntry, SlashCommands,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::client::acp::AcpSession;
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{AgentMessages, AgentSession, PromptQueue};
#[cfg(not(target_arch = "wasm32"))]
use crate::events::{AgentApprovalReply, ApprovalDecision};
#[cfg(not(target_arch = "wasm32"))]
use crate::run_state::AgentRunState;
#[cfg(not(target_arch = "wasm32"))]
use crate::strategy::{AgentStrategies, kind_supports_cross_runtime};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::PageMetadata;
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::agent::{AgentKind, SwapStackSession};
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
            ResumeListRequest,
            ResumeSession,
            RuntimeSwitchRequest,
        )>::for_hosts(&["agent"]))
            .add_observer(on_chat_submit)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_resume_list_request)
            .add_observer(on_resume_session)
            .add_observer(on_runtime_switch_request)
            .add_observer(reset_chat_synced_on_page_ready)
            .add_systems(
                Update,
                (
                    push_chat_to_page,
                    sync_chat_to_ready_views,
                    drain_resume_list_tasks,
                ),
            );
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
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
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
        let Ok((messages, state, profile, meta, queue)) = sessions.get(stack) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state, profile, meta, queue),
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
    profile: Option<&Profile>,
    meta: Option<&PageMetadata>,
    queue: &PromptQueue,
) -> ChatSnapshot {
    let messages_json = serde_json::to_string(&messages.0).unwrap_or_else(|_| "[]".to_string());
    let (status, error, call_id, name) = match state {
        AgentRunState::Idle => ("idle", String::new(), String::new(), String::new()),
        AgentRunState::Installing { pct, message } => {
            let text = match pct {
                Some(p) => format!("{message} ({p}%)"),
                None => message.clone(),
            };
            ("installing", text, String::new(), String::new())
        }
        AgentRunState::Streaming => ("streaming", String::new(), String::new(), String::new()),
        AgentRunState::AwaitingApproval { call_id, name, .. } => {
            ("awaiting", String::new(), call_id.clone(), name.clone())
        }
        AgentRunState::Errored(message) => {
            ("errored", message.clone(), String::new(), String::new())
        }
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
        agent_name,
        agent_icon,
        accent_color,
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
            Option<&Profile>,
            Option<&PageMetadata>,
            &PromptQueue,
        ),
        Or<(
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<PromptQueue>,
        )>,
    >,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, messages, state, profile, meta, queue) in &sessions {
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
            &snapshot_of(messages, state, profile, meta, queue),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.items.push_back(text);
        queue.paused = false;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_cancel(
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    sessions: Query<(Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((acp, page)) = sessions.get(parent.parent()) else {
        return;
    };
    let Some(sid) = acp
        .map(|s| s.sid.clone())
        .or_else(|| page.map(|s| s.sid.clone()))
    else {
        return;
    };
    service.0.send(ClientMessage::AgentCancel { sid });
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
        queue.paused = false;
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
        queue.items.clear();
        queue.paused = false;
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
fn sessions_for_kind(
    sessions: Vec<crate::client::cli::strategy::ResumableSession>,
    kind: AgentKind,
) -> Vec<crate::client::cli::strategy::ResumableSession> {
    sessions
        .into_iter()
        .filter(|session| session.kind == kind)
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn on_resume_list_request(
    trigger: On<BinReceive<ResumeListRequest>>,
    strategies: Option<Res<AgentStrategies>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    agent_sessions: Query<&AgentSession>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let strategies = strategies.map(|s| (*s).clone()).unwrap_or_default();
    let kind = child_of
        .get(webview)
        .ok()
        .map(ChildOf::parent)
        .and_then(|stack| {
            acp_sessions
                .get(stack)
                .ok()
                .and_then(|acp| AgentKind::from_url_segment(&acp.agent_id))
                .or_else(|| {
                    agent_sessions
                        .get(stack)
                        .ok()
                        .map(|session| session.kind)
                })
        });
    let task = IoTaskPool::get().spawn(async move {
        let sessions = kind
            .map(|kind| sessions_for_kind(strategies.list_all_sessions(), kind))
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let dir = s
                    .cwd
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.cwd.to_string_lossy().to_string());
                ResumableSessionEntry {
                    kind: s.kind.as_url_segment().to_string(),
                    sid: s.sid,
                    cwd: s.cwd.to_string_lossy().to_string(),
                    title: s.title,
                    subtitle: format!("{} · {}", relative_time(s.mtime), dir),
                    cross_runtime: s.cross_runtime,
                }
            })
            .collect();
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

/// Page → native: resume a picked session on this stack, in the current runtime.
#[cfg(not(target_arch = "wasm32"))]
fn on_resume_session(
    trigger: On<BinReceive<ResumeSession>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
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
    let prefer_acp = acp_sessions.get(stack).is_ok();
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let target = crate::AgentUrl::for_session(kind, &payload.sid, prefer_acp, &acp_ids);
    swap.write(SwapStackSession {
        stack,
        target_url: target.format(),
        cwd: std::path::PathBuf::from(&payload.cwd),
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
    fn resume_results_only_include_current_agent_kind() {
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
        let filtered = sessions_for_kind(
            vec![
                session(AgentKind::Claude, "claude-1"),
                session(AgentKind::Codex, "codex-1"),
            ],
            AgentKind::Claude,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].sid, "claude-1");
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
    fn composer_slash_items_support_mouse_selection() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("onclick: move |_| run_slash_command"));
        assert!(source.contains("onclick: move |_| select_resume_session"));
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
}
