use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::AgentKind;
use crate::AgentVariant;
use crate::client::cli::strategy::{CliAgentStrategy, ResumableSession};
use crate::message::Message;

pub trait AgentStrategy: Send + Sync + 'static {
    fn kind(&self) -> AgentKind;
    fn variant(&self) -> AgentVariant;
}

#[derive(Resource, Default, Clone)]
pub struct AgentStrategies {
    cli: HashMap<AgentKind, Arc<dyn CliAgentStrategy>>,
}

impl AgentStrategies {
    pub fn register_cli(&mut self, strategy: Box<dyn CliAgentStrategy>) {
        self.cli.insert(strategy.kind(), strategy.into());
    }

    pub fn get_cli(&self, kind: AgentKind) -> Option<&dyn CliAgentStrategy> {
        self.cli.get(&kind).map(Arc::as_ref)
    }

    pub fn cli_strategies(&self) -> impl Iterator<Item = &dyn CliAgentStrategy> {
        self.cli.values().map(Arc::as_ref)
    }

    /// All resumable sessions across every registered CLI strategy, newest-first, deduped.
    pub fn list_all_sessions(&self) -> Vec<ResumableSession> {
        let all = self
            .cli_strategies()
            .flat_map(|s| s.list_sessions())
            .collect();
        sort_sessions(all)
    }

    pub fn load_transcript(&self, kind: AgentKind, sid: &str) -> Result<Vec<Message>, String> {
        self.get_cli(kind)
            .ok_or_else(|| format!("no session strategy registered for {}", kind.display_name()))?
            .load_transcript(sid)
    }
}

/// Whether a kind's ACP and CLI runtimes share the same session id (so a session can be
/// handed off between them). Single source of truth for the `cross_runtime` flag.
pub fn kind_supports_cross_runtime(kind: AgentKind) -> bool {
    matches!(kind, AgentKind::Vibe | AgentKind::Claude | AgentKind::Codex)
}

/// Maps a built-in launcher id or its ACP registry id to the shared agent kind.
pub(crate) fn acp_agent_kind(agent_id: &str) -> Option<AgentKind> {
    AgentKind::all().into_iter().find(|kind| {
        let segment = kind.as_url_segment();
        agent_id == segment || agent_id == crate::acp_install::registry_id_alias(segment)
    })
}

/// Sort newest-first and drop duplicate `(kind, sid)` keeping the newest.
pub fn sort_sessions(mut sessions: Vec<ResumableSession>) -> Vec<ResumableSession> {
    sessions.sort_by_key(|s| std::cmp::Reverse(s.mtime));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|s| seen.insert((s.kind, s.sid.clone())));
    sessions
}

#[cfg(test)]
#[path = "strategy.test.rs"]
mod tests;
