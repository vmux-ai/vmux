//! Picking up an earlier conversation, and moving one between runtimes.
//!
//! Listing resumable sessions walks every agent's on-disk history, so it runs on the IO pool and
//! reaches the webview through a drain system. A handoff additionally has to rebuild the context
//! the new runtime never saw.

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};

use crate::client::acp::AcpSession;
use crate::components::AgentSession;
use crate::handoff::{DEFAULT_CONTEXT_LIMIT, build_context};
use crate::run_state::AgentRunState;
use crate::strategy::{AgentStrategies, acp_agent_kind, kind_supports_cross_runtime};
use vmux_chat::event::{
    RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions, ResumeListRequest,
    ResumeSession, RuntimeSwitchRequest,
};
use vmux_core::agent::{AgentKind, StackSessionHandoff, SwapStackSession};
use vmux_core::team::Profile;

/// Resuming a session, and switching the runtime that serves one.
pub(super) struct ChatResumePlugin;

impl Plugin for ChatResumePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(
            ResumeListRequest,
            ResumeSession,
            RuntimeSwitchRequest,
        )>::for_hosts(&["agent", "start"]))
            .add_observer(on_resume_list_request)
            .add_observer(on_resume_session)
            .add_observer(on_runtime_switch_request)
            .add_systems(
                Update,
                (drain_resume_list_tasks, drain_resume_handoff_tasks),
            );
    }
}

#[derive(Component)]
struct ResumeListTask {
    webview: Entity,
    task: Task<ResumableSessions>,
}

#[derive(Component)]
struct ResumeHandoffTask {
    stack: Entity,
    target_url: String,
    cwd: std::path::PathBuf,
    task: Task<Result<StackSessionHandoff, String>>,
}

/// Age in seconds for a session's last-modified time.
fn relative_time_seconds(mtime: std::time::SystemTime) -> u64 {
    std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Page → native: `/resume` was opened — reply with the on-disk session list.
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
                subtitle: dir,
                age_seconds: relative_time_seconds(session.mtime),
                agent_name,
                cross_runtime: session.cross_runtime,
            }
        })
        .collect()
}

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
        .and_then(|acp| acp_agent_kind(&acp.agent_id))
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
        && let Some(target_url) =
            foreign_handoff_target(&acp.agent_id, acp_agent_kind(&acp.agent_id), kind)
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

/// The target url + cwd for an ACP↔CLI runtime handoff of the current session, or `None` when
/// the handoff is unavailable (unknown agent, no session id yet, bad `to`).
fn runtime_switch_target(
    agent_id: &str,
    resume: Option<&str>,
    cwd: &std::path::Path,
    to: &str,
    acp_ids: &[String],
) -> Option<(String, std::path::PathBuf)> {
    let kind = acp_agent_kind(agent_id)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
    fn runtime_switch_builtin_acp_agents_to_cli() {
        let cases = [
            ("claude", "claude"),
            ("claude-acp", "claude"),
            ("codex", "codex"),
            ("codex-acp", "codex"),
            ("vibe", "vibe"),
            ("mistral-vibe", "vibe"),
        ];
        let ids = cases
            .iter()
            .map(|(id, _)| (*id).to_string())
            .collect::<Vec<_>>();
        for (agent_id, cli_segment) in cases {
            let got = runtime_switch_target(agent_id, Some("sid-9"), Path::new("/w"), "cli", &ids);
            assert_eq!(
                got,
                Some((
                    format!("vmux://agent/{cli_segment}/cli/sid-9"),
                    std::path::PathBuf::from("/w")
                ))
            );
        }
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
    fn runtime_switch_gated_for_unknown_agent() {
        let ids = vec!["claude".to_string()];
        assert_eq!(
            runtime_switch_target("custom", Some("s"), Path::new("/w"), "cli", &ids),
            None
        );
    }
}
