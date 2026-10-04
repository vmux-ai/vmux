use std::cmp::Reverse;

use bevy::prelude::*;
use vmux_api::command_bar::CommandBarPage;
#[cfg(test)]
use vmux_command::ClaimedUrls;
use vmux_command::{
    ClaimedUrl, CommandBarWorkDirectory, ContributedPage, WriteCommandBarSnapshots,
};
use vmux_ecs::{AgentWorkingDir, ArchivedPage, LastActivatedAt};
use vmux_session::AcpSession;

use super::acp::AcpPackageChanged;
use super::acp::registry::RegistryAgent;
use crate::route::AcpRoute;

pub(crate) struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AcpPackageChanged>()
            .add_systems(Startup, claim)
            .add_systems(
                Update,
                (
                    sync_work_directories,
                    sync_installed,
                    ApplyDeferred,
                    publish,
                )
                    .chain()
                    .in_set(WriteCommandBarSnapshots),
            );
    }
}

#[derive(Component)]
struct InstalledAgent;

fn claim(mut commands: Commands) {
    commands.spawn((
        Name::new("Agent command-bar contribution"),
        ClaimedUrl(vmux_chat::ChatPlugin::URL.to_string()),
    ));
}

fn sync_work_directories(
    changed: Query<(Entity, &AgentWorkingDir), Changed<AgentWorkingDir>>,
    mut removed: RemovedComponents<AgentWorkingDir>,
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

fn sync_installed(
    agents: Query<(Entity, Ref<RegistryAgent>, Has<InstalledAgent>)>,
    mut packages: MessageReader<AcpPackageChanged>,
    mut commands: Commands,
) {
    let refresh = packages.read().count() > 0;
    for (entity, agent, installed) in &agents {
        if !refresh && !agent.is_added() && !agent.is_changed() {
            continue;
        }
        match (agent.is_installed(), installed) {
            (true, false) => {
                commands.entity(entity).insert(InstalledAgent);
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<(InstalledAgent, ContributedPage)>();
            }
            _ => {}
        }
    }
}

fn publish(
    agents: Query<(Entity, &RegistryAgent, Option<&ContributedPage>), With<InstalledAgent>>,
    sessions: Query<(&AcpSession, Option<&LastActivatedAt>)>,
    archived: Query<&ArchivedPage>,
    mut commands: Commands,
) {
    let mut recent = Vec::<(String, i64)>::new();
    for (session, timestamp) in &sessions {
        let url = AcpRoute::agent(&session.agent_id).url();
        let timestamp = timestamp.map_or(i64::MIN, |timestamp| timestamp.0);
        if let Some((_, current)) = recent.iter_mut().find(|(candidate, _)| *candidate == url) {
            *current = (*current).max(timestamp);
        } else {
            recent.push((url, timestamp));
        }
    }
    for page in &archived {
        let Some(AcpRoute::Acp { id, .. }) = AcpRoute::parse(&page.url) else {
            continue;
        };
        let url = AcpRoute::agent(id).url();
        if let Some((_, current)) = recent.iter_mut().find(|(candidate, _)| *candidate == url) {
            *current = (*current).max(page.closed_at);
        } else {
            recent.push((url, page.closed_at));
        }
    }
    recent.sort_by(|(left_url, left_time), (right_url, right_time)| {
        Reverse(*left_time)
            .cmp(&Reverse(*right_time))
            .then_with(|| left_url.cmp(right_url))
    });

    let mut pages = Vec::new();
    for (entity, agent, _) in &agents {
        pages.push((
            entity,
            ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    url: AcpRoute::agent(&agent.id).url(),
                    title: agent.name.clone(),
                    keywords: vec![agent.id.clone(), "acp".to_string(), "agent".to_string()],
                    icon: agent
                        .icon
                        .as_ref()
                        .map(|icon| vmux_ecs::PageIcon::Favicon(icon.clone()))
                        .unwrap_or_default(),
                    shortcut: String::new(),
                    prompt_target: true,
                    startup: false,
                },
            },
        ));
    }
    pages.sort_by(|(_, left), (_, right)| {
        let left_rank = recent
            .iter()
            .position(|(url, _)| url == &left.page.url)
            .unwrap_or(usize::MAX);
        let right_rank = recent
            .iter()
            .position(|(url, _)| url == &right.page.url)
            .unwrap_or(usize::MAX);
        left_rank.cmp(&right_rank).then_with(|| {
            left.page
                .title
                .to_lowercase()
                .cmp(&right.page.title.to_lowercase())
        })
    });
    for (rank, (entity, mut page)) in pages.into_iter().enumerate() {
        page.rank = rank;
        let Ok((_, _, current)) = agents.get(entity) else {
            continue;
        };
        if current != Some(&page) {
            commands.entity(entity).insert(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use vmux_command::ContributedPage;
    use vmux_ecs::ProcessId;

    use super::*;
    use crate::host::acp::registry::Distribution;

    impl RegistryAgent {
        fn test(id: &str, name: &str, icon: Option<&str>) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
                version: None,
                description: None,
                icon: icon.map(str::to_string),
                distribution: Distribution::default(),
            }
        }
    }

    #[test]
    fn installed_agents_contribute_pages_in_recent_order() {
        let mut app = App::new();
        app.add_systems(Update, publish);
        app.world_mut().spawn((
            RegistryAgent::test(
                "claude-acp",
                "Claude Agent",
                Some("https://cdn.example/claude-acp.svg"),
            ),
            InstalledAgent,
        ));
        app.world_mut()
            .spawn((RegistryAgent::test("codex", "Codex", None), InstalledAgent));
        app.world_mut().spawn((
            AcpSession {
                agent_id: "codex".to_string(),
                sid: "session".to_string(),
                cwd: std::path::PathBuf::new(),
                anchor: ProcessId::new(),
                resume: None,
            },
            LastActivatedAt(20),
        ));

        app.update();

        let mut query = app.world_mut().query::<&ContributedPage>();
        let mut pages = query.iter(app.world()).cloned().collect::<Vec<_>>();
        pages.sort_by_key(|page| page.rank);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "codex");
        assert_eq!(pages[1].id, "claude-acp");
        assert!(matches!(
            pages[1].page.icon,
            vmux_ecs::PageIcon::Favicon(ref icon)
                if icon == "https://cdn.example/claude-acp.svg"
        ));
    }

    #[test]
    fn only_bare_agent_urls_are_claimed() {
        let mut world = World::new();
        world.spawn(ClaimedUrl(vmux_chat::ChatPlugin::URL.to_string()));

        let claimed = world
            .run_system_once(|claimed: ClaimedUrls| {
                [
                    claimed.contains("vmux://sessions/"),
                    claimed.contains("vmux://sessions"),
                    claimed.contains("vmux://agent/"),
                    claimed.contains("vmux://agent"),
                    claimed.contains("vmux://sessions/codex"),
                    claimed.contains("vmux://sessions/codex/cli"),
                    claimed.contains("vmux://agent/codex"),
                    claimed.contains("vmux://agent/codex/cli"),
                ]
            })
            .expect("claims_url system runs");

        assert_eq!(
            claimed,
            [true, true, true, true, false, false, false, false]
        );
    }
}
