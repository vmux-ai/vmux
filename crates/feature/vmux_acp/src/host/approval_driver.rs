use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_session::ApprovalPolicy;

#[derive(Default, Deserialize, Serialize)]
struct SavedApprovalGrants {
    by_agent: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
}

#[derive(Component)]
pub(super) struct ApprovalDriver {
    path: PathBuf,
    grants: SavedApprovalGrants,
}

impl ApprovalDriver {
    pub(super) fn load() -> Self {
        Self::load_from(
            vmux_ecs::profile::ProfilePaths::current()
                .profile()
                .join("agent-approvals.json"),
        )
    }

    pub(super) fn load_from(path: PathBuf) -> Self {
        let grants = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, grants }
    }

    pub(super) fn policy_for(&self, agent: &str, cwd: &Path) -> ApprovalPolicy {
        let agent = Self::agent_id(agent);
        let auto = Self::scope(cwd)
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
        ApprovalPolicy { auto }
    }

    pub(super) fn remember(&mut self, agent: &str, cwd: &Path, tool: &str) {
        let Some(scope) = Self::scope(cwd) else {
            return;
        };
        let inserted = self
            .grants
            .by_agent
            .entry(Self::agent_id(agent))
            .or_default()
            .entry(scope)
            .or_default()
            .insert(ApprovalPolicy::tool_key(tool));
        if inserted && let Err(error) = self.save() {
            warn!("failed to save agent approvals: {error}");
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.grants).map_err(std::io::Error::other)?;
        vmux_path::AtomicFile::write(&self.path, &bytes)
    }

    fn scope(cwd: &Path) -> Option<String> {
        vmux_git::worktree::CheckoutInfo::try_from(cwd)
            .map(|checkout| checkout.common_dir)
            .ok()
            .or_else(|| std::fs::canonicalize(cwd).ok())
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn agent_id(agent: &str) -> String {
        agent.trim().to_ascii_lowercase()
    }
}
