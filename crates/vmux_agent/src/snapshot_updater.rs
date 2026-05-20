use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentProviderSummary, AgentStrategySummary, CommandBarAgentsSnapshot,
};

use crate::plugin::AgentProviders;
use crate::strategy::AgentStrategies;

pub fn update_agents_snapshot(
    providers: Option<Res<AgentProviders>>,
    strategies: Option<Res<AgentStrategies>>,
    mut snapshot: ResMut<CommandBarAgentsSnapshot>,
) {
    let providers_changed = providers
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    let strategies_changed = strategies
        .as_ref()
        .map(|r| r.is_changed() || r.is_added())
        .unwrap_or(false);
    if !providers_changed
        && !strategies_changed
        && (!snapshot.providers.is_empty() || !snapshot.strategies.is_empty())
    {
        return;
    }

    snapshot.providers = providers
        .as_ref()
        .map(|p| {
            p.command_entries()
                .into_iter()
                .map(|e| AgentProviderSummary {
                    id: e.id.to_string(),
                    name: e.name.to_string(),
                    shortcut: e.shortcut.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    snapshot.strategies = strategies
        .as_ref()
        .map(|s| {
            s.page_strategies()
                .map(|st| AgentStrategySummary {
                    provider: st.provider().to_string(),
                    model: st.model().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_empty_snapshot_when_no_resources() {
        let mut app = App::new();
        app.init_resource::<CommandBarAgentsSnapshot>();
        app.add_systems(Update, update_agents_snapshot);
        app.update();
        let snap = app.world().resource::<CommandBarAgentsSnapshot>();
        assert!(snap.providers.is_empty());
        assert!(snap.strategies.is_empty());
    }
}
