use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{
    ExplorerCloseEditor, ExplorerCollapseAll, ExplorerPanelSetVisible, ExplorerPanelWidth,
    ExplorerRevealCurrent, ExplorerTreePrefetch, ExplorerTreeRefresh, ExplorerTreeToggle,
};

mod fs;
mod mutation;
mod outline;
mod panel;
mod search;
mod tabs;
mod tree;

#[cfg(test)]
mod tests;

pub(super) use mutation::ExplorerMutationPlugin;
pub(super) use outline::{ExplorerOutlinePlugin, OutlineDirty};
pub use panel::StackExplorerVisibility;
#[cfg(test)]
pub(super) use panel::{ExplorerPanelDefaults, StackExplorerRevision};
pub(super) use panel::{ExplorerPanelPlugin, ExplorerPanelSent};
pub(super) use search::ExplorerSearchPlugin;
pub use search::GlobalSearchRequest;
pub(super) use tabs::{ExplorerTabsPlugin, OpenEditorsDirty};
#[cfg(test)]
pub(super) use tree::{ExplorerTree, IDLE_TREE_CAPACITY};
pub(super) use tree::{ExplorerTreeDirty, ExplorerTreePlugin, ExplorerTrees};

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
        .add_plugins(UiEventPlugin::<(
            ExplorerTreeToggle,
            ExplorerTreePrefetch,
            ExplorerTreeRefresh,
            ExplorerRevealCurrent,
            ExplorerCloseEditor,
            ExplorerPanelSetVisible,
            ExplorerPanelWidth,
        )>::default())
        .add_plugins(UiEventPlugin::<(ExplorerCollapseAll,)>::default());
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
