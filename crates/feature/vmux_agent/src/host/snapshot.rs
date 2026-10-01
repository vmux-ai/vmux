use bevy::prelude::*;
use vmux_command::snapshot::{
    AgentPromptTarget, AgentSummary, CommandBarProjection, CommandBarWorkDirectory,
};

use vmux_ecs::{ArchivedPage, LastActivatedAt};

use crate::host::acp::registry::RegistryAgent;
use crate::route::AcpRoute;

pub(super) struct SnapshotPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SnapshotSet {
    AgentSessions,
}

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::host::acp::AcpPackageChanged>()
            .add_systems(
                Update,
                (
                    update_agents.in_set(SnapshotSet::AgentSessions),
                    update_recent_agents,
                    sync_work_directories,
                )
                    .chain()
                    .in_set(vmux_command::snapshot::WriteCommandBarSnapshots),
            );
    }
}

fn sync_work_directories(
    changed: Query<(Entity, &vmux_ecs::AgentWorkingDir), Changed<vmux_ecs::AgentWorkingDir>>,
    mut removed: RemovedComponents<vmux_ecs::AgentWorkingDir>,
    mut commands: Commands,
) {
    for (entity, directory) in &changed {
        commands.entity(entity).insert(CommandBarWorkDirectory(
            directory.0.to_string_lossy().into_owned(),
        ));
    }
    for entity in removed.read() {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<CommandBarWorkDirectory>();
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_agents(
    catalog: Query<Ref<RegistryAgent>>,
    mut removed: RemovedComponents<RegistryAgent>,
    mut package_changes: MessageReader<crate::host::acp::AcpPackageChanged>,
    mut state: Single<&mut CommandBarProjection>,
) {
    let catalog_changed = catalog
        .iter()
        .any(|agent| agent.is_changed() || agent.is_added())
        || removed.read().next().is_some();
    let installs_changed = package_changes.read().count() > 0;
    if !catalog_changed && !installs_changed && !state.agents.acp.is_empty() {
        return;
    }

    let mut acp = Vec::new();
    for agent in &catalog {
        if !agent.is_installed() {
            continue;
        }
        acp.push(AgentSummary {
            id: agent.id.clone(),
            name: agent.name.clone(),
            url: format!("{}{}", vmux_chat::ChatPlugin::URL, agent.id),
            icon: agent.icon.clone().unwrap_or_default(),
        });
    }
    acp.sort_by_key(|agent| agent.name.to_lowercase());

    let next = vmux_command::snapshot::CommandBarAgentsSnapshot {
        acp,
        recent: state.agents.recent.clone(),
    };
    if state.agents != next {
        state.agents = next;
    }
}

fn update_recent_agents(
    acp_sessions: Query<(&vmux_session::AcpSession, Option<&LastActivatedAt>)>,
    archived_pages: Query<&ArchivedPage>,
    mut state: Single<&mut CommandBarProjection>,
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
            AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, &session.agent_id),
        );
    }
    for page in &archived_pages {
        let target = match AcpRoute::parse(&page.url) {
            Some(AcpRoute::Acp { id, .. }) => {
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, id)
            }
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
    if state.agents.recent != recent {
        state.agents.recent = recent;
    }
}

fn agent_prompt_target_sort_name(target: &AgentPromptTarget) -> String {
    target.id.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection(app: &App) -> &CommandBarProjection {
        app.world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarProjection>())
            .unwrap()
    }

    fn app() -> App {
        let mut app = App::new();
        app.world_mut().spawn(CommandBarProjection::default());
        app.add_plugins(SnapshotPlugin);
        app
    }

    #[test]
    fn writes_empty_snapshot_when_no_resources() {
        let mut app = app();
        app.update();
        let snap = &projection(&app).agents;
        assert!(snap.acp.is_empty());
    }

    #[test]
    fn recent_agents_are_deduped_and_sorted_by_last_use() {
        let mut app = app();
        app.world_mut().spawn((
            vmux_session::AcpSession {
                agent_id: "claude".to_string(),
                sid: "acp-session".to_string(),
                cwd: std::path::PathBuf::new(),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            },
            LastActivatedAt(30),
        ));
        app.world_mut().spawn((
            vmux_session::AcpSession {
                agent_id: "claude-acp".to_string(),
                sid: "older-acp-session".to_string(),
                cwd: std::path::PathBuf::new(),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            },
            LastActivatedAt(10),
        ));

        app.update();

        assert_eq!(
            projection(&app).agents.recent,
            vec![
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude"),
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude-acp")
            ]
        );

        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<vmux_session::AcpSession>>();
        let acp_sessions: Vec<_> = q.iter(app.world()).collect();
        for session in acp_sessions {
            app.world_mut().despawn(session);
        }
        app.update();

        assert_eq!(
            projection(&app).agents.recent,
            vec![
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude"),
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude-acp")
            ]
        );
    }

    #[test]
    fn closed_acp_agent_remains_recent() {
        let mut app = app();
        app.world_mut().spawn(ArchivedPage {
            url: "vmux://sessions/codex-acp/session-1".to_string(),
            closed_at: 30,
            ..default()
        });

        app.update();

        assert_eq!(
            projection(&app).agents.recent,
            vec![AgentPromptTarget::under(
                vmux_chat::ChatPlugin::URL,
                "codex-acp"
            )]
        );
    }

    #[test]
    fn equal_recent_agent_times_fall_back_to_name() {
        let mut app = app();
        app.world_mut().spawn((
            vmux_session::AcpSession {
                agent_id: "claude-acp".to_string(),
                sid: "acp-session".to_string(),
                cwd: std::path::PathBuf::new(),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            },
            LastActivatedAt(10),
        ));
        app.world_mut().spawn((
            vmux_session::AcpSession {
                agent_id: "codex-acp".to_string(),
                sid: "other-session".to_string(),
                cwd: std::path::PathBuf::new(),
                anchor: vmux_ecs::ProcessId::new(),
                resume: None,
            },
            LastActivatedAt(10),
        ));

        app.update();

        assert_eq!(
            projection(&app).agents.recent,
            vec![
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude-acp"),
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "codex-acp"),
            ]
        );
    }
}
