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
    pub fn launcher_pages(&self) -> Vec<CommandBarPage> {
        let mut pages = Vec::with_capacity(self.acp.len() + self.providers.len());
        for agent in &self.acp {
            pages.push(CommandBarPage {
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
            });
        }
        for agent in &self.providers {
            pages.push(CommandBarPage {
                host: "agent".to_string(),
                url: agent.url.clone(),
                title: format!("{} (CLI)", agent.name),
                keywords: vec![agent.id.clone(), "cli".to_string(), "agent".to_string()],
                icon: vmux_core::PageIcon::None,
                shortcut: String::new(),
            });
        }
        let mut recent_rank: HashMap<String, usize> = HashMap::new();
        for (rank, target) in self.recent.iter().enumerate() {
            recent_rank.insert(target.url(), rank);
        }
        pages.sort_by(|a, b| {
            recent_rank
                .get(&a.url)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&recent_rank.get(&b.url).copied().unwrap_or(usize::MAX))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        pages
    }

    /// Where a prompt should go: `requested` when that agent is installed, else the most recent.
    ///
    /// `None` means no agent is installed at all, which is the one case a caller has to refuse
    /// rather than substitute for — silently launching a different agent than the one asked for
    /// would be worse than doing nothing.
    pub fn prompt_url(&self, requested: Option<&str>) -> Option<String> {
        let pages = self.launcher_pages();
        if let Some(requested) = requested
            && let Some(page) = pages.iter().find(|page| page.url == requested)
        {
            return Some(page.url.clone());
        }
        pages.first().map(|page| page.url.clone())
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
            snapshot.prompt_url(None).as_deref(),
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
            snapshot.prompt_url(None).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(CommandBarAgentsSnapshot::default().prompt_url(None), None);
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
            snapshot.prompt_url(Some("vmux://agent/claude")).as_deref(),
            Some("vmux://agent/claude")
        );
        assert_eq!(
            snapshot
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
        assert_eq!(pages[0].url, "vmux://agent/codex/cli");
        assert_eq!(pages[0].title, "Codex (CLI)");
        assert_eq!(pages[0].host, "agent");
        assert_eq!(pages[1].title, "Claude Agent");
        assert!(matches!(
            pages[1].icon,
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
