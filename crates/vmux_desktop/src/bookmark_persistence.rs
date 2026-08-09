use bevy::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use std::path::PathBuf;
use vmux_core::{Bookmark, BookmarkOrder, Collapsed, Folder, Order, PageMetadata, Pin, Uuid};
use vmux_layout::LayoutStartupSet;

pub(crate) struct BookmarkPersistencePlugin;

impl Plugin for BookmarkPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BookmarkAutoSave>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_systems(
                Startup,
                load_bookmarks_on_startup.after(LayoutStartupSet::Persistence),
            )
            .add_systems(
                PostUpdate,
                (
                    migrate_legacy_bookmark_order,
                    mark_bookmarks_dirty,
                    autosave_bookmarks,
                )
                    .chain(),
            );
    }
}

type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

pub(crate) fn bookmarks_path() -> PathBuf {
    vmux_core::profile::profile_dir().join("bookmarks.ron")
}

fn bookmark_scene_filter() -> WorldFilter {
    WorldFilter::deny_all()
        .allow::<ChildOf>()
        .allow::<Children>()
        .allow::<Name>()
        .allow::<Pin>()
        .allow::<Bookmark>()
        .allow::<Folder>()
        .allow::<Collapsed>()
        .allow::<Uuid>()
        .allow::<BookmarkOrder>()
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

#[derive(Resource, Default)]
struct BookmarkAutoSave {
    dirty: bool,
}

fn migrate_legacy_bookmark_order(
    legacy: Query<(Entity, &Order), (BookmarkFilter, Without<BookmarkOrder>)>,
    mut commands: Commands,
) {
    for (entity, order) in &legacy {
        commands
            .entity(entity)
            .insert(BookmarkOrder(order.0))
            .remove::<Order>()
            .remove::<Save>();
    }
}

fn mark_bookmarks_dirty(
    mut auto: ResMut<BookmarkAutoSave>,
    changed: Query<
        (),
        (
            BookmarkFilter,
            Or<(
                Added<Pin>,
                Added<Bookmark>,
                Added<Folder>,
                Added<Collapsed>,
                Changed<Name>,
                Changed<BookmarkOrder>,
                Changed<PageMetadata>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    bookmark_items: Query<(), Or<(With<Bookmark>, With<Pin>, With<Folder>)>>,
    mut removed_pin: RemovedComponents<Pin>,
    mut removed_bookmark: RemovedComponents<Bookmark>,
    mut removed_folder: RemovedComponents<Folder>,
    mut removed_collapsed: RemovedComponents<Collapsed>,
    mut removed_child_of: RemovedComponents<ChildOf>,
) {
    let removed_child_of_bookmark = removed_child_of
        .read()
        .any(|entity| bookmark_items.get(entity).is_ok());
    let any_removed = removed_pin.read().next().is_some()
        | removed_bookmark.read().next().is_some()
        | removed_folder.read().next().is_some()
        | removed_collapsed.read().next().is_some()
        | removed_child_of_bookmark;
    if any_removed || !changed.is_empty() {
        auto.dirty = true;
    }
}

fn autosave_bookmarks(mut auto: ResMut<BookmarkAutoSave>, mut commands: Commands) {
    if !auto.dirty {
        return;
    }
    save_bookmarks_to_path(&mut commands, bookmarks_path());
    auto.dirty = false;
}

#[cfg(test)]
#[path = "bookmark_persistence.test.rs"]
mod tests;
