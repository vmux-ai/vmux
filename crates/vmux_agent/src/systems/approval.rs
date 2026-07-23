use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::client::acp::AcpSession;
use crate::components::{AgentApprovalPolicy, AgentSession, approval_tool_key};
use crate::events::{AgentApprovalReply, ApprovalDecision};
use crate::run_state::AgentRunState;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ApprovalDecision as ProtoDecision, ClientMessage};

#[derive(Default, Deserialize, Serialize)]
struct SavedApprovalGrants {
    by_agent: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
}

#[derive(Resource)]
pub(crate) struct AgentApprovalStore {
    path: PathBuf,
    grants: SavedApprovalGrants,
}

impl AgentApprovalStore {
    pub(crate) fn load() -> Self {
        Self::load_from(vmux_core::profile::profile_dir().join("agent-approvals.json"))
    }

    fn load_from(path: PathBuf) -> Self {
        let grants = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, grants }
    }

    fn policy_for(&self, agent: &str, cwd: &Path) -> AgentApprovalPolicy {
        let agent = canonical_agent_id(agent);
        let auto = repository_key(cwd)
            .and_then(|repository| {
                self.grants
                    .by_agent
                    .get(&agent)
                    .and_then(|repositories| repositories.get(&repository))
                    .cloned()
            })
            .unwrap_or_default()
            .into_iter()
            .collect();
        AgentApprovalPolicy { auto }
    }

    fn remember(&mut self, agent: &str, cwd: &Path, tool: &str) {
        let Some(repository) = repository_key(cwd) else {
            return;
        };
        let inserted = self
            .grants
            .by_agent
            .entry(canonical_agent_id(agent))
            .or_default()
            .entry(repository)
            .or_default()
            .insert(approval_tool_key(tool));
        if inserted && let Err(error) = self.save() {
            warn!("failed to save agent approvals: {error}");
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(parent) = self.path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(&self.grants).map_err(std::io::Error::other)?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, &self.path)
    }
}

fn repository_key(cwd: &Path) -> Option<String> {
    vmux_git::worktree::common_dir_of(cwd)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn canonical_agent_id(agent: &str) -> String {
    match agent.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-acp" => "claude".to_string(),
        "codex" | "codex-acp" => "codex".to_string(),
        "vibe" | "vibe-acp" | "mistral-vibe" => "vibe".to_string(),
        other => other.strip_suffix("-acp").unwrap_or(other).to_string(),
    }
}

pub(crate) fn sync_persisted_acp_approval_policy(
    store: Res<AgentApprovalStore>,
    mut sessions: Query<(&AcpSession, &mut AgentApprovalPolicy), Changed<AcpSession>>,
) {
    for (session, mut policy) in &mut sessions {
        *policy = store.policy_for(&session.agent_id, &session.cwd);
    }
}

fn protocol_decision(decision: ApprovalDecision) -> ProtoDecision {
    match decision {
        ApprovalDecision::Allow => ProtoDecision::Allow,
        ApprovalDecision::AllowAlways => ProtoDecision::AllowAlways,
        ApprovalDecision::Deny => ProtoDecision::Deny,
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn handle_approval_reply(
    trigger: On<AgentApprovalReply>,
    mut q: Query<(
        &mut AgentRunState,
        &mut AgentApprovalPolicy,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
    service: Option<Res<ServiceClient>>,
    mut store: Option<ResMut<AgentApprovalStore>>,
) {
    let reply = trigger.event();
    let Ok((mut state, mut policy, page, acp)) = q.get_mut(reply.session) else {
        return;
    };
    let Some(sid) = page
        .map(|s| s.sid.clone())
        .or_else(|| acp.map(|s| s.sid.clone()))
    else {
        return;
    };
    let matches_call = matches!(
        &*state,
        AgentRunState::AwaitingApproval { call_id, .. } if call_id == &reply.call_id
    );
    if !matches_call {
        return;
    }
    if reply.decision == ApprovalDecision::AllowAlways
        && let AgentRunState::AwaitingApproval { name, .. } = &*state
    {
        policy.allow(name);
        if let Some(acp) = acp
            && let Some(store) = store.as_deref_mut()
        {
            store.remember(&acp.agent_id, &acp.cwd, name);
        }
    }
    let decision = protocol_decision(reply.decision);
    if let Some(service) = service.as_ref() {
        service.0.send(ClientMessage::AgentApprove {
            sid,
            call_id: reply.call_id.clone(),
            decision,
        });
    }
    *state = AgentRunState::Streaming;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, AgentVariant};
    use serde_json::json;

    fn session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Page,
            sid: "s".into(),
            provider: "anthropic".into(),
            model: "m".into(),
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_observer(handle_approval_reply);
        app
    }

    #[test]
    fn deny_sets_streaming() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                session(),
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "run_shell".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::Deny,
        });
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming)
        ));
    }

    #[test]
    fn acp_session_reply_sets_streaming() {
        use crate::client::acp::AcpSession;
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "vibe-acp".into(),
                    sid: "s".into(),
                    cwd: std::path::PathBuf::from("/tmp"),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "edit".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::Allow,
        });
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Streaming)
        ));
    }

    #[test]
    fn allow_always_records_policy_and_preserves_decision_scope() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                session(),
                AgentApprovalPolicy::default(),
                AgentRunState::AwaitingApproval {
                    call_id: "abc".into(),
                    name: "run_shell".into(),
                    args: json!({}),
                },
            ))
            .id();
        app.world_mut().trigger(AgentApprovalReply {
            session: entity,
            call_id: "abc".into(),
            decision: ApprovalDecision::AllowAlways,
        });
        app.update();
        let policy = app.world().get::<AgentApprovalPolicy>(entity).unwrap();
        assert!(policy.allows("run_shell"));
        assert_eq!(
            protocol_decision(ApprovalDecision::AllowAlways),
            ProtoDecision::AllowAlways
        );
    }

    #[test]
    fn approval_grants_persist_by_agent_repository_and_tool() {
        let directory = tempfile::tempdir().unwrap();
        vmux_git::worktree::repository_init(directory.path()).unwrap();
        let path = directory.path().join("approvals.json");
        let mut store = AgentApprovalStore::load_from(path.clone());
        store.remember("codex-acp", directory.path(), "mcp__vmux__run");

        let loaded = AgentApprovalStore::load_from(path);

        assert!(
            loaded
                .policy_for("codex", directory.path())
                .allows("mcp.vmux.run")
        );
        assert!(
            !loaded
                .policy_for("claude", directory.path())
                .allows("mcp.vmux.run")
        );
        assert!(
            !loaded
                .policy_for("codex", directory.path())
                .allows("mcp.vmux.open_file")
        );
    }
}
