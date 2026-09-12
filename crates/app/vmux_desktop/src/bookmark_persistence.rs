use bevy::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vmux_core::{Bookmark, BookmarkOrder, Collapsed, Folder, Order, PageMetadata, Pin, Uuid};
use vmux_layout::LayoutStartupSet;

pub(crate) struct BookmarkPersistencePlugin;

impl Plugin for BookmarkPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<OfferedBookmarkDefaults>()
            .init_resource::<OfferedBookmarkDefaults>()
            .init_resource::<BookmarkAutoSave>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_observer(seed_default_bookmarks_after_load)
            .add_systems(
                Startup,
                load_bookmarks_on_startup.after(LayoutStartupSet::Persistence),
            )
            .add_systems(
                PostUpdate,
                (
                    migrate_legacy_bookmark_order,
                    migrate_smart_bookmark_folders,
                    migrate_shortcut_bookmark_aliases,
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
        .allow::<vmux_core::SmartBookmarkFolder>()
        .allow::<Collapsed>()
        .allow::<Uuid>()
        .allow::<BookmarkOrder>()
        .allow::<PageMetadata>()
}

fn bookmark_resource_filter() -> WorldFilter {
    WorldFilter::deny_all().allow::<OfferedBookmarkDefaults>()
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
    save.resources = bookmark_resource_filter();
    commands.trigger_save(save);
}

fn load_bookmarks_on_startup(
    settings: Res<vmux_setting::AppSettings>,
    pins: Query<&PageMetadata, With<Pin>>,
    bookmarks: Query<(Entity, &PageMetadata, Has<Pin>, Option<&ChildOf>), With<Bookmark>>,
    folders: Query<(Entity, &Name, Option<&vmux_core::SmartBookmarkFolder>), With<Folder>>,
    orders: Query<&BookmarkOrder, BookmarkFilter>,
    mut offered: ResMut<OfferedBookmarkDefaults>,
    mut auto: ResMut<BookmarkAutoSave>,
    mut commands: Commands,
) {
    if vmux_core::profile::is_test_session() {
        return;
    }
    let path = bookmarks_path();
    if !path.exists() {
        BookmarkDefaults::of(
            &settings.browser.bookmarks,
            &settings.browser.bookmark_folders,
        )
        .seed(
            &pins,
            &bookmarks,
            &folders,
            &orders,
            &mut offered,
            &mut auto,
            &mut commands,
        );
        return;
    }
    commands.insert_resource(BookmarkLoadPending);
    commands.trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
}

fn seed_default_bookmarks_after_load(
    _trigger: On<Loaded>,
    pending: Option<Res<BookmarkLoadPending>>,
    settings: Res<vmux_setting::AppSettings>,
    pins: Query<&PageMetadata, With<Pin>>,
    bookmarks: Query<(Entity, &PageMetadata, Has<Pin>, Option<&ChildOf>), With<Bookmark>>,
    folders: Query<(Entity, &Name, Option<&vmux_core::SmartBookmarkFolder>), With<Folder>>,
    orders: Query<&BookmarkOrder, BookmarkFilter>,
    mut offered: ResMut<OfferedBookmarkDefaults>,
    mut auto: ResMut<BookmarkAutoSave>,
    mut commands: Commands,
) {
    if pending.is_none() {
        return;
    }
    BookmarkDefaults::of(
        &settings.browser.bookmarks,
        &settings.browser.bookmark_folders,
    )
    .seed(
        &pins,
        &bookmarks,
        &folders,
        &orders,
        &mut offered,
        &mut auto,
        &mut commands,
    );
    commands.remove_resource::<BookmarkLoadPending>();
}

#[derive(Resource)]
struct BookmarkLoadPending;

#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Resource)]
struct OfferedBookmarkDefaults {
    urls: Vec<String>,
    #[reflect(default)]
    folders: Vec<String>,
    #[reflect(default)]
    folder_bookmarks: Vec<String>,
}

impl OfferedBookmarkDefaults {
    fn claim_new_urls(&mut self, defaults: &[String]) -> Vec<String> {
        let mut offered = self
            .urls
            .iter()
            .map(|url| BookmarkDefaults::key(url))
            .collect::<HashSet<_>>();
        let mut claimed = Vec::new();
        for url in defaults {
            let key = BookmarkDefaults::key(url);
            if key.is_empty() || !offered.insert(key) {
                continue;
            }
            self.urls.push(url.clone());
            claimed.push(url.clone());
        }
        claimed
    }

    fn claim_new_folders(
        &mut self,
        defaults: &[vmux_setting::BookmarkFolderSettings],
    ) -> Vec<String> {
        let mut offered = self
            .folders
            .iter()
            .map(|name| BookmarkDefaults::key(name))
            .collect::<HashSet<_>>();
        let mut claimed = Vec::new();
        for folder in defaults {
            let key = BookmarkDefaults::key(&folder.name);
            if key.is_empty() || !offered.insert(key) {
                continue;
            }
            self.folders.push(folder.name.clone());
            claimed.push(folder.name.clone());
        }
        claimed
    }
}

struct BookmarkDefaults<'a> {
    urls: &'a [String],
    folders: &'a [vmux_setting::BookmarkFolderSettings],
}

impl<'a> BookmarkDefaults<'a> {
    fn of(urls: &'a [String], folders: &'a [vmux_setting::BookmarkFolderSettings]) -> Self {
        Self { urls, folders }
    }

    fn key(url: &str) -> String {
        if let Some(canonical) = vmux_shortcut::ShortcutUrl::canonical(url) {
            return canonical.trim_end_matches('/').to_ascii_lowercase();
        }
        url.trim().trim_end_matches('/').to_ascii_lowercase()
    }

    fn seed(
        self,
        pins: &Query<&PageMetadata, With<Pin>>,
        bookmarks: &Query<(Entity, &PageMetadata, Has<Pin>, Option<&ChildOf>), With<Bookmark>>,
        folders: &Query<(Entity, &Name, Option<&vmux_core::SmartBookmarkFolder>), With<Folder>>,
        orders: &Query<&BookmarkOrder, BookmarkFilter>,
        offered: &mut OfferedBookmarkDefaults,
        auto: &mut BookmarkAutoSave,
        commands: &mut Commands,
    ) {
        let claimed_urls = offered.claim_new_urls(self.urls);
        let claimed_folders = offered.claim_new_folders(self.folders);
        let mut changed = !claimed_urls.is_empty() || !claimed_folders.is_empty();
        let mut pinned = pins
            .iter()
            .map(|metadata| Self::key(&metadata.url))
            .collect::<HashSet<_>>();
        let mut next_order = orders
            .iter()
            .map(|order| order.0)
            .max()
            .map_or(0, |order| order.saturating_add(1));
        for url in claimed_urls {
            if !pinned.insert(Self::key(&url)) {
                continue;
            }
            commands.spawn((
                Pin,
                Uuid(uuid::Uuid::new_v4().to_string()),
                PageMetadata {
                    title: url.clone(),
                    url,
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
                BookmarkOrder(next_order),
            ));
            next_order = next_order.saturating_add(1);
        }
        let mut existing_folders = folders
            .iter()
            .map(|(entity, name, smart)| (Self::key(name.as_str()), (entity, smart.copied())))
            .collect::<HashMap<_, _>>();
        for name in claimed_folders {
            let key = Self::key(&name);
            if existing_folders.contains_key(&key) {
                continue;
            }
            let setting = self
                .folders
                .iter()
                .find(|folder| Self::key(&folder.name) == key);
            let mut entity = commands.spawn((
                Folder,
                Uuid(uuid::Uuid::new_v4().to_string()),
                Name::new(name),
                BookmarkOrder(next_order),
            ));
            if let Some(smart) = setting.and_then(|folder| folder.smart) {
                entity.insert(smart);
            }
            let entity = entity.id();
            existing_folders.insert(key, (entity, setting.and_then(|folder| folder.smart)));
            next_order = next_order.saturating_add(1);
        }
        for setting in self.folders {
            let Some(smart) = setting.smart else {
                continue;
            };
            let Some((entity, current)) = existing_folders.get(&Self::key(&setting.name)) else {
                continue;
            };
            if *current != Some(smart) {
                commands.entity(*entity).insert(smart);
                changed = true;
            }
        }
        if !offered.folder_bookmarks.is_empty() {
            let legacy = std::mem::take(&mut offered.folder_bookmarks);
            for encoded in legacy {
                let Some((folder_key, url_key)) = encoded.split_once('\n') else {
                    continue;
                };
                let is_smart = self
                    .folders
                    .iter()
                    .any(|folder| folder.smart.is_some() && Self::key(&folder.name) == folder_key);
                if !is_smart {
                    offered.folder_bookmarks.push(encoded);
                    continue;
                }
                let Some((folder, _)) = existing_folders.get(folder_key) else {
                    changed = true;
                    continue;
                };
                for (entity, metadata, pinned, parent) in bookmarks.iter() {
                    if Self::key(&metadata.url) != url_key
                        || parent.is_none_or(|parent| parent.parent() != *folder)
                    {
                        continue;
                    }
                    if pinned {
                        commands
                            .entity(entity)
                            .remove::<Bookmark>()
                            .remove::<ChildOf>();
                    } else {
                        commands.entity(entity).despawn();
                    }
                }
                changed = true;
            }
        }
        if changed {
            auto.dirty = true;
        }
    }
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

fn migrate_smart_bookmark_folders(
    folders: Query<(Entity, Option<&Children>), With<vmux_core::SmartBookmarkFolder>>,
    mut offered: ResMut<OfferedBookmarkDefaults>,
    mut auto: ResMut<BookmarkAutoSave>,
    mut commands: Commands,
) {
    let mut changed = false;
    for (folder, children) in &folders {
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).remove::<ChildOf>();
            }
        }
        commands.entity(folder).despawn();
        changed = true;
    }
    if !offered.folder_bookmarks.is_empty() {
        offered.folder_bookmarks.clear();
        changed = true;
    }
    if changed {
        auto.dirty = true;
    }
}

fn migrate_shortcut_bookmark_aliases(
    items: Query<
        (
            Entity,
            &PageMetadata,
            Has<Pin>,
            Has<Bookmark>,
            Option<&ChildOf>,
            Option<&BookmarkOrder>,
        ),
        Or<(With<Pin>, With<Bookmark>)>,
    >,
    mut offered: ResMut<OfferedBookmarkDefaults>,
    mut auto: ResMut<BookmarkAutoSave>,
    mut commands: Commands,
) {
    let mut changed = false;
    let mut seen = HashSet::new();
    let mut normalized_urls = Vec::new();
    for url in &offered.urls {
        let normalized = vmux_shortcut::ShortcutUrl::canonical(url)
            .unwrap_or(url)
            .to_string();
        if seen.insert(BookmarkDefaults::key(&normalized)) {
            normalized_urls.push(normalized);
        }
    }
    if offered.urls != normalized_urls {
        offered.urls = normalized_urls;
        changed = true;
    }

    let mut aliases = items
        .iter()
        .filter(|(_, metadata, _, _, _, _)| {
            vmux_shortcut::ShortcutUrl::canonical(&metadata.url).is_some()
        })
        .map(|(entity, metadata, pinned, bookmarked, parent, order)| {
            (
                entity,
                metadata.clone(),
                pinned,
                bookmarked,
                parent.map(ChildOf::parent),
                order.map_or(u32::MAX, |order| order.0),
            )
        })
        .collect::<Vec<_>>();
    aliases.sort_by_key(|(entity, _, _, _, _, order)| (*order, entity.to_bits()));
    let Some((survivor, mut metadata, survivor_pinned, survivor_bookmarked, survivor_parent, _)) =
        aliases.first().cloned()
    else {
        if changed {
            auto.dirty = true;
        }
        return;
    };
    let original_metadata = metadata.clone();
    let original_url = metadata.url.clone();
    if metadata.title.trim() == original_url.trim() {
        metadata.title = vmux_shortcut::PAGE_URL.to_string();
    }
    if metadata.url != vmux_shortcut::PAGE_URL {
        metadata.url = vmux_shortcut::PAGE_URL.to_string();
    }
    let pinned = aliases.iter().any(|(_, _, pinned, _, _, _)| *pinned);
    let bookmarked = aliases
        .iter()
        .any(|(_, _, _, bookmarked, _, _)| *bookmarked);
    let parent = aliases.iter().find_map(|(_, _, _, _, parent, _)| *parent);
    let mut survivor_commands = commands.entity(survivor);
    if metadata != original_metadata {
        survivor_commands.insert(metadata);
        changed = true;
    }
    if pinned && !survivor_pinned {
        survivor_commands.insert(Pin);
        changed = true;
    }
    if bookmarked && !survivor_bookmarked {
        survivor_commands.insert(Bookmark);
        changed = true;
    }
    if parent != survivor_parent
        && let Some(parent) = parent
    {
        survivor_commands.insert(ChildOf(parent));
        changed = true;
    }
    for (entity, _, _, _, _, _) in aliases.into_iter().skip(1) {
        commands.entity(entity).despawn();
        changed = true;
    }
    if changed {
        auto.dirty = true;
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
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<OfferedBookmarkDefaults>()
            .insert_resource(OfferedBookmarkDefaults {
                urls: vec!["vmux://start/".into()],
                ..default()
            })
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app.world_mut().spawn((
            Folder,
            Uuid("f1".into()),
            Name::new("PRs"),
            BookmarkOrder(0),
        ));
        save_app.world_mut().spawn((
            Bookmark,
            Uuid("b1".into()),
            PageMetadata {
                title: "A".into(),
                url: "https://a.test".into(),
                icon: vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Smartphone),
                bg_color: None,
            },
            BookmarkOrder(1),
        ));
        save_app
            .world_mut()
            .spawn((Save, Name::new("excluded-save-entity")));
        let p = path.clone();
        save_app.add_systems(Update, move |mut c: Commands| {
            let mut s = SaveWorld::<BookmarkFilter>::into_file(p.clone());
            s.components = bookmark_scene_filter();
            s.resources = bookmark_resource_filter();
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
            .add_plugins(vmux_core::CorePlugin)
            .register_type::<OfferedBookmarkDefaults>()
            .init_resource::<OfferedBookmarkDefaults>()
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
        let p2 = path.clone();
        load_app.add_systems(Update, move |mut c: Commands| {
            c.trigger_load(LoadWorld::<BookmarkFilter>::from_file(p2.clone()));
        });
        load_app.update();
        load_app.update();

        let bookmarks = load_app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .iter(load_app.world())
            .cloned()
            .collect::<Vec<_>>();
        let folders = load_app
            .world_mut()
            .query_filtered::<Entity, With<Folder>>()
            .iter(load_app.world())
            .count();
        assert_eq!(bookmarks.len(), 1, "bookmark rebuilt");
        assert_eq!(
            bookmarks[0].icon,
            vmux_core::PageIcon::Builtin(vmux_core::BuiltinIcon::Smartphone)
        );
        assert_eq!(folders, 1, "folder rebuilt");
        let excluded = load_app
            .world_mut()
            .query::<&Name>()
            .iter(load_app.world())
            .any(|name| name.as_str() == "excluded-save-entity");
        assert!(!excluded, "Save-only entity excluded");
        assert_eq!(
            load_app.world().resource::<OfferedBookmarkDefaults>().urls,
            ["vmux://start/"]
        );
        assert!(
            load_app
                .world()
                .resource::<OfferedBookmarkDefaults>()
                .folders
                .is_empty()
        );
        assert!(
            load_app
                .world()
                .resource::<OfferedBookmarkDefaults>()
                .folder_bookmarks
                .is_empty()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removed_default_bookmarks_are_not_offered_again() {
        let defaults = vec!["vmux://start/".into(), "vmux://terminal/".into()];
        let mut offered = OfferedBookmarkDefaults::default();

        assert_eq!(offered.claim_new_urls(&defaults), defaults);
        assert!(offered.claim_new_urls(&defaults).is_empty());
    }

    #[test]
    fn shortcut_alias_bookmarks_migrate_to_one_canonical_entry() {
        let mut app = App::new();
        app.insert_resource(OfferedBookmarkDefaults {
            urls: vec!["vmux://cheatsheet/".into(), vmux_shortcut::PAGE_URL.into()],
            ..default()
        })
        .init_resource::<BookmarkAutoSave>()
        .add_systems(Update, migrate_shortcut_bookmark_aliases);
        let survivor = app
            .world_mut()
            .spawn((
                Pin,
                PageMetadata {
                    title: "vmux://cheatsheet/".into(),
                    url: "vmux://cheatsheet/".into(),
                    ..default()
                },
                BookmarkOrder(2),
            ))
            .id();
        let duplicate = app
            .world_mut()
            .spawn((
                Bookmark,
                PageMetadata {
                    title: "Keyboard Shortcuts".into(),
                    url: vmux_shortcut::PAGE_URL.into(),
                    ..default()
                },
                BookmarkOrder(8),
            ))
            .id();

        app.update();

        let entity = app.world().entity(survivor);
        assert!(entity.contains::<Pin>());
        assert!(entity.contains::<Bookmark>());
        assert_eq!(
            entity.get::<PageMetadata>().unwrap().url,
            vmux_shortcut::PAGE_URL
        );
        assert!(app.world().get_entity(duplicate).is_err());
        assert_eq!(
            app.world().resource::<OfferedBookmarkDefaults>().urls,
            [vmux_shortcut::PAGE_URL]
        );
        assert!(app.world().resource::<BookmarkAutoSave>().dirty);

        app.world_mut().resource_mut::<BookmarkAutoSave>().dirty = false;
        app.update();

        assert!(!app.world().resource::<BookmarkAutoSave>().dirty);
    }

    #[test]
    fn removed_default_folders_are_not_offered_again() {
        let defaults = vec![
            vmux_setting::BookmarkFolderSettings {
                name: "Projects".into(),
                smart: Some(vmux_core::SmartBookmarkFolder::Projects),
            },
            vmux_setting::BookmarkFolderSettings {
                name: "Knowledge".into(),
                smart: Some(vmux_core::SmartBookmarkFolder::Knowledge),
            },
        ];
        let mut offered = OfferedBookmarkDefaults::default();

        assert_eq!(
            offered.claim_new_folders(&defaults),
            ["Projects", "Knowledge"]
        );
        assert!(offered.claim_new_folders(&defaults).is_empty());
    }

    #[test]
    fn existing_bookmark_store_receives_missing_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bookmarks.ron");
        let mut save_app = App::new();
        save_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(vmux_core::CorePlugin)
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app.world_mut().spawn((
            Pin,
            Uuid("existing".into()),
            PageMetadata {
                title: "Existing".into(),
                url: "https://example.com".into(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(4),
        ));
        let save_path = path.clone();
        save_app.add_systems(Update, move |mut commands: Commands| {
            let mut save = SaveWorld::<BookmarkFilter>::into_file(save_path.clone());
            save.components = bookmark_scene_filter();
            commands.trigger_save(save);
        });
        save_app.update();
        save_app.update();

        let mut settings = vmux_setting::AppSettings::embedded();
        settings.browser.bookmarks = vec!["vmux://start/".into(), "vmux://terminal/".into()];
        settings.browser.bookmark_folders = vec![
            vmux_setting::BookmarkFolderSettings {
                name: "Projects".into(),
                smart: Some(vmux_core::SmartBookmarkFolder::Projects),
            },
            vmux_setting::BookmarkFolderSettings {
                name: "Knowledge".into(),
                smart: Some(vmux_core::SmartBookmarkFolder::Knowledge),
            },
        ];
        let mut load_app = App::new();
        load_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(vmux_core::CorePlugin)
            .insert_resource(settings)
            .register_type::<OfferedBookmarkDefaults>()
            .init_resource::<OfferedBookmarkDefaults>()
            .init_resource::<BookmarkAutoSave>()
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_observer(seed_default_bookmarks_after_load);
        load_app.world_mut().insert_resource(BookmarkLoadPending);
        load_app
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
        load_app.update();
        load_app.update();

        let mut pins = load_app
            .world_mut()
            .query_filtered::<(&PageMetadata, &BookmarkOrder), With<Pin>>()
            .iter(load_app.world())
            .map(|(metadata, order)| (metadata.url.clone(), order.0))
            .collect::<Vec<_>>();
        pins.sort();
        assert_eq!(
            pins,
            [
                ("https://example.com".into(), 4),
                ("vmux://start/".into(), 5),
                ("vmux://terminal/".into(), 6),
            ]
        );
        assert_eq!(
            load_app.world().resource::<OfferedBookmarkDefaults>().urls,
            ["vmux://start/", "vmux://terminal/"]
        );
        let mut folders = load_app
            .world_mut()
            .query_filtered::<&Name, With<Folder>>()
            .iter(load_app.world())
            .map(|name| name.as_str().to_string())
            .collect::<Vec<_>>();
        folders.sort();
        assert_eq!(folders, ["Knowledge", "Projects"]);
        assert_eq!(
            load_app
                .world()
                .resource::<OfferedBookmarkDefaults>()
                .folders,
            ["Projects", "Knowledge"]
        );
        let mut smart_folders = load_app
            .world_mut()
            .query::<(&Name, &vmux_core::SmartBookmarkFolder)>()
            .iter(load_app.world())
            .map(|(name, smart)| (name.as_str().to_string(), *smart))
            .collect::<Vec<_>>();
        smart_folders.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            smart_folders,
            [
                (
                    "Knowledge".into(),
                    vmux_core::SmartBookmarkFolder::Knowledge
                ),
                ("Projects".into(), vmux_core::SmartBookmarkFolder::Projects),
            ]
        );
        assert!(
            load_app
                .world()
                .resource::<OfferedBookmarkDefaults>()
                .folder_bookmarks
                .is_empty()
        );
    }

    #[test]
    fn smart_folder_migration_moves_children_to_bookmark_root() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .insert_resource(OfferedBookmarkDefaults {
                folders: vec!["Projects".into()],
                folder_bookmarks: vec!["projects\nvmux://projects".into()],
                ..default()
            })
            .init_resource::<BookmarkAutoSave>()
            .add_systems(Update, migrate_smart_bookmark_folders);
        let bookmark = app
            .world_mut()
            .spawn((
                Bookmark,
                Uuid("project-bookmark".into()),
                PageMetadata {
                    title: "Projects".into(),
                    url: "vmux://projects/".into(),
                    ..default()
                },
                BookmarkOrder(0),
            ))
            .id();
        let folder = app
            .world_mut()
            .spawn((
                Folder,
                Uuid("projects-folder".into()),
                Name::new("Projects"),
                BookmarkOrder(1),
                vmux_core::SmartBookmarkFolder::Projects,
            ))
            .id();
        app.world_mut().entity_mut(bookmark).insert(ChildOf(folder));
        app.update();

        assert!(app.world().get_entity(folder).is_err());
        assert!(app.world().entity(bookmark).contains::<Bookmark>());
        assert!(!app.world().entity(bookmark).contains::<ChildOf>());
        assert!(
            app.world()
                .resource::<OfferedBookmarkDefaults>()
                .folder_bookmarks
                .is_empty()
        );
        assert!(app.world().resource::<BookmarkAutoSave>().dirty);
    }

    #[test]
    fn legacy_bookmark_order_migration_removes_space_save_marker() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(vmux_core::CorePlugin)
            .add_systems(Update, migrate_legacy_bookmark_order);
        let entity = app
            .world_mut()
            .spawn((Bookmark, Uuid("b1".into()), Order(3)))
            .id();
        assert!(app.world().get::<Save>(entity).is_some());

        app.update();

        assert_eq!(
            app.world().get::<BookmarkOrder>(entity),
            Some(&BookmarkOrder(3))
        );
        assert!(app.world().get::<Order>(entity).is_none());
        assert!(app.world().get::<Save>(entity).is_none());
    }
}
