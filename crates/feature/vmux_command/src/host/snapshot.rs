use bevy::prelude::*;
use std::collections::HashMap;
use vmux_api::command_bar::{
    CommandBarPage, CommandBarRecentFile, CommandBarWorkDir, SearchEngine,
};
use vmux_ecs::launcher::RendersLauncherPanel;
use vmux_ecs::page::PageManifest;

pub type CommandBarUiStateUpdates =
    vmux_ecs::host::UiState<vmux_api::command_bar::CommandBarUiState>;

pub struct UiStatePlugin;

impl Plugin for UiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawn_bar_projection, ApplyDeferred, update_pages).chain(),
        )
        .add_plugins(vmux_ecs::host::UiStatePlugin::<
            vmux_api::command_bar::CommandBarUiState,
        >::default())
        .add_systems(PreUpdate, attach_bar_ui_state);
    }
}

fn spawn_bar_projection(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar projection"),
        CommandBarProjection::default(),
    ));
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WriteCommandBarSnapshots;

#[derive(Component, Default, Clone, Debug)]
pub struct CommandBarProjection {
    pub agents: CommandBarAgentsSnapshot,
    pub workspace: CommandBarWorkspaceSnapshot,
    pub projects: CommandBarProjectRoots,
    pub agent_models: CommandBarAgentModels,
    pub agent_modes: CommandBarAgentModes,
    pub spaces: CommandBarSpacesSnapshot,
    pub terminals: CommandBarTerminalsSnapshot,
    pub pages: CommandBarPagesSnapshot,
    pub work: CommandBarWorkSnapshot,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CommandBarWorkspaceSnapshot {
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
    pub tabs: Vec<vmux_api::command_bar::CommandBarTab>,
    pub stack_count: usize,
    pub project_root: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CommandBarProjectRoots {
    pub roots: Vec<String>,
    pub active: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CommandBarAgentModels {
    pub agents: Vec<vmux_api::command_bar::AgentModels>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CommandBarAgentModes {
    pub agents: Vec<vmux_api::command_bar::AgentModes>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CommandBarAgentsSnapshot {
    pub acp: Vec<AgentSummary>,
    pub recent: Vec<AgentPromptTarget>,
}

impl AgentPromptTarget {
    pub fn recency_ranks(targets: &[Self]) -> HashMap<String, usize> {
        let mut ranks = HashMap::new();
        for (rank, target) in targets.iter().enumerate() {
            ranks.entry(target.url.clone()).or_insert(rank);
        }
        ranks
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ContributedPages<'w, 's> {
    pages: Query<'w, 's, &'static ContributedPage>,
}

impl ContributedPages<'_, '_> {
    pub fn sorted(&self) -> Vec<ContributedPage> {
        let mut pages: Vec<ContributedPage> = self.pages.iter().cloned().collect();
        pages.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
        pages
    }

    pub fn prompt_url(&self, requested: Option<&str>) -> Option<String> {
        if let Some(requested) = requested
            && self.pages.iter().any(|entry| entry.page.url == requested)
        {
            return Some(requested.to_string());
        }
        let pages = self.sorted();
        let first = pages.first()?;
        Some(first.page.url.clone())
    }

    pub fn page_url(&self, id: &str) -> Option<String> {
        let entry = self.pages.iter().find(|entry| entry.id == id)?;
        Some(entry.page.url.clone())
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ClaimedUrls<'w, 's> {
    urls: Query<'w, 's, &'static ClaimedUrl>,
}

impl ClaimedUrls<'_, '_> {
    pub fn contains(&self, url: &str) -> bool {
        self.urls.iter().any(|claimed| claimed.matches(url))
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

impl ClaimedUrl {
    fn matches(&self, url: &str) -> bool {
        match (
            vmux_api::VmuxRoute::canonical(&self.0),
            vmux_api::VmuxRoute::canonical(url),
        ) {
            (Some(claimed), Some(candidate)) => claimed == candidate,
            _ => self.0 == url,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AgentPromptTarget {
    pub id: String,
    url: String,
}

impl AgentPromptTarget {
    pub fn under(root: &str, id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            url: format!("{root}{id}"),
            id,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: String,
}

impl From<&AgentSummary> for ContributedPage {
    fn from(agent: &AgentSummary) -> Self {
        Self {
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
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
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

#[derive(Default, Clone, Debug)]
pub struct CommandBarTerminalsSnapshot {
    pub running: HashMap<String, Entity>,
    pub terminal_page_url: String,
}

#[derive(Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<RegisteredPage>,
}

#[derive(Clone, Debug)]
pub struct RegisteredPage {
    pub page: CommandBarPage,
    pub title_message_id: Option<String>,
    pub replaces_command: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub struct CommandBarWorkSnapshot {
    pub work_dirs: Vec<CommandBarWorkDir>,
    pub recent_files: Vec<CommandBarRecentFile>,
    pub search_engines: Vec<SearchEngine>,
    pub projects: Vec<String>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CommandBarWorkDirectory(pub String);

fn update_pages(manifests: Query<&PageManifest>, mut state: Single<&mut CommandBarProjection>) {
    let snapshot = &mut state.pages;
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
                url: manifest.url(),
                title: manifest.title.to_string(),
                keywords: manifest.keywords.iter().map(|k| k.to_string()).collect(),
                icon: manifest
                    .icon
                    .map(vmux_ecs::PageIcon::Builtin)
                    .unwrap_or_default(),
                shortcut: String::new(),
                prompt_target: false,
                startup: manifest.startup,
            },
            title_message_id: manifest.title_message_id.map(str::to_string),
            replaces_command: manifest.replaces_command.map(str::to_string),
        });
    }
    pages.sort_by(|a, b| a.page.url.cmp(&b.page.url));
    snapshot.pages = pages;
}

fn attach_bar_ui_state(
    pages: Query<
        Entity,
        (
            With<RendersLauncherPanel>,
            Without<CommandBarUiStateUpdates>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands
            .entity(page)
            .insert(CommandBarUiStateUpdates::default());
    }
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
                .run_system_once(move |pages: ContributedPages| {
                    pages.prompt_url(requested.as_deref())
                })
                .expect("prompt_url system runs")
        }
    }

    #[test]
    fn agents_snapshot_default_is_empty() {
        let s = CommandBarAgentsSnapshot::default();
        assert!(s.acp.is_empty());
        assert!(s.recent.is_empty());
    }

    #[test]
    fn prompt_goes_to_the_lowest_ranked_page() {
        let pages = vec![
            ContributedPage::ranked("vmux://sessions/claude", 1),
            ContributedPage::ranked("vmux://sessions/codex/cli", 0),
        ];

        assert_eq!(
            ContributedPage::prompt_url_among(pages, None).as_deref(),
            Some("vmux://sessions/codex/cli")
        );
    }

    #[test]
    fn prompt_honours_a_listed_request_and_ignores_a_stale_one() {
        let pages = || {
            vec![
                ContributedPage::ranked("vmux://sessions/codex/cli", 0),
                ContributedPage::ranked("vmux://sessions/claude", 1),
            ]
        };

        assert_eq!(
            ContributedPage::prompt_url_among(pages(), Some("vmux://sessions/claude")).as_deref(),
            Some("vmux://sessions/claude")
        );
        assert_eq!(
            ContributedPage::prompt_url_among(pages(), Some("vmux://sessions/uninstalled"))
                .as_deref(),
            Some("vmux://sessions/codex/cli")
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
            .run_system_once(|commands: Query<&ContributedCommand>| {
                let mut ids: Vec<String> = commands.iter().map(|row| row.id.clone()).collect();
                ids.sort();
                ids
            })
            .expect("read runs");
        assert_eq!(ids, ["first-again", "second"]);
    }

    #[test]
    fn pages_snapshot_collects_only_command_bar_pages() {
        let mut app = App::new();
        app.add_systems(Update, update_pages);
        app.world_mut().spawn(CommandBarProjection::default());
        app.world_mut().spawn(PageManifest {
            url: "vmux://services/",
            asset_host: "services",
            owns_subtree: false,
            title: "Services",
            title_message_id: Some("services-title"),
            replaces_command: Some("service_open"),
            keywords: &["daemon"],
            icon: Some(vmux_ecs::BuiltinIcon::Settings),
            command_bar: true,
            startup: false,
        });
        app.world_mut().spawn(PageManifest {
            url: "vmux://layout/",
            asset_host: "layout",
            owns_subtree: false,
            title: "Layout",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: None,
            command_bar: false,
            startup: false,
        });

        app.update();

        let snapshot = app
            .world()
            .iter_entities()
            .find_map(|entity| entity.get::<CommandBarProjection>())
            .unwrap();
        let snap = &snapshot.pages;
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].page.url, "vmux://services/");
        assert_eq!(snap.pages[0].page.url, "vmux://services/");
        assert_eq!(
            snap.pages[0].title_message_id.as_deref(),
            Some("services-title")
        );
        assert_eq!(
            snap.pages[0].replaces_command.as_deref(),
            Some("service_open")
        );
    }
}
