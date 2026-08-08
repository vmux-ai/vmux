use crate::event::{CommandBarPage, CommandBarRecentFile, CommandBarWorkDir, SearchEngine};
use bevy::prelude::*;
use std::collections::HashMap;
use vmux_core::agent::AgentKind;
use vmux_core::page::PageManifest;

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

impl CommandBarAgentsSnapshot {
    /// Launcher entries for installed ACP and CLI agents, most recently used first.
    pub fn launcher_pages(&self) -> Vec<ContributedPage> {
        let mut pages = Vec::with_capacity(self.acp.len() + self.providers.len());
        for agent in &self.acp {
            pages.push(ContributedPage {
                id: agent.id.clone(),
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
                },
            });
        }
        for agent in &self.providers {
            pages.push(ContributedPage {
                id: agent.id.clone(),
                page: CommandBarPage {
                    host: "agent".to_string(),
                    url: agent.url.clone(),
                    title: format!("{} (CLI)", agent.name),
                    keywords: vec![agent.id.clone(), "cli".to_string(), "agent".to_string()],
                    icon: vmux_core::PageIcon::None,
                    shortcut: String::new(),
                },
            });
        }
        let mut recent_rank: HashMap<String, usize> = HashMap::new();
        for (rank, target) in self.recent.iter().enumerate() {
            recent_rank.insert(target.url(), rank);
        }
        pages.sort_by(|a, b| {
            recent_rank
                .get(&a.page.url)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&recent_rank.get(&b.page.url).copied().unwrap_or(usize::MAX))
                .then_with(|| {
                    a.page
                        .title
                        .to_lowercase()
                        .cmp(&b.page.title.to_lowercase())
                })
        });
        pages
    }
}

/// What other crates add to the command bar.
///
/// The command bar lists pages and commands; it does not know what any of them are for. Whoever
/// owns a capability describes it here and handles it when it is chosen, so the command bar stays
/// a launcher rather than growing a branch per feature.
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarContributions {
    /// Pages to list, and the targets a prompt can be sent to, most preferred first.
    pub pages: Vec<ContributedPage>,
    pub commands: Vec<ContributedCommand>,
    /// Urls a contributor resolves itself instead of them naming a page to open.
    ///
    /// Opening one of these as an ordinary url would land on nothing: they stand for a choice the
    /// contributor has to make — "the default one" — not for a page that exists yet.
    pub claimed_urls: Vec<String>,
}

/// One contributed page: something the command bar can list, open, and send a prompt to.
#[derive(Clone, Debug)]
pub struct ContributedPage {
    /// Echoed back when this page is chosen by id rather than by url. Opaque to the command bar.
    pub id: String,
    pub page: CommandBarPage,
}

/// One contributed command-bar row.
#[derive(Clone, Debug)]
pub struct ContributedCommand {
    /// Echoed back by the command bar when this row is chosen. Opaque to it.
    pub id: String,
    /// Fluent message naming the row, with its arguments. Not a rendered string: the contributor
    /// has no locale, and the command bar does.
    pub message_id: String,
    pub args: Vec<(String, String)>,
}

impl CommandBarContributions {
    /// Where a prompt should go: `requested` when that page is listed, else the most preferred.
    ///
    /// `None` means nothing accepts a prompt, which a caller has to refuse rather than substitute
    /// for — opening something other than what was asked for would be worse than doing nothing.
    pub fn prompt_url(&self, requested: Option<&str>) -> Option<String> {
        if let Some(requested) = requested
            && let Some(entry) = self.pages.iter().find(|entry| entry.page.url == requested)
        {
            return Some(entry.page.url.clone());
        }
        self.pages.first().map(|entry| entry.page.url.clone())
    }

    /// The page a contributed id names.
    pub fn page_url(&self, id: &str) -> Option<String> {
        let entry = self.pages.iter().find(|entry| entry.id == id)?;
        Some(entry.page.url.clone())
    }

    /// Whether a contributor resolves this url itself.
    pub fn claims_url(&self, url: &str) -> bool {
        self.claimed_urls.iter().any(|claimed| claimed == url)
    }
}

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
    pub pid_to_entity: HashMap<u32, Entity>,
    pub agent_session_to_entity: HashMap<(AgentKind, String), Entity>,
    pub terminal_page_url: String,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarPagesSnapshot {
    pub pages: Vec<CommandBarPage>,
}

/// Command-bar "current work" data: working dirs of open terminal/agent panes and
/// recently-opened `file://` entries. Populated by updater systems in `vmux_layout`.
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarWorkSnapshot {
    pub work_dirs: Vec<CommandBarWorkDir>,
    pub recent_files: Vec<CommandBarRecentFile>,
    pub search_engines: Vec<SearchEngine>,
}

pub fn update_pages_snapshot(
    manifests: Query<&PageManifest>,
    mut snapshot: ResMut<CommandBarPagesSnapshot>,
) {
    if !snapshot.pages.is_empty() {
        return;
    }
    let mut pages: Vec<CommandBarPage> = manifests
        .iter()
        .filter(|manifest| manifest.command_bar)
        .map(|manifest| CommandBarPage {
            host: manifest.host.to_string(),
            url: manifest.url(),
            title: manifest.title.to_string(),
            keywords: manifest.keywords.iter().map(|k| k.to_string()).collect(),
            icon: manifest
                .icon
                .map(vmux_core::PageIcon::Builtin)
                .unwrap_or_default(),
            shortcut: String::new(),
        })
        .collect();
    pages.sort_by(|a, b| a.url.cmp(&b.url));
    snapshot.pages = pages;
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::agent::AgentKind;

    /// What the contributing crate publishes, so the prompt tests run the real path.
    fn contributions(agents: &CommandBarAgentsSnapshot) -> CommandBarContributions {
        CommandBarContributions {
            pages: agents.launcher_pages(),
            ..Default::default()
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
    fn prompt_prefers_most_recent_installed_agent() {
        let snapshot = CommandBarAgentsSnapshot {
            recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
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
                icon: String::new(),
            }],
            ..Default::default()
        };

        assert_eq!(
            contributions(&snapshot).prompt_url(None).as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn prompt_falls_back_to_installed_agent() {
        let snapshot = CommandBarAgentsSnapshot {
            acp: vec![AgentProviderSummary {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                url: "vmux://agent/claude".to_string(),
                icon: String::new(),
            }],
            ..Default::default()
        };

        assert_eq!(
            contributions(&snapshot).prompt_url(None).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            contributions(&CommandBarAgentsSnapshot::default()).prompt_url(None),
            None
        );
    }

    #[test]
    fn prompt_uses_selected_installed_agent_and_rejects_stale_url() {
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
                icon: String::new(),
            }],
            recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
            ..Default::default()
        };

        assert_eq!(
            contributions(&snapshot)
                .prompt_url(Some("vmux://agent/claude"))
                .as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            contributions(&snapshot)
                .prompt_url(Some("vmux://agent/uninstalled"))
                .as_deref(),
            Some("vmux://agent/codex/cli")
        );
    }

    #[test]
    fn launcher_pages_lists_only_snapshot_agents_in_recent_order() {
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
        let pages = snapshot.launcher_pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, "codex");
        assert_eq!(pages[0].page.url, "vmux://agent/codex/cli");
        assert_eq!(pages[0].page.title, "Codex (CLI)");
        assert_eq!(pages[0].page.host, "agent");
        assert_eq!(pages[1].page.title, "Claude Agent");
        assert!(matches!(
            pages[1].page.icon,
            vmux_core::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
        ));
    }

    #[test]
    fn terminals_snapshot_default_is_empty() {
        let s = CommandBarTerminalsSnapshot::default();
        assert!(s.pid_to_entity.is_empty());
        assert!(s.agent_session_to_entity.is_empty());
    }

    #[test]
    fn pages_snapshot_collects_only_command_bar_pages() {
        let mut app = App::new();
        app.init_resource::<CommandBarPagesSnapshot>()
            .add_systems(Update, update_pages_snapshot);
        app.world_mut().spawn(PageManifest {
            host: "settings",
            title: "Settings",
            keywords: &["preferences"],
            icon: Some(vmux_core::BuiltinIcon::Settings),
            command_bar: true,
        });
        app.world_mut().spawn(PageManifest {
            host: "layout",
            title: "Layout",
            keywords: &[],
            icon: None,
            command_bar: false,
        });

        app.update();

        let snap = app.world().resource::<CommandBarPagesSnapshot>();
        assert_eq!(snap.pages.len(), 1);
        assert_eq!(snap.pages[0].host, "settings");
        assert_eq!(snap.pages[0].url, "vmux://settings/");
    }
}
