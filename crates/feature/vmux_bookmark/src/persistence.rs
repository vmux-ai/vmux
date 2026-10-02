use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vmux_api::bookmark::SmartBookmarkFolder;
use vmux_ecs::profile::{ProfilePaths, is_test_session};
use vmux_ecs::{Bookmark, BookmarkOrder, Collapsed, Folder, PageIcon, PageMetadata, Pin, Uuid};
use vmux_layout::LayoutStartupSet;
use vmux_setting::{AppSettings, BookmarkFolderSettings};
use vmux_shortcut::ShortcutUrl;

pub(super) struct BookmarkPersistencePlugin;

impl Plugin for BookmarkPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<OfferedBookmarkDefaults>()
            .init_resource::<OfferedBookmarkDefaults>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_observer(seed_defaults)
            .add_observer(insert_defaults)
            .add_systems(
                Startup,
                (spawn, ApplyDeferred, load_bookmarks_on_startup)
                    .chain()
                    .after(LayoutStartupSet::Persistence),
            )
            .add_systems(
                PostUpdate,
                (mark_bookmarks_dirty, autosave_bookmarks).chain(),
            );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Bookmark persistence"),
        BookmarkAutoSave::default(),
    ));
}

type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

#[derive(Component)]
struct BookmarkPersistencePath(PathBuf);

impl Default for BookmarkPersistencePath {
    fn default() -> Self {
        Self(ProfilePaths::current().profile().join("bookmarks.ron"))
    }
}

impl BookmarkPersistencePath {
    fn scene_filter() -> WorldFilter {
        WorldFilter::deny_all()
            .allow::<ChildOf>()
            .allow::<Children>()
            .allow::<Name>()
            .allow::<Pin>()
            .allow::<Bookmark>()
            .allow::<Folder>()
            .allow::<SmartBookmarkFolder>()
            .allow::<Collapsed>()
            .allow::<Uuid>()
            .allow::<BookmarkOrder>()
            .allow::<PageMetadata>()
    }

    fn resource_filter() -> WorldFilter {
        WorldFilter::deny_all().allow::<OfferedBookmarkDefaults>()
    }
}

fn load_bookmarks_on_startup(
    persistence: Single<Entity, With<BookmarkAutoSave>>,
    path: Single<&BookmarkPersistencePath>,
    mut commands: Commands,
) {
    if is_test_session() {
        return;
    }
    let path = path.0.clone();
    if !path.exists() {
        commands.trigger(SeedBookmarkDefaults);
        return;
    }
    commands.entity(*persistence).insert(BookmarkLoadPending);
    commands.trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
}

fn seed_defaults(
    _trigger: On<Loaded>,
    pending: Option<Single<Entity, With<BookmarkLoadPending>>>,
    mut commands: Commands,
) {
    let Some(pending) = pending else {
        return;
    };
    commands.trigger(SeedBookmarkDefaults);
    commands.entity(*pending).remove::<BookmarkLoadPending>();
}

#[derive(Event)]
struct SeedBookmarkDefaults;

#[derive(Component)]
struct BookmarkLoadPending;

#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Resource)]
struct OfferedBookmarkDefaults {
    urls: Vec<String>,
    #[reflect(default)]
    folders: Vec<String>,
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

    fn claim_new_folders(&mut self, defaults: &[BookmarkFolderSettings]) -> Vec<String> {
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
    folders: &'a [BookmarkFolderSettings],
}

impl<'a> BookmarkDefaults<'a> {
    fn new(urls: &'a [String], folders: &'a [BookmarkFolderSettings]) -> Self {
        Self { urls, folders }
    }

    fn key(url: &str) -> String {
        if let Some(canonical) = ShortcutUrl::canonical(url) {
            return canonical.trim_end_matches('/').to_ascii_lowercase();
        }
        url.trim().trim_end_matches('/').to_ascii_lowercase()
    }
}

#[derive(SystemParam)]
struct BookmarkSeed<'w, 's> {
    pins: Query<'w, 's, &'static PageMetadata, With<Pin>>,
    folders:
        Query<'w, 's, (Entity, &'static Name, Option<&'static SmartBookmarkFolder>), With<Folder>>,
    orders: Query<'w, 's, &'static BookmarkOrder, BookmarkFilter>,
    offered: ResMut<'w, OfferedBookmarkDefaults>,
    auto: Single<'w, 's, &'static mut BookmarkAutoSave>,
    commands: Commands<'w, 's>,
}

fn insert_defaults(
    _trigger: On<SeedBookmarkDefaults>,
    settings: Res<AppSettings>,
    mut seed: BookmarkSeed,
) {
    let defaults = BookmarkDefaults::new(
        &settings.browser.bookmarks,
        &settings.browser.bookmark_folders,
    );
    let claimed_urls = seed.offered.claim_new_urls(defaults.urls);
    let claimed_folders = seed.offered.claim_new_folders(defaults.folders);
    let mut changed = !claimed_urls.is_empty() || !claimed_folders.is_empty();
    let mut pinned = seed
        .pins
        .iter()
        .map(|metadata| BookmarkDefaults::key(&metadata.url))
        .collect::<HashSet<_>>();
    let mut next_order = seed
        .orders
        .iter()
        .map(|order| order.0)
        .max()
        .map_or(0, |order| order.saturating_add(1));
    for url in claimed_urls {
        if !pinned.insert(BookmarkDefaults::key(&url)) {
            continue;
        }
        seed.commands.spawn((
            Pin,
            Uuid(uuid::Uuid::new_v4().to_string()),
            PageMetadata {
                title: url.clone(),
                url,
                icon: PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(next_order),
        ));
        next_order = next_order.saturating_add(1);
    }
    let mut existing_folders = seed
        .folders
        .iter()
        .map(|(entity, name, smart)| {
            (
                BookmarkDefaults::key(name.as_str()),
                (entity, smart.copied()),
            )
        })
        .collect::<HashMap<_, _>>();
    for name in claimed_folders {
        let key = BookmarkDefaults::key(&name);
        if existing_folders.contains_key(&key) {
            continue;
        }
        let setting = defaults
            .folders
            .iter()
            .find(|folder| BookmarkDefaults::key(&folder.name) == key);
        let mut entity = seed.commands.spawn((
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
    for setting in defaults.folders {
        let Some(smart) = setting.smart else {
            continue;
        };
        let Some((entity, current)) = existing_folders.get(&BookmarkDefaults::key(&setting.name))
        else {
            continue;
        };
        if *current != Some(smart) {
            seed.commands.entity(*entity).insert(smart);
            changed = true;
        }
    }
    if changed {
        seed.auto.dirty = true;
    }
}

#[derive(Component, Default)]
#[require(BookmarkPersistencePath)]
struct BookmarkAutoSave {
    dirty: bool,
}

fn mark_bookmarks_dirty(
    mut auto: Single<&mut BookmarkAutoSave>,
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

fn autosave_bookmarks(
    mut auto: Single<&mut BookmarkAutoSave>,
    path: Single<&BookmarkPersistencePath>,
    mut commands: Commands,
) {
    if !auto.dirty {
        return;
    }
    if !is_test_session() {
        if let Some(parent) = path.0.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut save = SaveWorld::<BookmarkFilter>::into_file(path.0.clone());
        save.components = BookmarkPersistencePath::scene_filter();
        save.resources = BookmarkPersistencePath::resource_filter();
        commands.trigger_save(save);
    }
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
            .add_plugins(vmux_ecs::EcsPlugin)
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
                icon: vmux_ecs::PageIcon::Builtin(vmux_ecs::BuiltinIcon::Smartphone),
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
            s.components = BookmarkPersistencePath::scene_filter();
            s.resources = BookmarkPersistencePath::resource_filter();
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
            .add_plugins(vmux_ecs::EcsPlugin)
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
            vmux_ecs::PageIcon::Builtin(vmux_ecs::BuiltinIcon::Smartphone)
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
    fn removed_default_folders_are_not_offered_again() {
        let defaults = vec![
            vmux_setting::BookmarkFolderSettings {
                name: "Projects".into(),
                smart: Some(SmartBookmarkFolder::Projects),
            },
            vmux_setting::BookmarkFolderSettings {
                name: "Knowledge".into(),
                smart: Some(SmartBookmarkFolder::Knowledge),
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
            .add_plugins(vmux_ecs::EcsPlugin)
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app.world_mut().spawn((
            Pin,
            Uuid("existing".into()),
            PageMetadata {
                title: "Existing".into(),
                url: "https://example.com".into(),
                icon: vmux_ecs::PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(4),
        ));
        let save_path = path.clone();
        save_app.add_systems(Update, move |mut commands: Commands| {
            let mut save = SaveWorld::<BookmarkFilter>::into_file(save_path.clone());
            save.components = BookmarkPersistencePath::scene_filter();
            commands.trigger_save(save);
        });
        save_app.update();
        save_app.update();

        let mut settings = vmux_setting::AppSettings::default();
        settings.browser.bookmarks = vec!["vmux://start/".into(), "vmux://terminal/".into()];
        settings.browser.bookmark_folders = vec![
            vmux_setting::BookmarkFolderSettings {
                name: "Projects".into(),
                smart: Some(SmartBookmarkFolder::Projects),
            },
            vmux_setting::BookmarkFolderSettings {
                name: "Knowledge".into(),
                smart: Some(SmartBookmarkFolder::Knowledge),
            },
        ];
        let mut load_app = App::new();
        load_app
            .world_mut()
            .spawn((BookmarkAutoSave::default(), BookmarkLoadPending));
        load_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(vmux_ecs::EcsPlugin)
            .insert_resource(settings)
            .register_type::<OfferedBookmarkDefaults>()
            .init_resource::<OfferedBookmarkDefaults>()
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_observer(seed_defaults)
            .add_observer(insert_defaults);
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
            .query::<(&Name, &SmartBookmarkFolder)>()
            .iter(load_app.world())
            .map(|(name, smart)| (name.as_str().to_string(), *smart))
            .collect::<Vec<_>>();
        smart_folders.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            smart_folders,
            [
                ("Knowledge".into(), SmartBookmarkFolder::Knowledge),
                ("Projects".into(), SmartBookmarkFolder::Projects),
            ]
        );
    }
}
