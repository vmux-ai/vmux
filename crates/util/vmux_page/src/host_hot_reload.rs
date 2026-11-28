use std::path::Path;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::{
    ChildOf, Commands, Component, Entity, EntityEvent, IntoScheduleConfigs, MessageReader, Query,
    With, Without,
};
use bevy_ecs::schedule::ApplyDeferred;
use tracing::warn;
use vmux_ecs::{FilePath, UiIpcReady};
use vmux_watch::{FileWatch, FileWatchEvent, FileWatchPlugin, FileWatchSet};

use crate::host::{HostedPage, PageScriptRequest};

const SWAP_TEMPLATE: &str = include_str!("hot_reload.js");

pub struct HotReloadPlugin;

impl Plugin for HotReloadPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FileWatchPlugin>() {
            app.add_plugins(FileWatchPlugin);
        }
        app.add_systems(
            Update,
            (attach_watchers, ApplyDeferred)
                .chain()
                .before(FileWatchSet::Reconcile),
        )
        .add_systems(Update, apply_changes.after(FileWatchSet::Reconcile));
    }
}

#[derive(Component, Clone, Debug)]
pub struct HotReloadStylesheet {
    pub stylesheet_id: String,
    pub reload_count: u64,
}

impl HotReloadStylesheet {
    pub fn new(stylesheet_id: impl Into<String>) -> Self {
        Self {
            stylesheet_id: stylesheet_id.into(),
            reload_count: 0,
        }
    }

    fn swap_script(&self, css: &str) -> Result<String, serde_json::Error> {
        let id_json = serde_json::to_string(&self.stylesheet_id)?;
        let css_json = serde_json::to_string(css)?;
        Ok(SWAP_TEMPLATE
            .replace("__VMUX_HOT_RELOAD_ID__", &id_json)
            .replace("__VMUX_HOT_RELOAD_CSS__", &css_json))
    }
}

struct StylesheetFileIo;

impl StylesheetFileIo {
    fn watch_directory(path: &FilePath) -> Option<&Path> {
        path.as_path()
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    }

    fn matches(target: &FilePath, candidate: &Path) -> bool {
        if candidate == target.as_path() {
            return true;
        }
        match (target.as_path().canonicalize(), candidate.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }

    fn read(path: &FilePath) -> Result<String, std::io::Error> {
        std::fs::read_to_string(path.as_path())
    }
}

#[derive(Component)]
struct AttachedWatch;

#[derive(EntityEvent, Clone, Debug)]
pub struct StylesheetReloaded {
    #[event_target]
    pub page: Entity,
    pub stylesheet_id: String,
    pub reload_count: u64,
}

type PendingWatchQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static FilePath), (With<HotReloadStylesheet>, Without<AttachedWatch>)>;

type TrackedStylesheetsQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static FilePath,
        &'static mut HotReloadStylesheet,
    ),
>;

fn attach_watchers(pending: PendingWatchQuery, mut commands: Commands) {
    for (entity, path) in &pending {
        let Some(directory) = StylesheetFileIo::watch_directory(path) else {
            continue;
        };
        commands.spawn((FileWatch::non_recursive(directory), ChildOf(entity)));
        commands.entity(entity).try_insert(AttachedWatch);
    }
}

fn apply_changes(
    mut events: MessageReader<FileWatchEvent>,
    ready_pages: Query<(), (With<HostedPage>, With<UiIpcReady>)>,
    mut sheets: TrackedStylesheetsQuery,
    mut commands: Commands,
) {
    let mut changed_paths = Vec::new();
    for event in events.read() {
        changed_paths.extend(event.paths.iter().cloned());
    }
    if changed_paths.is_empty() {
        return;
    }
    for (parent, path, mut sheet) in &mut sheets {
        let page = parent.parent();
        if !ready_pages.contains(page) {
            continue;
        }
        if !changed_paths
            .iter()
            .any(|candidate| StylesheetFileIo::matches(path, candidate))
        {
            continue;
        }
        let css = match StylesheetFileIo::read(path) {
            Ok(css) => css,
            Err(error) => {
                warn!(
                    "vmux_page: skipping hot reload for {}: {error}",
                    path.as_path().display()
                );
                continue;
            }
        };
        let Ok(script) = sheet.swap_script(&css) else {
            continue;
        };
        sheet.reload_count = sheet.reload_count.saturating_add(1);
        commands.trigger(PageScriptRequest { page, script });
        commands.trigger(StylesheetReloaded {
            page,
            stylesheet_id: sheet.stylesheet_id.clone(),
            reload_count: sheet.reload_count,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use bevy_ecs::prelude::{On, ResMut, Resource};
    use dioxus::prelude::*;

    use crate::Page;
    use crate::host::{PageHostPlugin, PageLifecycle};

    #[component]
    fn StyledView() -> Element {
        rsx! { div { "styled" } }
    }

    static STYLED_PAGE: Page = Page::pane("app://styled/", StyledView).titled("Styled");

    #[derive(Resource, Default)]
    struct ReloadCount(u64);

    fn count_reloads(trigger: On<StylesheetReloaded>, mut count: ResMut<ReloadCount>) {
        count.0 = trigger.event().reload_count;
    }

    #[test]
    fn editing_watched_stylesheet_evaluates_hot_reload_script_and_emits_event() {
        let dir = tempfile::tempdir().unwrap();
        let css_path = dir.path().join("theme.css");
        std::fs::write(&css_path, "body{color:white;}").unwrap();

        let mut app = App::new();
        app.add_plugins((PageHostPlugin::headless(), HotReloadPlugin))
            .init_resource::<ReloadCount>()
            .add_observer(count_reloads);
        let page = app.world_mut().spawn(HostedPage::new(&STYLED_PAGE)).id();
        app.world_mut().spawn((
            HotReloadStylesheet::new("theme"),
            FilePath::from(css_path.as_path()),
            ChildOf(page),
        ));
        app.update();

        std::fs::write(&css_path, "body{color:lime;}").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().resource::<ReloadCount>().0 == 0 {
            app.update();
            assert!(Instant::now() < deadline, "hot reload timed out");
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(app.world().resource::<ReloadCount>().0, 1);
        assert_eq!(
            app.world()
                .get::<PageLifecycle>(page)
                .unwrap()
                .scripts_evaluated,
            1
        );
    }
}
