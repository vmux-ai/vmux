use crate::event::{CommandBarPage, CommandBarRecentFile, CommandBarWorkDir};
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

/// Agent identity used for recent-first launcher ordering.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum AgentPromptTarget {
    /// Built-in terminal CLI.
    Cli(AgentKind),
    /// Registry-driven ACP agent.
    Acp { id: String },
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

    #[test]
    fn agents_snapshot_default_is_empty() {
        let s = CommandBarAgentsSnapshot::default();
        assert!(s.providers.is_empty());
        assert!(s.strategies.is_empty());
        assert!(s.acp.is_empty());
        assert!(s.recent.is_empty());
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
