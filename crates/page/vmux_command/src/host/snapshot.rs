use crate::event::{CommandBarPage, CommandBarRecentFile, CommandBarWorkDir, SearchEngine};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::HashMap;
use vmux_core::agent::AgentKind;
use vmux_core::page::PageManifest;

/// Owns what the command bar searches over: the snapshot resources every domain writes into,
/// and the page list built once from the registered manifests.
///
/// Where [`WriteCommandBarSnapshots`] sits relative to the command bus is
/// [`crate::CommandPlugin`]'s to say, since that orders it against reads and writes.
pub struct CommandBarSnapshotPlugin;

impl Plugin for CommandBarSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandBarAgentsSnapshot>()
            .init_resource::<CommandBarSpacesSnapshot>()
            .init_resource::<CommandBarTerminalsSnapshot>()
            .init_resource::<CommandBarPagesSnapshot>()
            .init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Startup, update_pages_snapshot);
    }
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WriteCommandBarSnapshots;

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarAgentsSnapshot {
    pub providers: Vec<AgentProviderSummary>,
    pub strategies: Vec<AgentStrategySummary>,
    /// Installed registry ACP agents and their single-segment launch URLs.
    pub acp: Vec<AgentProviderSummary>,
    /// Installed ACP and CLI agents, most recently used first.
    pub recent: Vec<AgentPromptTarget>,
}

impl AgentPromptTarget {
    /// Rank of each target's url, most recently used first.
    ///
    /// The snapshot records recency; turning it into the launcher's order is the contributing
    /// crate's business, and this is the part of it that needs no knowledge of what an agent is.
    ///
    /// A url appearing twice keeps its first rank, since that is the more recent one and the later
    /// occurrence would otherwise push the agent down the launcher.
    pub fn recency_ranks(targets: &[Self]) -> HashMap<String, usize> {
        let mut ranks = HashMap::new();
        for (rank, target) in targets.iter().enumerate() {
            ranks.entry(target.url()).or_insert(rank);
        }
        ranks
    }
}

/// What other crates add to the command bar.
///
/// The command bar lists pages and commands; it does not know what any of them are for. Whoever
/// owns a capability spawns it as an entity and handles it when it is chosen, so the command bar
/// stays a launcher rather than growing a branch per feature.
///
/// One entity per row is what lets more than one crate contribute. The list used to be a resource
/// every contributor overwrote wholesale, so a second one silently erased the first every frame.
/// A contributor now despawns only the entities it spawned, which it recognises by its own private
/// marker component — this type never needs to know who owns what.
#[derive(SystemParam)]
pub struct Contributions<'w, 's> {
    pages: Query<'w, 's, &'static ContributedPage>,
    commands: Query<'w, 's, &'static ContributedCommand>,
    claimed: Query<'w, 's, &'static ClaimedUrl>,
}

impl Contributions<'_, '_> {
    /// Contributed pages, most preferred first.
    ///
    /// Entity iteration order carries no meaning, so preference is [`ContributedPage::rank`] and
    /// ties break by id. Rank orders one contributor's own pages; between two contributors the id
    /// is all there is to go on, which is arbitrary but stable.
    pub fn pages(&self) -> Vec<&ContributedPage> {
        let mut pages: Vec<&ContributedPage> = self.pages.iter().collect();
        pages.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
        pages
    }

    /// Contributed command rows, in no particular order.
    pub fn commands(&self) -> impl Iterator<Item = &ContributedCommand> {
        self.commands.iter()
    }

    /// Where a prompt should go: `requested` when that page is listed, else the most preferred.
    ///
    /// `None` means nothing accepts a prompt, which a caller has to refuse rather than substitute
    /// for — opening something other than what was asked for would be worse than doing nothing.
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

    /// The page a contributed id names.
    pub fn page_url(&self, id: &str) -> Option<String> {
        let entry = self.pages.iter().find(|entry| entry.id == id)?;
        Some(entry.page.url.clone())
    }

    /// Whether a contributor resolves this url itself.
    pub fn claims_url(&self, url: &str) -> bool {
        self.claimed.iter().any(|claimed| claimed.0 == url)
    }
}

/// Matches a contribution entity of any kind whose row was spawned or edited.
type ContributionTouched = Or<(
    Changed<ContributedPage>,
    Changed<ContributedCommand>,
    Changed<ClaimedUrl>,
)>;

/// Whether the contributed rows changed since the reading system last ran, despawns included.
///
/// Split from [`Contributions`] because reading despawns needs `&mut` and every other reader only
/// wants to look. Both halves are needed: a republish arrives as spawns, but the last row going
/// away is a despawn and nothing else.
#[derive(SystemParam)]
pub struct ContributionsChanged<'w, 's> {
    touched: Query<'w, 's, (), ContributionTouched>,
    pages: RemovedComponents<'w, 's, ContributedPage>,
    commands: RemovedComponents<'w, 's, ContributedCommand>,
    claimed: RemovedComponents<'w, 's, ClaimedUrl>,
}

impl ContributionsChanged<'_, '_> {
    /// True when a row was spawned, edited or despawned.
    ///
    /// Every removal reader is drained even once the answer is known, so a despawn this frame
    /// cannot be reported again as a change next frame.
    pub fn any(&mut self) -> bool {
        let removed =
            self.pages.read().count() + self.commands.read().count() + self.claimed.read().count();
        removed > 0 || !self.touched.is_empty()
    }
}

/// One contributed page: something the command bar can list, open, and send a prompt to.
#[derive(Component, Clone, Debug)]
pub struct ContributedPage {
    /// Echoed back when this page is chosen by id rather than by url. Opaque to the command bar.
    pub id: String,
    pub page: CommandBarPage,
    /// Preference among this contributor's pages, lowest first.
    pub rank: usize,
}

/// One contributed command-bar row.
#[derive(Component, Clone, Debug)]
pub struct ContributedCommand {
    /// Echoed back by the command bar when this row is chosen. Opaque to it.
    pub id: String,
    /// Fluent message naming the row, with its arguments. Not a rendered string: the contributor
    /// has no locale, and the command bar does.
    pub message_id: String,
    pub args: Vec<(String, String)>,
}

/// A url a contributor resolves itself instead of naming a page to open.
///
/// Opening one of these as an ordinary url would land on nothing: they stand for a choice the
/// contributor has to make — "the default one" — not for a page that exists yet.
#[derive(Component, Clone, Debug)]
pub struct ClaimedUrl(pub String);

/// Agent identity used for recent-first launcher ordering.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum AgentPromptTarget {
    /// Built-in terminal CLI.
    Cli(AgentKind),
    /// Registry-driven ACP agent.
    Acp { id: String },
}

impl AgentPromptTarget {
    /// The launch URL this target resolves to.
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
    /// Optional icon URL (e.g. an ACP-registry agent's SVG); empty = fall back to a default icon.
    pub icon: String,
}

#[derive(Clone, Debug)]
pub struct AgentStrategySummary {
    pub provider: String,
    pub model: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarSpacesSnapshot {
    pub spaces: Vec<SpaceSummary>,
    pub active_space_id: String,
    pub active_space_name: String,
    pub spaces_page_url: String,
}

#[derive(Clone, Debug)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    pub profile: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarTerminalsSnapshot {
    /// The pane already running each terminal row, keyed by the url that row carries.
    ///
    /// Keyed by url rather than by pid so that choosing a terminal is a lookup. The command bar
    /// would otherwise have to parse a pid back out of the url, which means knowing how
    /// `vmux_terminal` builds one — and being wrong about it silently spawns a second terminal
    /// instead of going to the one the user picked.
    pub running: HashMap<String, Entity>,
    pub agent_session_to_entity: HashMap<(AgentKind, String), Entity>,
    pub terminal_page_url: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<RegisteredPage>,
}

/// A page the command bar lists, with the parts of its manifest that only resolve per locale.
///
/// The title and the superseded command stay unresolved here because the snapshot is built once at
/// startup and the locale can change after it. Whoever renders the payload holds the locale and
/// finishes the job.
#[derive(Clone, Debug)]
pub struct RegisteredPage {
    pub page: CommandBarPage,
    pub title_message_id: Option<&'static str>,
    pub replaces_command: Option<&'static str>,
}

/// Command-bar "current work" data: working dirs of open terminal/agent panes and
/// recently-opened `file://` entries. Populated by updater systems in `vmux_layout`.
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

        /// Spawn these rows, then ask where a prompt would go.
        ///
        /// Spawns rather than handing [`Contributions`] a list, so the prompt tests run against
        /// the entities the command bar actually reads.
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

    /// A request names a page; honouring one that is no longer listed would send the prompt
    /// somewhere the user did not ask for, so it falls back to the preferred page instead.
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

    /// Nothing accepting a prompt has to be refused rather than substituted for.
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

    /// Republishing is scoped to the rows the contributor spawned.
    ///
    /// The shape this replaced was a resource each contributor overwrote, so the second one to run
    /// each frame erased the first. Only one crate ever contributed, which is why it never showed.
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
