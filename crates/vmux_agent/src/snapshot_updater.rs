use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentPromptTarget, AgentProviderSummary, AgentStrategySummary, CommandBarAgentsSnapshot,
};

use vmux_core::agent::AgentProviderTargetKind;
use vmux_core::{ArchivedPage, LastActivatedAt, Ready};

use crate::client::page::strategy_index::PageStrategyIndex;

#[allow(clippy::type_complexity)]
pub(crate) fn update_agents_snapshot(
    providers_q: Query<(&AgentProviderTargetKind, &Name), With<Ready>>,
    changed_q: Query<
        Entity,
        (
            With<AgentProviderTargetKind>,
            Or<(Added<Ready>, Added<AgentProviderTargetKind>)>,
        ),
    >,
    page_idx: Option<Res<PageStrategyIndex>>,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    install_generation: Option<Res<crate::client::acp::AcpInstallGeneration>>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
) {
    let providers_changed = !changed_q.is_empty();
    let idx_changed = page_idx
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    let catalog_changed = catalog
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    let installs_changed = install_generation
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !providers_changed
        && !idx_changed
        && !catalog_changed
        && !installs_changed
        && (!snapshot.providers.is_empty()
            || !snapshot.strategies.is_empty()
            || !snapshot.acp.is_empty())
    {
        return;
    }

    let catalog_agents = catalog
        .as_ref()
        .map(|c| c.agents.as_slice())
        .unwrap_or_default();
    snapshot.acp = acp_agent_summaries(catalog_agents, |agent| {
        crate::acp_install::is_agent_installed(agent)
    });

    let mut providers: Vec<AgentProviderSummary> = providers_q
        .iter()
        .map(|(kind, name)| AgentProviderSummary {
            id: kind.0.as_url_segment().to_string(),
            name: name.as_str().to_string(),
            // CLI agents live at `<kind>/cli` under the new grammar (bare `<kind>` is ACP).
            url: format!("{}cli", kind.0.cli_url_prefix()),
            icon: String::new(),
        })
        .collect();
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot.providers = providers;

    snapshot.strategies = page_idx
        .as_ref()
        .map(|idx| {
            idx.keys()
                .map(|key| AgentStrategySummary {
                    provider: key.provider.clone(),
                    model: key.model.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
}

fn acp_agent_summaries(
    catalog: &[crate::acp_registry::RegistryAgent],
    is_installed: impl Fn(&crate::acp_registry::RegistryAgent) -> bool,
) -> Vec<AgentProviderSummary> {
    let mut agents: Vec<AgentProviderSummary> = catalog
        .iter()
        .filter(|agent| is_installed(agent))
        .map(|agent| AgentProviderSummary {
            id: agent.id.clone(),
            name: agent.name.clone(),
            url: format!(
                "vmux://agent/{}",
                crate::acp_install::agent_url_id(&agent.id)
            ),
            icon: agent.icon.clone().unwrap_or_default(),
        })
        .collect();
    agents.sort_by_key(|agent| agent.name.to_lowercase());
    agents
}

pub(crate) fn update_recent_agents(
    acp_sessions: Query<(&crate::client::acp::AcpSession, Option<&LastActivatedAt>)>,
    cli_sessions: Query<(&vmux_core::agent::AgentSession, &ChildOf)>,
    stack_times: Query<&LastActivatedAt>,
    archived_pages: Query<&ArchivedPage>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
    mut remembered: Local<std::collections::HashMap<AgentPromptTarget, i64>>,
) {
    let mut consider = |timestamp: i64, target: AgentPromptTarget| {
        if remembered
            .get(&target)
            .is_none_or(|current| timestamp > *current)
        {
            remembered.insert(target, timestamp);
        }
    };

    for (session, timestamp) in &acp_sessions {
        consider(
            timestamp.map(|timestamp| timestamp.0).unwrap_or(i64::MIN),
            AgentPromptTarget::Acp {
                id: crate::acp_install::agent_url_id(crate::acp_install::registry_id_alias(
                    &session.agent_id,
                ))
                .to_string(),
            },
        );
    }
    for (session, child_of) in &cli_sessions {
        consider(
            stack_times
                .get(child_of.parent())
                .map(|timestamp| timestamp.0)
                .unwrap_or(i64::MIN),
            AgentPromptTarget::Cli(session.kind),
        );
    }
    for page in &archived_pages {
        let target = match crate::url::AgentUrl::parse(&page.url) {
            Some(crate::url::AgentUrl::Cli { kind, .. }) => AgentPromptTarget::Cli(kind),
            Some(crate::url::AgentUrl::Acp { id, .. }) => AgentPromptTarget::Acp {
                id: crate::acp_install::agent_url_id(crate::acp_install::registry_id_alias(&id))
                    .to_string(),
            },
            _ => continue,
        };
        consider(page.closed_at, target);
    }

    let mut recent: Vec<_> = remembered.iter().collect();
    recent.sort_by_cached_key(|(target, timestamp)| {
        (
            std::cmp::Reverse(**timestamp),
            agent_prompt_target_sort_name(target),
        )
    });
    let recent = recent
        .into_iter()
        .map(|(target, _)| target.clone())
        .collect();
    if snapshot.recent != recent {
        snapshot.recent = recent;
    }
}

fn agent_prompt_target_sort_name(target: &AgentPromptTarget) -> String {
    match target {
        AgentPromptTarget::Cli(kind) => kind.display_name().to_lowercase(),
        AgentPromptTarget::Acp { id } => id.to_lowercase(),
    }
}

use crate::session::AgentSessionToEntity;
use vmux_command::snapshot::CommandBarTerminalsSnapshot;

pub fn update_agent_sessions_snapshot(
    sessions: Option<Res<AgentSessionToEntity>>,
    mut snapshot: ResMut<CommandBarTerminalsSnapshot>,
) {
    let changed = sessions
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !changed && !snapshot.agent_session_to_entity.is_empty() {
        return;
    }
    snapshot.agent_session_to_entity = sessions.as_deref().map(|m| m.0.clone()).unwrap_or_default();
}

#[cfg(test)]
#[path = "snapshot_updater.test.rs"]
mod tests;
