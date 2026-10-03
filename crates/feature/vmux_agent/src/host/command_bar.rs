use std::cmp::Reverse;

use bevy::prelude::*;
use vmux_api::command_bar::CommandBarPage;
#[cfg(test)]
use vmux_command::ClaimedUrls;
use vmux_command::{
    ClaimedUrl, CommandBarWorkDirectory, ContributedPage, WriteCommandBarSnapshots,
};
use vmux_ecs::{Cwd, EntityTarget, LastActivatedAt};
#[cfg(test)]
use vmux_session::SessionId;
use vmux_session::{AgentId, Session};

use super::acp::AcpPackageChanged;
use super::acp::registry::RegistryAgent;
use crate::route::SessionRoute;

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
    sessions: Query<Ref<Cwd>, With<Session>>,
    stacks: Query<(
        Entity,
        Ref<EntityTarget<Session>>,
        Option<&CommandBarWorkDirectory>,
    )>,
    mut commands: Commands,
) {
    for (stack, target, current) in &stacks {
        let Ok(cwd) = sessions.get(target.entity()) else {
            if current.is_some() {
                commands.entity(stack).remove::<CommandBarWorkDirectory>();
            }
            continue;
        };
        if !target.is_added() && !cwd.is_changed() {
            continue;
        }
        let next = CommandBarWorkDirectory(cwd.0.to_string_lossy().into_owned());
        if current != Some(&next) {
            commands.entity(stack).insert(next);
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
    sessions: Query<(&AgentId, Option<&LastActivatedAt>), With<Session>>,
    mut commands: Commands,
) {
    let mut recent = Vec::<(String, i64)>::new();
    for (agent_id, timestamp) in &sessions {
        let timestamp = timestamp.map_or(i64::MIN, |timestamp| timestamp.0);
        if let Some((_, current)) = recent
            .iter_mut()
            .find(|(candidate, _)| candidate == &agent_id.0)
        {
            *current = (*current).max(timestamp);
        } else {
            recent.push((agent_id.0.clone(), timestamp));
        }
    }
    recent.sort_by(|(left_agent, left_time), (right_agent, right_time)| {
        Reverse(*left_time)
            .cmp(&Reverse(*right_time))
            .then_with(|| left_agent.cmp(right_agent))
    });

    let mut pages = Vec::new();
    for (entity, agent, _) in &agents {
        pages.push((
            entity,
            ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    url: SessionRoute::manager_for_agent(&AgentId(agent.id.clone())),
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
            .position(|(agent_id, _)| agent_id == &left.id)
            .unwrap_or(usize::MAX);
        let right_rank = recent
            .iter()
            .position(|(agent_id, _)| agent_id == &right.id)
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
            Session,
            SessionId("session".into()),
            AgentId("codex".into()),
            Cwd::default(),
            vmux_ecs::ProcessAnchor(ProcessId::new()),
            LastActivatedAt(20),
        ));

        app.update();

        let mut query = app.world_mut().query::<&ContributedPage>();
        let mut pages = query.iter(app.world()).cloned().collect::<Vec<_>>();
        pages.sort_by_key(|page| page.rank);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "codex");
        assert_eq!(pages[1].id, "claude-acp");
        assert_eq!(pages[0].page.url, "vmux://sessions/?agent=codex");
        assert_eq!(pages[1].page.url, "vmux://sessions/?agent=claude-acp");
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
            [true, true, false, false, false, false, false, false]
        );
    }
}
