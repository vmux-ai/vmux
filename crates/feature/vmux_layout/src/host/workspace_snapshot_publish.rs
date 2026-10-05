use bevy::prelude::*;
use vmux_command::{
    CommandBarContextSnapshot, CommandBarWorkspaceSnapshot, WriteCommandBarSnapshots,
};
use vmux_ui::i18n::Locale;

use crate::settings::ResolvedLocale;
use crate::workspace_snapshot::TabGather;

pub(crate) struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            publish_workspace_snapshot.in_set(WriteCommandBarSnapshots),
        );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar workspace"),
        CommandBarWorkspaceSnapshot::default(),
    ));
}

fn publish_workspace_snapshot(
    tab_gather: TabGather,
    locale: Option<Res<ResolvedLocale>>,
    projects: Query<(&crate::tab::Tab, Option<&crate::tab::TabWorkspace>)>,
    context: Single<&CommandBarContextSnapshot>,
    mut state: Single<&mut CommandBarWorkspaceSnapshot>,
) {
    let active_tab = tab_gather.active_tab.get();
    let project_root = 'root: {
        let Some(active_tab) = active_tab else {
            break 'root None;
        };
        let Ok((tab, workspace)) = projects.get(active_tab) else {
            break 'root None;
        };
        if let Some(workspace) = workspace {
            let dir = workspace.project_dir.trim();
            if !dir.is_empty() {
                break 'root Some(dir.to_string());
            }
        }
        let Some(dir) = tab.startup_dir.as_deref() else {
            break 'root None;
        };
        let dir = dir.trim();
        if dir.is_empty() {
            break 'root None;
        }
        Some(dir.to_string())
    };
    let (_, pane, stack) = tab_gather.focus.resolve(active_tab);
    let locale = locale
        .as_deref()
        .map(|resolved| resolved.0.clone())
        .unwrap_or_else(Locale::preferred);
    let tabs = tab_gather.tabs(active_tab, &context.label, &locale);
    let next = CommandBarWorkspaceSnapshot {
        stack,
        pane,
        tabs,
        stack_count: tab_gather.stack_q.iter().count(),
        project_root,
    };
    if **state != next {
        **state = next;
    }
}
