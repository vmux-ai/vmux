use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use moonshine_save::prelude::*;
use std::collections::{HashMap, HashSet};
use vmux_api::PageIcon;
use vmux_api::bookmark::SmartBookmarkFolder;
use vmux_ecs::profile::SessionEnvironment;
use vmux_ecs::store::State;
use vmux_ecs::store::{SchemaPlugin, Types};
use vmux_ecs::{Bookmark, BookmarkOrder, Collapsed, Folder, PageMetadata, Pin, Uuid};
use vmux_layout::LayoutStartupSet;
#[cfg(test)]
use vmux_setting::AppSettings;
use vmux_setting::BookmarkDefaults;

use crate::store_driver::{BookmarkFilter, BookmarkStorePath};

pub(super) struct BookmarkStorePlugin;

impl Plugin for BookmarkStorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SchemaPlugin::<BookmarkStore>::default())
            .init_resource::<OfferedBookmarkDefaults>()
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
            .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
            .add_observer(seed_defaults)
            .add_observer(insert_defaults)
            .add_systems(
                Startup,
                (spawn, ApplyDeferred, load_bookmarks_on_startup)
                    .chain()
                    .after(LayoutStartupSet::Store),
            )
            .add_systems(
                PostUpdate,
                (mark_bookmarks_dirty, autosave_bookmarks).chain(),
            );
    }
}

const RETIRED_DEFAULT_BOOKMARKS: [&str; 2] = ["vmux://spaces/", "vmux://team/"];

#[vmux_ecs::store(
    ChildOf,
    Children,
    Name,
    BookmarkOrder,
    Pin,
    Bookmark,
    Folder,
    SmartBookmarkFolder,
    Collapsed,
    Uuid,
    PageMetadata,
    OfferedBookmarkDefaults
)]
struct BookmarkStore;

type BookmarkStoreTypes = Types<BookmarkStore>;

fn spawn(profile: vmux_ecs::profile::CurrentProfile, mut commands: Commands) {
    let path = match profile.paths() {
        Some(paths) => paths.profile().join("bookmarks.ron"),
        None => {
            #[cfg(test)]
            {
                vmux_ecs::profile::ProfilePaths::current()
                    .profile()
                    .join("bookmarks.ron")
            }
            #[cfg(not(test))]
            {
                return;
            }
        }
    };
    commands.spawn((
        Name::new("Bookmark store"),
        BookmarkAutoSave,
        BookmarkStorePath(path),
    ));
}

fn load_bookmarks_on_startup(
    store: Single<Entity, With<BookmarkAutoSave>>,
    mut state: Single<&mut State, With<BookmarkAutoSave>>,
    path: Single<&BookmarkStorePath>,
    mut commands: Commands,
) {
    if SessionEnvironment::is_test() {
        return;
    }
    if !path.0.exists() {
        commands.trigger(SeedBookmarkDefaults);
        return;
    }
    let load = match path.load() {
        Ok(load) => load,
        Err(error) => {
            error!(path = %path.0.display(), %error, "bookmark load failed");
            state.fail(error.to_string());
            return;
        }
    };
    state.materialize();
    commands.entity(*store).insert(BookmarkLoadPending);
    commands.trigger_load(load);
}

fn seed_defaults(
    _trigger: On<Loaded>,
    pending: Option<Single<Entity, With<BookmarkLoadPending>>>,
    mut state: Single<&mut State, With<BookmarkAutoSave>>,
    mut commands: Commands,
) {
    let Some(pending) = pending else {
        return;
    };
    state.ready();
    commands.trigger(SeedBookmarkDefaults);
    commands.entity(*pending).remove::<BookmarkLoadPending>();
}

#[derive(Event)]
struct SeedBookmarkDefaults;

#[derive(Component)]
struct BookmarkLoadPending;

#[derive(Resource, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Resource)]
#[type_path = "vmux_bookmark"]
pub(super) struct OfferedBookmarkDefaults {
    urls: Vec<String>,
    #[reflect(default)]
    pin_order_revision: u32,
    #[reflect(default)]
    vm_root_revision: u32,
    #[reflect(default)]
    folders: Vec<String>,
    #[reflect(default)]
    folder_bookmarks: Vec<String>,
}

impl BookmarkSeed<'_, '_> {
    const PIN_ORDER_REVISION: u32 = 5;
    const LEGACY_PIN_ORDER_REVISION: u32 = 4;

    fn move_vm_default(&mut self, urls: &[String]) -> HashSet<Entity> {
        let mut migrated = HashSet::new();
        let vm = Self::key("vmux://vm/");
        if self.offered.vm_root_revision > 0 || !urls.iter().any(|url| Self::key(url) == vm) {
            return migrated;
        }
        let containers = Self::key("vmux://vm/docker/containers");
        for url in &mut self.offered.urls {
            if Self::key(url) == containers {
                *url = "vmux://vm/".into();
            }
        }
        for (entity, metadata, _) in &self.pins {
            if Self::key(&metadata.url) != containers {
                continue;
            }
            let mut metadata = metadata.clone();
            metadata.url = "vmux://vm/".into();
            metadata.title = "VM".into();
            metadata.icon = PageIcon::Builtin(vmux_api::BuiltinIcon::Computer);
            self.commands.entity(entity).insert(metadata);
            migrated.insert(entity);
        }
        self.offered.vm_root_revision = 1;
        migrated
    }

    fn key(url: &str) -> String {
        if let Some(canonical) = vmux_api::VmuxRoute::canonical(url) {
            return canonical.trim_end_matches('/').to_ascii_lowercase();
        }
        url.trim().trim_end_matches('/').to_ascii_lowercase()
    }

    fn place_container(
        &mut self,
        existing: &[(Entity, String, u32)],
        claimed: &[String],
        next_order: u32,
        claimed_orders: &mut HashMap<String, u32>,
    ) {
        let mut pins = existing.to_vec();
        let mut order = next_order;
        for url in claimed {
            let key = Self::key(url);
            if pins.iter().any(|(_, existing, _)| *existing == key) {
                continue;
            }
            pins.push((Entity::PLACEHOLDER, key, order));
            order = order.saturating_add(1);
        }
        pins.sort_by_key(|(entity, _, order)| (*order, entity.to_bits()));
        let container_key = Self::key("vmux://vm/");
        let simulator_key = Self::key("vmux://simulator/");
        let Some(container) = pins.iter().position(|(_, key, _)| *key == container_key) else {
            return;
        };
        let Some(simulator) = pins.iter().position(|(_, key, _)| *key == simulator_key) else {
            return;
        };
        if container == simulator + 1 {
            return;
        }
        let pin = pins.remove(container);
        let simulator = if container < simulator {
            simulator - 1
        } else {
            simulator
        };
        pins.insert(simulator + 1, pin);
        for (order, (entity, key, previous)) in pins.into_iter().enumerate() {
            let order = order as u32;
            if entity == Entity::PLACEHOLDER {
                claimed_orders.insert(key, order);
            } else if previous != order {
                self.commands.entity(entity).insert(BookmarkOrder(order));
            }
        }
    }
}

#[derive(SystemParam)]
struct BookmarkSeed<'w, 's> {
    pins: Query<'w, 's, (Entity, &'static PageMetadata, &'static BookmarkOrder), With<Pin>>,
    folders:
        Query<'w, 's, (Entity, &'static Name, Option<&'static SmartBookmarkFolder>), With<Folder>>,
    orders: Query<'w, 's, &'static BookmarkOrder, BookmarkFilter>,
    offered: ResMut<'w, OfferedBookmarkDefaults>,
    auto: Single<'w, 's, &'static mut State, With<BookmarkAutoSave>>,
    commands: Commands<'w, 's>,
}

fn insert_defaults(
    _trigger: On<SeedBookmarkDefaults>,
    settings: Res<BookmarkDefaults>,
    mut seed: BookmarkSeed,
) {
    let urls = &settings.urls;
    let folders = &settings.folders;
    let pin_revision = seed.offered.pin_order_revision;
    let vm_revision = seed.offered.vm_root_revision;
    let migrated = seed.move_vm_default(urls);
    let retired_keys = RETIRED_DEFAULT_BOOKMARKS
        .iter()
        .map(|url| BookmarkSeed::key(url))
        .collect::<HashSet<_>>();
    let offered_before = seed
        .offered
        .urls
        .iter()
        .map(|url| BookmarkSeed::key(url))
        .collect::<HashSet<_>>();
    let mut retired_entities = HashSet::new();
    if pin_revision < BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
        for (entity, metadata, _) in &seed.pins {
            let key = BookmarkSeed::key(&metadata.url);
            if retired_keys.contains(&key) && offered_before.contains(&key) {
                seed.commands.entity(entity).despawn();
                retired_entities.insert(entity);
            }
        }
        seed.offered
            .urls
            .retain(|url| !retired_keys.contains(&BookmarkSeed::key(url)));
    }
    let mut offered_urls = seed
        .offered
        .urls
        .iter()
        .map(|url| BookmarkSeed::key(url))
        .collect::<HashSet<_>>();
    let mut claimed_urls = Vec::new();
    for url in urls {
        let key = BookmarkSeed::key(url);
        if key.is_empty() || !offered_urls.insert(key) {
            continue;
        }
        seed.offered.urls.push(url.clone());
        claimed_urls.push(url.clone());
    }
    let mut offered_folders = seed
        .offered
        .folders
        .iter()
        .map(|name| BookmarkSeed::key(name))
        .collect::<HashSet<_>>();
    let mut claimed_folders = Vec::new();
    for folder in folders {
        let key = BookmarkSeed::key(&folder.name);
        if key.is_empty() || !offered_folders.insert(key) {
            continue;
        }
        seed.offered.folders.push(folder.name.clone());
        claimed_folders.push(folder.name.clone());
    }
    let legacy_folder_bookmarks = !seed.offered.folder_bookmarks.is_empty();
    seed.offered.folder_bookmarks.clear();
    let mut changed = !claimed_urls.is_empty()
        || !claimed_folders.is_empty()
        || legacy_folder_bookmarks
        || vm_revision != seed.offered.vm_root_revision
        || !retired_entities.is_empty();
    let existing_pins = seed
        .pins
        .iter()
        .filter(|(entity, _, _)| !retired_entities.contains(entity))
        .map(|(entity, metadata, order)| {
            let key = if migrated.contains(&entity) {
                BookmarkSeed::key("vmux://vm/")
            } else {
                BookmarkSeed::key(&metadata.url)
            };
            (entity, key, order.0)
        })
        .collect::<Vec<_>>();
    let mut pinned = existing_pins
        .iter()
        .map(|(_, key, _)| key.clone())
        .collect::<HashSet<_>>();
    let mut next_order = seed
        .orders
        .iter()
        .map(|order| order.0)
        .max()
        .map_or(0, |order| order.saturating_add(1));
    let mut claimed_orders = HashMap::new();
    if pin_revision < BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
        let claimed = claimed_urls
            .iter()
            .map(|url| BookmarkSeed::key(url))
            .collect::<HashSet<_>>();
        let mut default_keys = Vec::new();
        let mut seen = HashSet::new();
        for url in urls {
            let key = BookmarkSeed::key(url);
            if seen.insert(key.clone()) && (pinned.contains(&key) || claimed.contains(&key)) {
                default_keys.push(key);
            }
        }
        let default_orders = default_keys
            .iter()
            .enumerate()
            .map(|(order, key)| (key.clone(), order as u32))
            .collect::<HashMap<_, _>>();
        for (entity, key, _) in &existing_pins {
            if let Some(order) = default_orders.get(key) {
                seed.commands.entity(*entity).insert(BookmarkOrder(*order));
            }
        }
        for url in &claimed_urls {
            let key = BookmarkSeed::key(url);
            if let Some(order) = default_orders.get(&key) {
                claimed_orders.insert(key, *order);
            }
        }
        let mut custom_pins = existing_pins
            .iter()
            .filter(|(_, key, _)| !default_orders.contains_key(key))
            .collect::<Vec<_>>();
        custom_pins.sort_by_key(|(entity, _, order)| (*order, entity.to_bits()));
        let mut order = default_orders.len() as u32;
        for (entity, _, _) in custom_pins {
            seed.commands.entity(*entity).insert(BookmarkOrder(order));
            order = order.saturating_add(1);
        }
        changed = true;
    }
    if pin_revision < BookmarkSeed::PIN_ORDER_REVISION {
        if pin_revision >= BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
            seed.place_container(
                &existing_pins,
                &claimed_urls,
                next_order,
                &mut claimed_orders,
            );
        }
        seed.offered.pin_order_revision = BookmarkSeed::PIN_ORDER_REVISION;
        changed = true;
    }
    for url in claimed_urls {
        if !pinned.insert(BookmarkSeed::key(&url)) {
            continue;
        }
        let order = claimed_orders
            .get(&BookmarkSeed::key(&url))
            .copied()
            .unwrap_or(next_order);
        seed.commands.spawn((
            Pin,
            Uuid(uuid::Uuid::new_v4().to_string()),
            PageMetadata {
                title: url.clone(),
                url,
                icon: PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(order),
        ));
        if order == next_order {
            next_order = next_order.saturating_add(1);
        }
    }
    let mut existing_folders = seed
        .folders
        .iter()
        .map(|(entity, name, smart)| (BookmarkSeed::key(name.as_str()), (entity, smart.copied())))
        .collect::<HashMap<_, _>>();
    for name in claimed_folders {
        let key = BookmarkSeed::key(&name);
        if existing_folders.contains_key(&key) {
            continue;
        }
        let setting = folders
            .iter()
            .find(|folder| BookmarkSeed::key(&folder.name) == key);
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
    for setting in folders {
        let Some(smart) = setting.smart else {
            continue;
        };
        let Some((entity, current)) = existing_folders.get(&BookmarkSeed::key(&setting.name))
        else {
            continue;
        };
        if *current != Some(smart) {
            seed.commands.entity(*entity).insert(smart);
            changed = true;
        }
    }
    if changed {
        seed.auto.advance();
    }
}

#[derive(Component, Default)]
#[require(State)]
struct BookmarkAutoSave;

#[derive(Component)]
struct RetryAt(std::time::Instant);

fn mark_bookmarks_dirty(
    mut auto: Single<&mut State, With<BookmarkAutoSave>>,
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
    let mut removed_child_of_bookmark = false;
    for entity in removed_child_of.read() {
        removed_child_of_bookmark |= bookmark_items.get(entity).is_ok();
    }
    let pin_removed = removed_pin.read().count() > 0;
    let bookmark_removed = removed_bookmark.read().count() > 0;
    let folder_removed = removed_folder.read().count() > 0;
    let collapsed_removed = removed_collapsed.read().count() > 0;
    let any_removed = pin_removed
        || bookmark_removed
        || folder_removed
        || collapsed_removed
        || removed_child_of_bookmark;
    if any_removed || !changed.is_empty() {
        auto.advance();
    }
}

fn autosave_bookmarks(
    store: Single<(Entity, &State, Option<&RetryAt>), With<BookmarkAutoSave>>,
    path: Single<&BookmarkStorePath>,
    types: Res<BookmarkStoreTypes>,
    mut commands: Commands,
) {
    let (entity, state, retry) = store.into_inner();
    if !state.is_dirty()
        || state.phase() != vmux_ecs::store::Phase::Ready
        || SessionEnvironment::is_test()
    {
        return;
    }
    if retry.is_some_and(|retry| std::time::Instant::now() < retry.0) {
        return;
    }
    if let Some(parent) = path.0.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut save = SaveWorld::<BookmarkFilter>::into_file(path.0.clone());
    types.apply(&mut save);
    commands.queue(BookmarkWrite { entity, save });
}

struct BookmarkWrite {
    entity: Entity,
    save: SaveWorld<BookmarkFilter>,
}

impl Command for BookmarkWrite {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(revision) = world
            .get_mut::<State>(self.entity)
            .and_then(|mut state| state.begin())
        else {
            return;
        };
        let result = moonshine_save::save::SaveCommand(self.save).apply(world);
        let Some(mut state) = world.get_mut::<State>(self.entity) else {
            return;
        };
        let failed = match result {
            Ok(()) => {
                state.succeed(revision);
                false
            }
            Err(error) => {
                error!(%error, "bookmark write failed; retrying in 60 seconds");
                state.retry(revision, error.to_string());
                true
            }
        };
        if failed {
            world.entity_mut(self.entity).insert(RetryAt(
                std::time::Instant::now() + std::time::Duration::from_secs(60),
            ));
        } else {
            world.entity_mut(self.entity).remove::<RetryAt>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_bookmark_write_retries_after_storage_recovers_without_a_new_edit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.ron");
        std::fs::create_dir(&path).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
        ))
        .add_plugins(SchemaPlugin::<BookmarkStore>::default())
        .insert_resource(OfferedBookmarkDefaults::default())
        .add_systems(Update, autosave_bookmarks);
        let store = app
            .world_mut()
            .spawn((BookmarkAutoSave, BookmarkStorePath(path.clone())))
            .id();
        app.world_mut().spawn((
            Bookmark,
            Uuid("retry".into()),
            PageMetadata {
                url: "https://example.test/".into(),
                ..Default::default()
            },
        ));
        app.world_mut().get_mut::<State>(store).unwrap().advance();
        app.update();
        assert!(path.is_dir());
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert!(app.world().get::<State>(store).unwrap().error().is_some());
        std::fs::remove_dir(&path).unwrap();
        app.world_mut().get_mut::<RetryAt>(store).unwrap().0 =
            std::time::Instant::now() - std::time::Duration::from_secs(1);
        app.update();
        assert!(path.is_file());
        let state = app.world().get::<State>(store).unwrap();
        assert!(!state.is_dirty());
        assert!(state.loaded().is_some());
        assert!(state.error().is_none());
    }

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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .add_plugins(SchemaPlugin::<BookmarkStore>::default())
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
                icon: vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Smartphone),
                bg_color: None,
            },
            BookmarkOrder(1),
        ));
        save_app
            .world_mut()
            .spawn((Save, Name::new("excluded-save-entity")));
        let p = path.clone();
        let types = save_app.world().resource::<BookmarkStoreTypes>().clone();
        save_app.add_systems(Update, move |mut c: Commands| {
            let mut s = SaveWorld::<BookmarkFilter>::into_file(p.clone());
            types.apply(&mut s);
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
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .add_plugins(SchemaPlugin::<BookmarkStore>::default())
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
            vmux_api::PageIcon::Builtin(vmux_api::BuiltinIcon::Smartphone)
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
    fn an_unavailable_icon_does_not_hide_other_bookmarks_or_rewrite_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.ron");
        let body = r#"(
            resources: {},
            entities: {
                1: (components: {
                    "vmux_ecs::Bookmark": (),
                    "vmux_header::system::PageMetadata": (
                        title: "Legacy",
                        url: "vmux://vm/docker/containers",
                        icon: Builtin(UnavailableIcon),
                        bg_color: None,
                    ),
                }),
                2: (components: {
                    "vmux_ecs::Bookmark": (),
                    "vmux_header::system::PageMetadata": (
                        title: "Terminal",
                        url: "vmux://terminal/",
                        icon: Builtin(Terminal),
                        bg_color: None,
                    ),
                }),
            },
        )"#;
        std::fs::write(&path, body).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
        ))
        .add_plugins(SchemaPlugin::<BookmarkStore>::default())
        .init_resource::<OfferedBookmarkDefaults>()
        .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
        let load = BookmarkStorePath(path.clone()).load().unwrap();
        app.world_mut().commands().trigger_load(load);
        app.update();
        let mut bookmarks = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Bookmark>>()
            .iter(app.world())
            .cloned()
            .collect::<Vec<_>>();
        bookmarks.sort_by(|left, right| left.title.cmp(&right.title));
        assert_eq!(bookmarks.len(), 2);
        assert_eq!(bookmarks[0].title, "Legacy");
        assert_eq!(bookmarks[0].url, "vmux://vm/docker/containers");
        assert_eq!(bookmarks[0].icon, PageIcon::None);
        assert_eq!(bookmarks[1].title, "Terminal");
        assert_eq!(
            bookmarks[1].icon,
            PageIcon::Builtin(vmux_api::BuiltinIcon::Terminal)
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), body);
    }

    #[test]
    fn legacy_container_pin_moves_after_simulator_without_reordering_other_pins() {
        let mut settings = AppSettings::default();
        settings.browser.bookmarks = vec!["vmux://simulator/".into(), "vmux://vm/".into()];
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
        ))
        .add_plugins(SchemaPlugin::<BookmarkStore>::default())
        .insert_resource(BookmarkDefaults::from(&settings))
        .insert_resource(settings)
        .insert_resource(OfferedBookmarkDefaults {
            urls: vec!["vmux://simulator/".into(), "vmux://containers/".into()],
            pin_order_revision: 4,
            ..default()
        })
        .add_observer(insert_defaults);
        app.world_mut().spawn(BookmarkAutoSave);
        let mut container = Entity::PLACEHOLDER;
        for (order, url) in [
            "https://first.example/",
            "vmux://simulator/",
            "https://second.example/",
            "vmux://containers/",
        ]
        .into_iter()
        .enumerate()
        {
            let entity = app
                .world_mut()
                .spawn((
                    Pin,
                    Uuid(url.into()),
                    BookmarkOrder(order as u32),
                    PageMetadata {
                        url: url.into(),
                        ..default()
                    },
                ))
                .id();
            if url == "vmux://containers/" {
                container = entity;
            }
        }
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.update();
        let mut pins = app
            .world_mut()
            .query_filtered::<(&PageMetadata, &BookmarkOrder), With<Pin>>()
            .iter(app.world())
            .map(|(metadata, order)| (order.0, metadata.url.clone()))
            .collect::<Vec<_>>();
        pins.sort();
        assert_eq!(
            pins.into_iter().map(|(_, url)| url).collect::<Vec<_>>(),
            [
                "https://first.example/",
                "vmux://simulator/",
                "vmux://vm/",
                "https://second.example/",
            ]
        );
        assert_eq!(
            app.world().get::<Uuid>(container).unwrap().0,
            "vmux://containers/"
        );
        app.world_mut()
            .entity_mut(container)
            .insert(BookmarkOrder(10));
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.update();
        assert_eq!(app.world().get::<BookmarkOrder>(container).unwrap().0, 10);
        app.world_mut().despawn(container);
        {
            let mut offered = app.world_mut().resource_mut::<OfferedBookmarkDefaults>();
            offered.pin_order_revision = 4;
            offered.vm_root_revision = 0;
            offered.urls = vec!["vmux://simulator/".into(), "vmux://containers/".into()];
        }
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Pin>>()
                .iter(app.world())
                .count(),
            3
        );
    }

    #[test]
    fn existing_bookmark_store_receives_missing_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bookmarks.ron");
        let mut save_app = App::new();
        save_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .add_plugins(SchemaPlugin::<BookmarkStore>::default())
            .add_observer(save_on::<SaveWorld<BookmarkFilter>>);
        save_app.world_mut().spawn((
            Pin,
            Uuid("existing".into()),
            PageMetadata {
                title: "Existing".into(),
                url: "https://example.com".into(),
                icon: vmux_api::PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(4),
        ));
        let save_path = path.clone();
        let types = save_app.world().resource::<BookmarkStoreTypes>().clone();
        save_app.add_systems(Update, move |mut commands: Commands| {
            let mut save = SaveWorld::<BookmarkFilter>::into_file(save_path.clone());
            types.apply(&mut save);
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
            .spawn((BookmarkAutoSave, BookmarkLoadPending));
        load_app
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(vmux_ecs::PrimitivesPlugin)
            .insert_resource(BookmarkDefaults::from(&settings))
            .insert_resource(settings)
            .add_plugins(SchemaPlugin::<BookmarkStore>::default())
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
                ("https://example.com".into(), 2),
                ("vmux://start/".into(), 0),
                ("vmux://terminal/".into(), 1),
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

        load_app.world_mut().trigger(SeedBookmarkDefaults);
        load_app.update();
        assert_eq!(
            load_app
                .world_mut()
                .query_filtered::<Entity, With<Pin>>()
                .iter(load_app.world())
                .count(),
            3
        );
        assert_eq!(
            load_app
                .world_mut()
                .query_filtered::<Entity, With<Folder>>()
                .iter(load_app.world())
                .count(),
            2
        );
    }
}
