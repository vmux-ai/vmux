use crate::event::{CommandBarPage, CommandBarRecentFile, CommandBarWorkDir, SearchEngine};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::HashMap;
use vmux_core::agent::AgentKind;
use vmux_core::page::PageManifest;

pub struct CommandBarSnapshotPlugin;

impl Plugin for CommandBarSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarWorkspaceSnapshot>()
            .init_resource::<CommandBarProjectRoots>()
            .init_resource::<CommandBarAgentModels>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Startup, update_pages_snapshot);
    }
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WriteCommandBarSnapshots;

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct CommandBarWorkspaceSnapshot {
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
    pub tabs: Vec<vmux_wire::command_bar::CommandBarTab>,
    pub stack_count: usize,
    pub project_root: Option<String>,
}

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct CommandBarProjectRoots {
    pub roots: Vec<String>,
}

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct CommandBarAgentModels {
    pub agents: Vec<vmux_wire::command_bar::AgentModels>,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarAgentsSnapshot {
    pub providers: Vec<AgentProviderSummary>,
    pub strategies: Vec<AgentStrategySummary>,
    pub acp: Vec<AgentProviderSummary>,
    pub recent: Vec<AgentPromptTarget>,
}

impl AgentPromptTarget {
    pub fn recency_ranks(targets: &[Self]) -> HashMap<String, usize> {
        let mut ranks = HashMap::new();
        for (rank, target) in targets.iter().enumerate() {
            ranks.entry(target.url()).or_insert(rank);
        }
        ranks
    }
}

#[derive(SystemParam)]
pub struct Contributions<'w, 's> {
    pages: Query<'w, 's, &'static ContributedPage>,
    commands: Query<'w, 's, &'static ContributedCommand>,
    claimed: Query<'w, 's, &'static ClaimedUrl>,
}

impl Contributions<'_, '_> {
    pub fn pages(&self) -> Vec<&ContributedPage> {
        let mut pages: Vec<&ContributedPage> = self.pages.iter().collect();
        pages.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
        pages
    }

    pub fn commands(&self) -> impl Iterator<Item = &ContributedCommand> {
        self.commands.iter()
    }

    pub fn prompt_url(&self, requested: Option<&str>) -> Option<String> {
        if let Some(requested) = requested
            && self.pages.iter().any(|entry| entry.page.url == requested)
        {
            return Some(requested.to_string());
        }
        let pages = self.pages();
        let first = pages.first()?;
        Some(first.page.url.clone())
    }

    pub fn page_url(&self, id: &str) -> Option<String> {
        let entry = self.pages.iter().find(|entry| entry.id == id)?;
        Some(entry.page.url.clone())
    }

    pub fn claims_url(&self, url: &str) -> bool {
        self.claimed.iter().any(|claimed| claimed.0 == url)
    }
}

type ContributionTouched = Or<(
    Changed<ContributedPage>,
    Changed<ContributedCommand>,
    Changed<ClaimedUrl>,
)>;

#[derive(SystemParam)]
pub struct ContributionsChanged<'w, 's> {
    touched: Query<'w, 's, (), ContributionTouched>,
    pages: RemovedComponents<'w, 's, ContributedPage>,
    commands: RemovedComponents<'w, 's, ContributedCommand>,
    claimed: RemovedComponents<'w, 's, ClaimedUrl>,
}

impl ContributionsChanged<'_, '_> {
    pub fn any(&mut self) -> bool {
        let removed =
            self.pages.read().count() + self.commands.read().count() + self.claimed.read().count();
        removed > 0 || !self.touched.is_empty()
    }
}

#[derive(Component, Clone, Debug)]
pub struct ContributedPage {
    pub id: String,
    pub page: CommandBarPage,
    pub rank: usize,
}

#[derive(Component, Clone, Debug)]
pub struct ContributedCommand {
    pub id: String,
    pub message_id: String,
    pub args: Vec<(String, String)>,
}

#[derive(Component, Clone, Debug)]
pub struct ClaimedUrl(pub String);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum AgentPromptTarget {
    Cli(AgentKind),
    Acp { id: String },
}

impl AgentPromptTarget {
    pub fn url(&self) -> String {
        match self {
            Self::Cli(kind) => format!("{}cli", kind.cli_url_prefix()),
            Self::Acp { id } => format!("vmux://agent/{id}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AgentProviderSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: String,
}

#[derive(Clone, Debug)]
pub struct AgentStrategySummary {
    pub provider: String,
    pub model: String,
}

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct CommandBarSpacesSnapshot {
    pub spaces: Vec<SpaceSummary>,
    pub active_space_id: String,
    pub active_space_name: String,
    pub spaces_page_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub profile: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarTerminalsSnapshot {
    pub running: HashMap<String, Entity>,
    pub agent_session_to_entity: HashMap<(AgentKind, String), Entity>,
    pub terminal_page_url: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<RegisteredPage>,
}

#[derive(Clone, Debug)]
pub struct RegisteredPage {
    pub page: CommandBarPage,
    pub title_message_id: Option<&'static str>,
    pub replaces_command: Option<&'static str>,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarWorkSnapshot {
    pub work_dirs: Vec<CommandBarWorkDir>,
    pub recent_files: Vec<CommandBarRecentFile>,
    pub search_engines: Vec<SearchEngine>,
}

fn update_pages_snapshot(
    manifests: Query<&PageManifest>,
    mut snapshot: ResMut<CommandBarPagesSnapshot>,
) {
    if !snapshot.pages.is_empty() {
        return;
    }
    let mut pages = Vec::new();
    for manifest in &manifests {
        if !manifest.command_bar {
            continue;
        }
        pages.push(RegisteredPage {
            page: CommandBarPage {
                host: manifest.host.to_string(),
                url: manifest.url(),
                title: manifest.title.to_string(),
                keywords: manifest.keywords.iter().map(|k| k.to_string()).collect(),
                icon: manifest
                    .icon
                    .map(vmux_core::PageIcon::Builtin)
                    .unwrap_or_default(),
                shortcut: String::new(),
                prompt_target: false,
            },
            title_message_id: manifest.title_message_id,
            replaces_command: manifest.replaces_command,
        });
    }
    pages.sort_by(|a, b| a.page.url.cmp(&b.page.url));
    snapshot.pages = pages;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    impl ContributedPage {
        fn ranked(url: &str, rank: usize) -> Self {
            Self {
                id: url.to_string(),
                rank,
                page: CommandBarPage {
                    host: "test".to_string(),
                    url: url.to_string(),
                    prompt_target: true,
                    ..Default::default()
                },
            }
        }

        fn prompt_url_among(pages: Vec<Self>, requested: Option<&str>) -> Option<String> {
            let mut world = World::new();
            for page in pages {
                world.spawn(page);
            }
            let requested = requested.map(str::to_string);
            world
                .run_system_once(move |contributions: Contributions| {
                    contributions.prompt_url(requested.as_deref())
                })
                .expect("prompt_url system runs")
        }
    }

    #[test]
    fn agents_snapshot_default_is_empty() {
        let s = CommandBarAgentsSnapshot::default();
        assert!(s.providers.is_empty());
        assert!(s.strategies.is_empty());
        assert!(s.acp.is_empty());
        assert!(s.recent.is_empty());
    }

    #[test]
    fn prompt_goes_to_the_lowest_ranked_page() {
        let pages = vec![
            ContributedPage::ranked("vmux://agent/claude", 1),
            ContributedPage::ranked("vmux://agent/codex/cli", 0),
        ];

        assert_eq!(
            ContributedPage::prompt_url_among(pages, None).as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn prompt_honours_a_listed_request_and_ignores_a_stale_one() {
        let pages = || {
            vec![
                ContributedPage::ranked("vmux://agent/codex/cli", 0),
                ContributedPage::ranked("vmux://agent/claude", 1),
            ]
        };

        assert_eq!(
            ContributedPage::prompt_url_among(pages(), Some("vmux://agent/claude")).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            ContributedPage::prompt_url_among(pages(), Some("vmux://agent/uninstalled")).as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn prompt_refuses_when_no_page_accepts_one() {
        assert_eq!(ContributedPage::prompt_url_among(Vec::new(), None), None);
    }

    #[derive(Component)]
    struct FirstContributor;

    #[derive(Component)]
    struct SecondContributor;

    impl ContributedCommand {
        fn named(id: &str) -> Self {
            Self {
                id: id.to_string(),
                message_id: "command-test-row".to_string(),
                args: Vec::new(),
            }
        }
    }

    #[test]
    fn one_contributor_rebuilding_leaves_the_others_rows() {
        let mut world = World::new();
        world.spawn((FirstContributor, ContributedCommand::named("first")));
        world.spawn((SecondContributor, ContributedCommand::named("second")));

        world
            .run_system_once(
                |mine: Query<Entity, With<FirstContributor>>, mut commands: Commands| {
                    for entity in mine.iter() {
                        commands.entity(entity).despawn();
                    }
                    commands.spawn((FirstContributor, ContributedCommand::named("first-again")));
                },
            )
            .expect("republish runs");

        let ids = world
            .run_system_once(|contributions: Contributions| {
                let mut ids: Vec<String> =
                    contributions.commands().map(|row| row.id.clone()).collect();
                ids.sort();
                ids
            })
            .expect("read runs");
        assert_eq!(ids, ["first-again", "second"]);
    }

    #[test]
    fn pages_snapshot_collects_only_command_bar_pages() {
        let mut app = App::new();
        app.init_resource::<CommandBarPagesSnapshot>()
            .add_systems(Update, update_pages_snapshot);
        app.world_mut().spawn(PageManifest {
            host: "services",
            title: "Services",
            title_message_id: Some("services-title"),
            replaces_command: Some("service_open"),
            keywords: &["daemon"],
            icon: Some(vmux_core::BuiltinIcon::Settings),
            command_bar: true,
        });
        app.world_mut().spawn(PageManifest {
            host: "layout",
            title: "Layout",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: None,
            command_bar: false,
        });

        app.update();

        let snap = app.world().resource::<CommandBarPagesSnapshot>();
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].page.host, "services");
        assert_eq!(snap.pages[0].page.url, "vmux://services/");
        assert_eq!(snap.pages[0].title_message_id, Some("services-title"));
        assert_eq!(snap.pages[0].replaces_command, Some("service_open"));
    }
}
