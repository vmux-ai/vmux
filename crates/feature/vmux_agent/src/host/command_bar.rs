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
                .after(crate::snapshot::SnapshotSet::AgentSessions),
        );
    }
}

const DEFAULT_AGENT_URLS: [&str; 4] = [
    "vmux://sessions/",
    "vmux://sessions",
    "vmux://agent/",
    "vmux://agent",
];

#[derive(Component)]
struct AgentContribution;

impl AgentContribution {
    fn launcher_pages(agents: &CommandBarAgentsSnapshot) -> Vec<ContributedPage> {
        let mut pages = Vec::with_capacity(agents.acp.len() + agents.cli.len());
        for agent in &agents.acp {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    url: agent.url.clone(),
                    title: agent.name.clone(),
                    keywords: vec![agent.id.clone(), "acp".to_string(), "agent".to_string()],
                    icon: if agent.icon.is_empty() {
                        vmux_core::PageIcon::None
                    } else {
                        vmux_core::PageIcon::Favicon(agent.icon.clone())
                    },
                    shortcut: String::new(),
                    prompt_target: true,
                },
            });
        }
        for agent in &agents.cli {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    url: agent.url.clone(),
                    title: format!("{} (CLI)", agent.name),
                    keywords: vec![agent.id.clone(), "cli".to_string(), "agent".to_string()],
                    icon: vmux_core::PageIcon::None,
                    shortcut: String::new(),
                    prompt_target: true,
                },
            });
        }
        let recency = AgentPromptTarget::recency_ranks(&agents.recent);
        pages.sort_by(|a, b| {
            recency
                .get(&a.page.url)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&recency.get(&b.page.url).copied().unwrap_or(usize::MAX))
                .then_with(|| {
                    a.page
                        .title
                        .to_lowercase()
                        .cmp(&b.page.title.to_lowercase())
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
    for url in DEFAULT_AGENT_URLS {
        commands.spawn((AgentContribution, ClaimedUrl(url.to_string())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_command::snapshot::AgentSummary;
    use vmux_core::agent::AgentKind;

    #[test]
    fn launcher_pages_list_only_installed_agents_in_recent_order() {
        let snapshot = CommandBarAgentsSnapshot {
            cli: vec![AgentSummary {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                url: "vmux://sessions/codex/cli".to_string(),
                icon: String::new(),
            }],
            acp: vec![AgentSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://sessions/claude".to_string(),
                icon: "https://cdn.example/claude-acp.svg".to_string(),
            }],
            recent: vec![
                AgentPromptTarget::Cli(AgentKind::Codex),
                AgentPromptTarget::Acp {
                    id: "claude".to_string(),
                },
            ],
            ..Default::default()
        };

        let pages = AgentContribution::launcher_pages(&snapshot);

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "codex");
        assert_eq!(pages[0].rank, 0);
        assert_eq!(pages[0].page.url, "vmux://sessions/codex/cli");
        assert_eq!(pages[0].page.title, "Codex (CLI)");
        assert_eq!(pages[1].rank, 1);
        assert_eq!(pages[1].page.title, "Claude Agent");
        assert!(matches!(
            pages[1].page.icon,
            vmux_core::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
        ));
    }

    #[test]
    fn only_bare_agent_urls_are_claimed() {
        let mut world = World::new();
        for url in DEFAULT_AGENT_URLS {
            world.spawn(ClaimedUrl(url.to_string()));
        }

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
