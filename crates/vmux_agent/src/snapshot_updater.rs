use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentPromptTarget, AgentProviderSummary, AgentStrategySummary, CommandBarAgentsSnapshot,
};

use vmux_core::agent::AgentProviderTargetKind;
use vmux_core::{LastActivatedAt, Ready};

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
    settings: Option<Res<vmux_setting::AppSettings>>,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    install_generation: Option<Res<crate::client::acp::AcpInstallGeneration>>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
) {
    let providers_changed = !changed_q.is_empty();
    let idx_changed = page_idx
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    let settings_changed = settings
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
        && !settings_changed
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
    let configured = settings
        .as_ref()
        .map(|s| s.agent.acp.as_slice())
        .unwrap_or_default();
    snapshot.acp = acp_agent_summaries(configured, catalog_agents, |agent| {
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
    configured: &[vmux_setting::AcpAgentConfig],
    catalog: &[crate::acp_registry::RegistryAgent],
    is_installed: impl Fn(&crate::acp_registry::RegistryAgent) -> bool,
) -> Vec<AgentProviderSummary> {
    let mut seen_registry_ids = std::collections::HashSet::new();
    let mut agents = Vec::new();
    for cfg in configured {
        let registry_id = crate::acp_install::registry_id_alias(&cfg.id);
        let registry = catalog.iter().find(|agent| agent.id == registry_id);
        seen_registry_ids.insert(registry_id.to_string());
        agents.push(AgentProviderSummary {
            id: cfg.id.clone(),
            name: registry
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| cfg.name.clone()),
            url: format!("vmux://agent/{}", cfg.id),
            icon: registry
                .and_then(|agent| agent.icon.clone())
                .unwrap_or_default(),
        });
    }
    agents.extend(
        catalog
            .iter()
            .filter(|agent| !seen_registry_ids.contains(&agent.id))
            .filter(|agent| is_installed(agent))
            .map(|agent| AgentProviderSummary {
                id: agent.id.clone(),
                name: agent.name.clone(),
                url: format!("vmux://agent/{}", agent.id),
                icon: agent.icon.clone().unwrap_or_default(),
            }),
    );
    agents.sort_by_key(|agent| agent.name.to_lowercase());
    agents
}

pub(crate) fn update_last_active_agent(
    page_sessions: Query<(
        Entity,
        &crate::components::AgentSession,
        Option<&LastActivatedAt>,
    )>,
    acp_sessions: Query<(
        Entity,
        &crate::client::acp::AcpSession,
        Option<&LastActivatedAt>,
    )>,
    cli_sessions: Query<(Entity, &vmux_core::agent::AgentSession, &ChildOf)>,
    stack_times: Query<&LastActivatedAt>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
) {
    let mut newest: Option<(i64, u64, AgentPromptTarget)> = None;
    let mut consider = |entity: Entity, timestamp: i64, target: AgentPromptTarget| {
        let key = (timestamp, entity.to_bits());
        if newest
            .as_ref()
            .is_none_or(|(current_timestamp, current_entity, _)| {
                key > (*current_timestamp, *current_entity)
            })
        {
            newest = Some((key.0, key.1, target));
        }
    };

    for (entity, session, timestamp) in &page_sessions {
        consider(
            entity,
            timestamp.map(|timestamp| timestamp.0).unwrap_or(i64::MIN),
            AgentPromptTarget::Page {
                provider: session.provider.clone(),
                model: session.model.clone(),
            },
        );
    }
    for (entity, session, timestamp) in &acp_sessions {
        consider(
            entity,
            timestamp.map(|timestamp| timestamp.0).unwrap_or(i64::MIN),
            AgentPromptTarget::Acp {
                id: session.agent_id.clone(),
            },
        );
    }
    for (entity, session, child_of) in &cli_sessions {
        consider(
            entity,
            stack_times
                .get(child_of.parent())
                .map(|timestamp| timestamp.0)
                .unwrap_or(i64::MIN),
            AgentPromptTarget::Cli(session.kind),
        );
    }

    if let Some((_, _, target)) = newest {
        snapshot.last_active = Some(target);
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
mod tests {
    use super::*;

    fn registry_agent(id: &str, name: &str) -> crate::acp_registry::RegistryAgent {
        crate::acp_registry::RegistryAgent {
            id: id.to_string(),
            name: name.to_string(),
            version: None,
            description: None,
            icon: None,
            repository: None,
            distribution: crate::acp_registry::Distribution::default(),
        }
    }

    fn configured_agent(id: &str, name: &str) -> vmux_setting::AcpAgentConfig {
        vmux_setting::AcpAgentConfig {
            id: id.to_string(),
            name: name.to_string(),
            command: "agent".to_string(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        }
    }

    #[test]
    fn writes_empty_snapshot_when_no_resources() {
        let mut app = App::new();
        app.init_resource::<CommandBarAgentsSnapshot>()
            .add_systems(Update, update_agents_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarAgentsSnapshot>();
        assert!(snap.providers.is_empty());
        assert!(snap.strategies.is_empty());
    }

    #[test]
    fn agent_sessions_snapshot_starts_empty() {
        let mut app = App::new();
        app.init_resource::<CommandBarTerminalsSnapshot>()
            .add_systems(Update, update_agent_sessions_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarTerminalsSnapshot>();
        assert!(snap.agent_session_to_entity.is_empty());
    }

    #[test]
    fn installed_unconfigured_acp_is_in_snapshot() {
        let catalog = vec![registry_agent("new-agent", "New Agent")];

        let agents = acp_agent_summaries(&[], &catalog, |_| true);

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "new-agent");
        assert_eq!(agents[0].url, "vmux://agent/new-agent");
    }

    #[test]
    fn configured_acp_alias_is_not_duplicated() {
        let configured = vec![configured_agent("claude", "Claude Code")];
        let catalog = vec![registry_agent("claude-acp", "Claude Agent")];

        let agents = acp_agent_summaries(&configured, &catalog, |_| true);

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "claude");
        assert_eq!(agents[0].name, "Claude Agent");
    }

    #[test]
    fn last_active_agent_tracks_most_recent_session() {
        let mut app = App::new();
        app.init_resource::<CommandBarAgentsSnapshot>()
            .add_systems(Update, update_last_active_agent);
        let page = app
            .world_mut()
            .spawn((
                crate::components::AgentSession {
                    kind: vmux_core::agent::AgentKind::Vibe,
                    variant: crate::AgentVariant::Page,
                    sid: "page-session".to_string(),
                    provider: "openai".to_string(),
                    model: "gpt-5".to_string(),
                },
                LastActivatedAt(10),
            ))
            .id();
        let cli_stack = app.world_mut().spawn(LastActivatedAt(20)).id();
        app.world_mut().spawn((
            vmux_core::agent::AgentSession {
                kind: vmux_core::agent::AgentKind::Codex,
            },
            ChildOf(cli_stack),
        ));

        app.update();

        assert_eq!(
            app.world()
                .resource::<CommandBarAgentsSnapshot>()
                .last_active,
            Some(AgentPromptTarget::Cli(vmux_core::agent::AgentKind::Codex))
        );

        app.world_mut().entity_mut(page).insert(LastActivatedAt(30));
        app.update();

        assert_eq!(
            app.world()
                .resource::<CommandBarAgentsSnapshot>()
                .last_active,
            Some(AgentPromptTarget::Page {
                provider: "openai".to_string(),
                model: "gpt-5".to_string(),
            })
        );
    }
}
