use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{
    ExplorerCloseEditor, ExplorerCollapseAll, ExplorerPanelSetVisible, ExplorerPanelWidth,
    ExplorerRevealCurrent, ExplorerTreePrefetch, ExplorerTreeRefresh, ExplorerTreeToggle,
};

use super::explorer_mutation::ExplorerMutationPlugin;
use super::explorer_outline::ExplorerOutlinePlugin;
use super::explorer_panel::ExplorerPanelPlugin;
use super::explorer_search::ExplorerSearchPlugin;
use super::explorer_tabs::ExplorerTabsPlugin;
use super::explorer_tree::ExplorerTreePlugin;

pub(super) struct EditorExplorerPlugin;

impl Plugin for EditorExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExplorerTreePlugin,
            ExplorerPanelPlugin,
            ExplorerMutationPlugin,
            ExplorerOutlinePlugin,
            ExplorerSearchPlugin,
            ExplorerTabsPlugin,
        ))
        .add_plugins(BinEventEmitterPlugin::<(
            ExplorerTreeToggle,
            ExplorerTreePrefetch,
            ExplorerTreeRefresh,
            ExplorerRevealCurrent,
            ExplorerCloseEditor,
            ExplorerPanelSetVisible,
            ExplorerPanelWidth,
        )>::default())
        .add_plugins(BinEventEmitterPlugin::<(ExplorerCollapseAll,)>::default());
    }
}

#[derive(Component, Default)]
pub(crate) struct ExplorerState {
    pub root: PathBuf,
    pub open_editors: Vec<PathBuf>,
    pub focus_path: Option<PathBuf>,
    pub(super) active_editor: Option<PathBuf>,
    pub(super) active_editor_is_dir: bool,
}

impl ExplorerState {
    pub(super) fn allows(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    pub(super) fn close_editor(&mut self, path: &Path) -> Option<PathBuf> {
        let at = self.open_editors.iter().position(|open| open == path)?;
        self.open_editors.remove(at);
        let neighbour = at.min(self.open_editors.len().saturating_sub(1));
        self.open_editors.get(neighbour).cloned()
    }
}
