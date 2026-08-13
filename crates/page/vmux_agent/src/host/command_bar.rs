//! What this crate offers the command bar, and what it does when one of those rows is chosen.
//!
//! The command bar is a launcher: it lists things and reports which was picked. It has no idea
//! what an agent is, so agents are described here as [`ContributedCommand`]s and claimed back here
//! when chosen. Keeping both halves in one file is the point — the id format is a private contract
//! between them, and splitting it would let the two drift.

use bevy::prelude::*;
use vmux_command::event::CommandBarPage;
use vmux_command::snapshot::{
    AgentPromptTarget, ClaimedUrl, CommandBarAgentsSnapshot, ContributedCommand, ContributedPage,
    WriteCommandBarSnapshots,
};
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest,
};

pub(crate) struct CommandBarPlugin;

impl Plugin for CommandBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, claim_chosen_command).add_systems(
            Update,
            // Publishing reads the snapshots the updaters write, so it runs after them. Stated
            // here rather than left to position in the set's chain, which is where it used to sit.
            publish_contributions
                .in_set(WriteCommandBarSnapshots)
                .after(crate::snapshot_updater::update_agent_sessions_snapshot),
        );
    }
}

/// Urls that name "whichever agent is default" rather than a page that exists.
///
/// Kept from before agent urls carried an id. The command bar cannot open these — there is nothing
/// at them until this crate picks one — so it hands them back instead.
const DEFAULT_AGENT_URLS: [&str; 2] = ["vmux://agent/", "vmux://agent"];

/// Marks a contribution entity as this crate's, so a republish clears only what it published.
///
/// Private on purpose: ownership is between a contributor and its own rows, and the command bar
/// reads every contribution without caring which crate spawned it.
#[derive(Component)]
struct AgentContribution;

impl AgentContribution {
    /// Launcher rows for installed ACP and CLI agents, most recently used first.
    ///
    /// What an agent looks like in a launcher is this crate's to decide: that a CLI provider is
    /// labelled as one, that an ACP agent's icon is a favicon when the registry gave it a url, and
    /// that recency beats alphabetical. Rank is written onto each row because the entities these
    /// become carry preference rather than inherit it from a position in a list.
    fn launcher_pages(agents: &CommandBarAgentsSnapshot) -> Vec<ContributedPage> {
        let mut pages = Vec::with_capacity(agents.acp.len() + agents.providers.len());
        for agent in &agents.acp {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    host: "agent".to_string(),
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
        for agent in &agents.providers {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                rank: 0,
                page: CommandBarPage {
                    host: "agent".to_string(),
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

/// Publish the agents to launch, and a row per model.
fn publish_contributions(
    agents: Res<CommandBarAgentsSnapshot>,
    mine: Query<Entity, With<AgentContribution>>,
    mut commands: Commands,
) {
    if !agents.is_changed() {
        return;
    }
    for entity in mine.iter() {
        commands.entity(entity).despawn();
    }
    for page in AgentContribution::launcher_pages(&agents) {
        commands.spawn((AgentContribution, page));
    }
    for strategy in &agents.strategies {
        let row = AppAgentId {
            provider: strategy.provider.clone(),
            model: strategy.model.clone(),
        };
        commands.spawn((
            AgentContribution,
            ContributedCommand {
                id: row.to_string(),
                message_id: "command-new-app-chat".to_string(),
                args: vec![
                    ("provider".to_string(), row.provider),
                    ("model".to_string(), row.model),
                ],
            },
        ));
    }
    for url in DEFAULT_AGENT_URLS {
        commands.spawn((AgentContribution, ClaimedUrl(url.to_string())));
    }
}

/// Act on a row or url the command bar handed back.
fn claim_chosen_command(
    mut reader: MessageReader<vmux_layout::ContributedCommandChosen>,
    mut attach: MessageWriter<PageAgentAttachRequest>,
    mut spawn: MessageWriter<PageAgentSpawnStackRequest>,
    mut attach_default: MessageWriter<PageAgentAttachDefaultRequest>,
    mut spawn_default: MessageWriter<PageAgentSpawnDefaultRequest>,
) {
    for chosen in reader.read() {
        if DEFAULT_AGENT_URLS.contains(&chosen.id.as_str()) {
            if let Some(stack) = chosen.stack {
                attach_default.write(PageAgentAttachDefaultRequest { stack });
            } else if let Some(pane) = chosen.pane {
                spawn_default.write(PageAgentSpawnDefaultRequest { pane });
            }
            continue;
        }
        let Some(row) = AppAgentId::parse(&chosen.id) else {
            continue;
        };
        let AppAgentId { provider, model } = row;
        let sid = uuid::Uuid::new_v4().to_string();
        if let Some(stack) = chosen.stack {
            attach.write(PageAgentAttachRequest {
                stack,
                provider,
                model,
                sid,
            });
        } else if let Some(pane) = chosen.pane {
            spawn.write(PageAgentSpawnStackRequest {
                pane,
                provider,
                model,
                sid,
            });
        }
    }
}

/// The id of a command-bar row that starts a chat with one provider and model.
///
/// Round trips through [`Display`](std::fmt::Display) and [`AppAgentId::parse`]: the command bar
/// carries the id and hands it back, so the two must agree on a format nobody else writes.
struct AppAgentId {
    provider: String,
    model: String,
}

impl AppAgentId {
    /// The provider and model an id names, or `None` when the row is someone else's.
    fn parse(id: &str) -> Option<Self> {
        let body = id.strip_prefix("app_")?.strip_suffix("_new")?;
        let (provider, model) = body.split_once('_')?;
        Some(Self {
            provider: provider.to_string(),
            model: model.to_string(),
        })
    }
}

impl std::fmt::Display for AppAgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { provider, model } = self;
        write!(f, "app_{provider}_{model}_new")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_command::snapshot::{AgentProviderSummary, Contributions};
    use vmux_core::agent::AgentKind;

    /// The launcher lists exactly what is installed, most recently used first, and each row
    /// carries that order as its rank because the entity it becomes has no position to inherit.
    #[test]
    fn launcher_pages_list_only_installed_agents_in_recent_order() {
        let snapshot = CommandBarAgentsSnapshot {
            providers: vec![AgentProviderSummary {
                id: "codex".to_string(),
                name: "Codex".to_string(),
                url: "vmux://agent/codex/cli".to_string(),
                icon: String::new(),
            }],
            acp: vec![AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
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
        assert_eq!(pages[0].page.url, "vmux://agent/codex/cli");
        assert_eq!(pages[0].page.title, "Codex (CLI)");
        assert_eq!(pages[0].page.host, "agent");
        assert_eq!(pages[1].rank, 1);
        assert_eq!(pages[1].page.title, "Claude Agent");
        assert!(matches!(
            pages[1].page.icon,
            vmux_core::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
        ));
    }

    /// The id is a private round trip between the two halves of this file. A row whose id does not
    /// survive it is published and then silently ignored when the user picks it.
    #[test]
    fn a_published_row_id_parses_back_to_what_named_it() {
        let id = AppAgentId {
            provider: "anthropic".to_string(),
            model: "claude-opus-4".to_string(),
        }
        .to_string();
        let parsed = AppAgentId::parse(&id).expect("an id this file wrote must parse");
        assert_eq!(
            (parsed.provider.as_str(), parsed.model.as_str()),
            ("anthropic", "claude-opus-4"),
            "model names contain the separator, so only the first underscore may split"
        );
    }

    /// Rows contributed by other crates land in the same reader; claiming them would start an
    /// agent for something entirely unrelated.
    #[test]
    fn another_crates_row_is_left_alone() {
        for id in ["browser_open_history", "app_new", "app_onlyprovider_new"] {
            assert!(AppAgentId::parse(id).is_none(), "{id} is not ours to claim");
        }
    }

    /// Only the bare urls stand for "the default agent". Claiming one that carries an id would
    /// send the user to whichever agent is default instead of the one they named.
    #[test]
    fn only_the_bare_agent_url_is_claimed() {
        let mut world = World::new();
        for url in DEFAULT_AGENT_URLS {
            world.spawn(ClaimedUrl(url.to_string()));
        }

        let claimed = world
            .run_system_once(|contributions: Contributions| {
                [
                    contributions.claims_url("vmux://agent/"),
                    contributions.claims_url("vmux://agent"),
                    contributions.claims_url("vmux://agent/codex"),
                    contributions.claims_url("vmux://agent/codex/cli"),
                ]
            })
            .expect("claims_url system runs");

        assert_eq!(claimed, [true, true, false, false]);
    }
}
