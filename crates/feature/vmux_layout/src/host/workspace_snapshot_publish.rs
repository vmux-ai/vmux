use bevy::prelude::*;
use vmux_command::snapshot::{
    CommandBarSpacesSnapshot, CommandBarWorkspaceSnapshot, WriteCommandBarSnapshots,
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
    spaces: Single<&CommandBarSpacesSnapshot>,
    mut state: Single<&mut CommandBarWorkspaceSnapshot>,
) {
    let active_tab = tab_gather.active_tab.get();
    let project_root = ProjectRoot::resolve(active_tab, &projects);
    let (_, pane, stack) = tab_gather.focus.resolve(active_tab);
    let locale = locale
        .as_deref()
        .map(|resolved| resolved.0.clone())
        .unwrap_or_else(Locale::preferred);
    let tabs = tab_gather.tabs(active_tab, &spaces.active_space_name, &locale);
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

struct ProjectRoot;

impl ProjectRoot {
    fn resolve(
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
