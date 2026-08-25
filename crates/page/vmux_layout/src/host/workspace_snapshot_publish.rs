use bevy::prelude::*;
use vmux_command::snapshot::{
    CommandBarSpacesSnapshot, CommandBarWorkspaceSnapshot, WriteCommandBarSnapshots,
};
use vmux_ui::i18n::Locale;

use crate::settings::ResolvedLocale;
use crate::workspace_snapshot::{TabGatherParams, gather_command_bar_tabs};

pub(crate) struct WorkspaceSnapshotPlugin;

impl Plugin for WorkspaceSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            publish_workspace_snapshot.in_set(WriteCommandBarSnapshots),
        );
    }
}

fn publish_workspace_snapshot(
    tab_gather: TabGatherParams,
    spaces: Res<CommandBarSpacesSnapshot>,
    locale: Option<Res<ResolvedLocale>>,
    projects: Query<(&crate::tab::Tab, Option<&crate::tab::TabWorkspace>)>,
    mut snapshot: ResMut<CommandBarWorkspaceSnapshot>,
) {
    let active_tab = tab_gather.active_tab.get();
    let project_root = ProjectRoot::of(active_tab, &projects);
    let (_, pane, stack) = crate::stack::focused_stack(
        active_tab,
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
    );
    let locale = locale
        .as_deref()
        .map(|resolved| resolved.0.clone())
        .unwrap_or_else(Locale::preferred);
    let tabs = gather_command_bar_tabs(
        active_tab,
        &tab_gather.all_children,
        &tab_gather.leaf_panes,
        &tab_gather.pane_ts,
        &tab_gather.pane_children,
        &tab_gather.stack_ts,
        &tab_gather.stack_q,
        &tab_gather.browser_meta,
        &tab_gather.child_of_q,
        &spaces.active_space_name,
        &locale,
    );
    let next = CommandBarWorkspaceSnapshot {
        stack,
        pane,
        tabs,
        stack_count: tab_gather.stack_q.iter().count(),
        project_root,
    };
    if *snapshot != next {
        *snapshot = next;
    }
}

struct ProjectRoot;

impl ProjectRoot {
    fn of(
        active_tab: Option<Entity>,
        projects: &Query<(&crate::tab::Tab, Option<&crate::tab::TabWorkspace>)>,
    ) -> Option<String> {
        let Ok((tab, workspace)) = projects.get(active_tab?) else {
            return None;
        };
        if let Some(workspace) = workspace {
            let dir = workspace.project_dir.trim();
            if !dir.is_empty() {
                return Some(dir.to_string());
            }
        }
        let dir = tab.startup_dir.as_deref()?.trim();
        if dir.is_empty() {
            return None;
        }
        Some(dir.to_string())
    }
}
