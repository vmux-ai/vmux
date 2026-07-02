use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentProviderSummary, AgentStrategySummary, CommandBarAgentsSnapshot,
};

use vmux_core::Ready;
use vmux_core::agent::AgentProviderTargetKind;

use crate::client::page::strategy_index::PageStrategyIndex;

#[allow(clippy::type_complexity)]
pub fn update_agents_snapshot(
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
    if !providers_changed
        && !idx_changed
        && !settings_changed
        && !catalog_changed
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
    snapshot.acp = settings
        .as_ref()
        .map(|s| {
            s.agent
                .acp
                .iter()
                .map(|cfg| {
                    let reg_id = crate::acp_install::registry_id_alias(&cfg.id);
                    let reg = catalog_agents.iter().find(|a| a.id == reg_id);
                    AgentProviderSummary {
                        id: cfg.id.clone(),
                        name: reg
                            .map(|a| a.name.clone())
                            .unwrap_or_else(|| cfg.name.clone()),
                        url: format!("vmux://agent/{}", cfg.id),
                        icon: reg.and_then(|a| a.icon.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let mut providers: Vec<AgentProviderSummary> = providers_q
        .iter()
        .map(|(kind, name)| AgentProviderSummary {
            id: kind.0.as_url_segment().to_string(),
            name: name.as_str().to_string(),
            url: kind.0.cli_url_prefix(),
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
}
