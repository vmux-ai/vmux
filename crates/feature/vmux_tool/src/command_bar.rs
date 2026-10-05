use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::mcp::{McpServerEntry, McpServerRequest, McpServerStatus, McpServersUiState};
use vmux_ecs::UiStateWrite;
use vmux_ecs::{CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged};

use crate::connection::McpSnapshotRequest;

pub(crate) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_observer(query)
            .add_observer(receive)
            .add_observer(activate);
    }
}

#[derive(Component)]
struct McpContribution(String);

#[derive(Component)]
struct McpCommandBar(String);

fn query(trigger: On<CommandBarQueryChanged>, mut results: McpResults) {
    let event = trigger.event();
    results.clear(event.target);
    let Some(filter) = McpQuery::parse(&event.query) else {
        results
            .commands
            .entity(event.target)
            .remove::<McpCommandBar>();
        return;
    };
    results
        .commands
        .entity(event.target)
        .insert(McpCommandBar(filter.to_string()));
    results.commands.trigger(McpSnapshotRequest {
        target: event.target,
    });
}

fn receive(
    trigger: On<UiStateWrite<McpServersUiState>>,
    active: Query<&McpCommandBar>,
    mut results: McpResults,
) {
    let target = trigger.event().webview();
    let Ok(active) = active.get(target) else {
        return;
    };
    results.clear(target);
    let state = trigger.event().update();
    for (rank, server) in state
        .servers
        .iter()
        .filter(|server| McpServerRow(server).matches(&active.0))
        .enumerate()
    {
        results.commands.spawn((
            Name::new(format!("MCP command-bar row: {}", server.id)),
            CommandBarContribution {
                row: McpServerRow(server).project(state),
                rank: rank as i32,
                close: false,
                pre_filtered: true,
                ..Default::default()
            },
            McpContribution(server.id.clone()),
            ChildOf(target),
        ));
    }
}

fn activate(
    trigger: On<CommandBarContributionActivated>,
    contributions: Query<&McpContribution>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(contribution) = contributions.get(event.target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: event.webview,
        payload: McpServerRequest {
            id: contribution.0.clone(),
        },
    });
}

#[derive(SystemParam)]
struct McpResults<'w, 's> {
    existing: Query<'w, 's, (Entity, &'static ChildOf), With<McpContribution>>,
    commands: Commands<'w, 's>,
}

impl McpResults<'_, '_> {
    fn clear(&mut self, target: Entity) {
        for (entity, parent) in &self.existing {
            if parent.parent() == target {
                self.commands.entity(entity).despawn();
            }
        }
    }
}

struct McpQuery;

impl McpQuery {
    fn parse(query: &str) -> Option<&str> {
        let rest = query.trim().strip_prefix("/mcp")?;
        if rest.is_empty() {
            return Some("");
        }
        rest.chars()
            .next()?
            .is_whitespace()
            .then(|| rest.trim_start())
    }
}

struct McpServerRow<'a>(&'a McpServerEntry);

impl McpServerRow<'_> {
    fn matches(&self, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        filter.is_empty()
            || self.0.id.to_ascii_lowercase().contains(&filter)
            || self.0.name.to_ascii_lowercase().contains(&filter)
            || self.0.description.to_ascii_lowercase().contains(&filter)
    }

    fn project(&self, state: &McpServersUiState) -> vmux_api::command_bar::CommandBarResultItem {
        let server = self.0;
        vmux_api::command_bar::CommandBarResultItem {
            key: server.id.clone(),
            title: server.name.clone(),
            subtitle: server.description.clone(),
            trailing: "\u{21b5}".to_string(),
            active: server.status == McpServerStatus::Connected,
            disabled: state.loading || state.pending.is_some(),
            ..Default::default()
        }
    }
}
