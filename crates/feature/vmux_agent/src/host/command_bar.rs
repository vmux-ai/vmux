use bevy::prelude::*;
use vmux_api::command_bar::CommandBarPage;
#[cfg(test)]
use vmux_command::snapshot::ClaimedUrls;
use vmux_command::snapshot::{
    AgentPromptTarget, ClaimedUrl, CommandBarAgentsSnapshot, CommandBarProjection, ContributedPage,
    WriteCommandBarSnapshots,
};

pub(crate) struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            publish_contributions
                .in_set(WriteCommandBarSnapshots)
                .after(crate::host::snapshot::SnapshotSet::AgentSessions),
        );
    }
}

#[derive(Component)]
struct AgentContribution;

impl AgentContribution {
    fn launcher_pages(agents: &CommandBarAgentsSnapshot) -> Vec<ContributedPage> {
        let mut pages = Vec::with_capacity(agents.acp.len());
        for agent in &agents.acp {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    url: agent.url.clone(),
                    title: agent.name.clone(),
                    keywords: vec![agent.id.clone(), "acp".to_string(), "agent".to_string()],
                    icon: if agent.icon.is_empty() {
                        vmux_ecs::PageIcon::None
                    } else {
                        vmux_ecs::PageIcon::Favicon(agent.icon.clone())
                    },
                    shortcut: String::new(),
                    prompt_target: true,
                    startup: false,
                },
            });
        }
        let recency = AgentPromptTarget::recency_ranks(&agents.recent);
        pages.sort_by(|left, right| {
            recency
                .get(&left.page.url)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&recency.get(&right.page.url).copied().unwrap_or(usize::MAX))
                .then_with(|| {
                    left.page
                        .title
                        .to_lowercase()
                        .cmp(&right.page.title.to_lowercase())
                })
        });
        for (rank, page) in pages.iter_mut().enumerate() {
            page.rank = rank;
        }
        pages
    }
}

fn publish_contributions(
    state: Single<&CommandBarProjection>,
    mut previous: Local<Option<CommandBarAgentsSnapshot>>,
    mine: Query<Entity, With<AgentContribution>>,
    mut commands: Commands,
) {
    if previous.as_ref() == Some(&state.agents) {
        return;
    }
    let agents = &state.agents;
    *previous = Some(agents.clone());
    for entity in mine.iter() {
        commands.entity(entity).despawn();
    }
    for page in AgentContribution::launcher_pages(agents) {
        commands.spawn((AgentContribution, page));
    }
    commands.spawn((
        AgentContribution,
        ClaimedUrl(vmux_chat::ChatPlugin::URL.to_string()),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_command::snapshot::AgentSummary;

    #[test]
    fn launcher_pages_list_only_installed_agents_in_recent_order() {
        let snapshot = CommandBarAgentsSnapshot {
            acp: vec![
                AgentSummary {
                    id: "claude-acp".to_string(),
                    name: "Claude Agent".to_string(),
                    url: "vmux://sessions/claude".to_string(),
                    icon: "https://cdn.example/claude-acp.svg".to_string(),
                },
                AgentSummary {
                    id: "codex".to_string(),
                    name: "Codex".to_string(),
                    url: "vmux://sessions/codex".to_string(),
                    icon: String::new(),
                },
            ],
            recent: vec![
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "codex"),
                AgentPromptTarget::under(vmux_chat::ChatPlugin::URL, "claude"),
            ],
            ..Default::default()
        };

        let pages = AgentContribution::launcher_pages(&snapshot);

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "codex");
        assert_eq!(pages[0].rank, 0);
        assert_eq!(pages[0].page.url, "vmux://sessions/codex");
        assert_eq!(pages[0].page.title, "Codex");
        assert_eq!(pages[1].rank, 1);
        assert_eq!(pages[1].page.title, "Claude Agent");
        assert!(matches!(
            pages[1].page.icon,
            vmux_ecs::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
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
