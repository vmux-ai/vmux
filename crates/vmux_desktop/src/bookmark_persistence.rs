use bevy::prelude::*;
use moonshine_save::prelude::*;
use std::path::PathBuf;
use vmux_core::{Bookmark, Collapsed, Folder, Order, PageMetadata, Pin, Uuid};

type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

pub(crate) fn bookmarks_path() -> PathBuf {
    vmux_core::profile::profile_dir().join("bookmarks.ron")
}

fn bookmark_scene_filter() -> SceneFilter {
    SceneFilter::deny_all()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>()
        .allow::<Pin>()
        .allow::<Bookmark>()
        .allow::<Folder>()
        .allow::<Collapsed>()
        .allow::<Uuid>()
        .allow::<Order>()
        .allow::<PageMetadata>()
}

fn save_bookmarks_to_path(commands: &mut Commands, path: PathBuf) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut save = SaveWorld::<BookmarkFilter>::into_file(path);
    save.components = bookmark_scene_filter();
    commands.trigger_save(save);
}

fn load_bookmarks_on_startup(mut commands: Commands) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    let path = bookmarks_path();
    if !path.exists() {
        return;
    }
    commands.trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
}

#[derive(Resource)]
struct BookmarkAutoSave {
    debounce: Timer,
    dirty: bool,
}

impl Default for BookmarkAutoSave {
    fn default() -> Self {
        Self {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            dirty: false,
        }
    }
}

fn mark_bookmarks_dirty(
    mut auto: ResMut<BookmarkAutoSave>,
    changed: Query<
        (),
        Or<(
            Added<Pin>,
            Added<Bookmark>,
            Added<Folder>,
            Added<Collapsed>,
            Changed<Name>,
            Changed<Order>,
            Changed<PageMetadata>,
        )>,
    >,
    mut removed_pin: RemovedComponents<Pin>,
    mut removed_bookmark: RemovedComponents<Bookmark>,
    mut removed_folder: RemovedComponents<Folder>,
    mut removed_collapsed: RemovedComponents<Collapsed>,
) {
    let any_removed = removed_pin.read().next().is_some()
        | removed_bookmark.read().next().is_some()
        | removed_folder.read().next().is_some()
        | removed_collapsed.read().next().is_some();
    if any_removed || !changed.is_empty() {
        auto.dirty = true;
        auto.debounce.reset();
    }
}

fn autosave_bookmarks(time: Res<Time>, mut auto: ResMut<BookmarkAutoSave>, mut commands: Commands) {
    if !auto.dirty {
        return;
    }
    auto.debounce.tick(time.delta());
    if auto.debounce.is_finished() {
        save_bookmarks_to_path(&mut commands, bookmarks_path());
        auto.dirty = false;
    }
}

pub(crate) struct BookmarkPersistencePlugin;

impl Plugin for BookmarkPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BookmarkAutoSave>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_systems(Startup, load_bookmarks_on_startup)
            .add_systems(Update, (mark_bookmarks_dirty, autosave_bookmarks).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips_bookmarks_and_excludes_save_entities() {
        let dir = std::env::temp_dir().join(format!("vmux-bm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bookmarks.ron");

        let mut save_app = App::new();
        save_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy::scene::ScenePlugin)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app
            .world_mut()
            .spawn((Folder, Uuid("f1".into()), Name::new("PRs"), Order(0)));
        save_app.world_mut().spawn((
            Bookmark,
            Uuid("b1".into()),
            PageMetadata {
                title: "A".into(),
                url: "https://a.test".into(),
                icon: vmux_core::icon::PageIcon::default(),
                bg_color: None,
            },
            Order(1),
        ));
        save_app.world_mut().spawn(Save);
        let p = path.clone();
        save_app.add_systems(Update, move |mut c: Commands| {
            let mut s = SaveWorld::<BookmarkFilter>::into_file(p.clone());
            s.components = bookmark_scene_filter();
            c.trigger_save(s);
        });
        save_app.update();
        save_app.update();

        assert!(path.exists(), "bookmarks.ron written");
        let ron = std::fs::read_to_string(&path).unwrap();
        assert!(ron.contains("b1"), "bookmark uuid persisted");
        assert!(ron.contains("PRs"), "folder name persisted");

        let mut load_app = App::new();
        load_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy::scene::ScenePlugin)
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
        let p2 = path.clone();
        load_app.add_systems(Update, move |mut c: Commands| {
            c.trigger_load(LoadWorld::<BookmarkFilter>::from_file(p2.clone()));
        });
        load_app.update();
        load_app.update();

        let bookmarks = load_app
            .world_mut()
            .query_filtered::<Entity, With<Bookmark>>()
            .iter(load_app.world())
            .count();
        let folders = load_app
            .world_mut()
            .query_filtered::<Entity, With<Folder>>()
            .iter(load_app.world())
            .count();
        assert_eq!(bookmarks, 1, "bookmark rebuilt");
        assert_eq!(folders, 1, "folder rebuilt");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
