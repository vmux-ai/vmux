use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::prelude::{App, Plugin};
use moonshine_save::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
use vmux_api::PageIcon;
use vmux_api::bookmark::SmartBookmarkFolder;
use vmux_ecs::profile::SessionEnvironment;
use vmux_ecs::store::{
    AutoSave, AutoSavePaused, AutoSavePlugin, AutoSavePolicy, SchemaPlugin, State, StorePath,
    StoreWriteRequest, StoreWriteResult, Types,
};
use vmux_ecs::time::BackoffPolicy;
#[cfg(test)]
use vmux_ecs::time::Clock;
use vmux_ecs::{
    Bookmark, BookmarkOrder, Collapsed, Failure, Folder, Loading, PageMetadata, Pin, Ready,
    StatusPlugin, Uuid,
};
use vmux_layout::LayoutStartupSet;
#[cfg(test)]
use vmux_setting::AppSettings;
use vmux_setting::BookmarkDefaults;

pub(super) struct BookmarkStorePlugin;

impl Plugin for BookmarkStorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatusPlugin>() {
            app.add_plugins(StatusPlugin);
        }
        app.add_plugins((
            SchemaPlugin::<BookmarkStore>::default(),
            AutoSavePlugin::<BookmarkStore>::default(),
        ))
        .init_resource::<OfferedBookmarkDefaults>()
        .add_observer(save_on::<SaveWorld<BookmarkFilter>>)
        .add_observer(load_on::<LoadWorld<BookmarkFilter>>)
        .add_observer(seed_defaults)
        .add_observer(insert_defaults)
        .add_observer(migrate_vm)
        .add_observer(retire_defaults)
        .add_observer(claim_pins)
        .add_observer(order_legacy)
        .add_observer(order_vm)
        .add_observer(publish_pins)
        .add_observer(seed_folders)
        .add_observer(finish_defaults)
        .add_observer(write_snapshot)
        .add_systems(
            Startup,
            (spawn, ApplyDeferred, load_bookmarks_on_startup)
                .chain()
                .after(LayoutStartupSet::Store),
        );
    }
}

type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

#[vmux_ecs::store(
    filter = BookmarkFilter,
    track;
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

impl BookmarkStore {
    fn load(path: &Path) -> std::io::Result<LoadWorld<BookmarkFilter>> {
        let mut source = std::fs::read_to_string(path)?;
        for (legacy, current) in [
            (
                "vmux_desktop::bookmark_persistence::OfferedBookmarkDefaults",
                "vmux_bookmark::OfferedBookmarkDefaults",
            ),
            ("vmux_core::BookmarkOrder", "vmux_ecs::BookmarkOrder"),
            ("vmux_core::Pin", "vmux_ecs::Pin"),
            ("vmux_core::Bookmark", "vmux_ecs::Bookmark"),
            ("vmux_core::Folder", "vmux_ecs::Folder"),
            ("vmux_core::Collapsed", "vmux_ecs::Collapsed"),
            ("vmux_core::Uuid", "vmux_ecs::Uuid"),
        ] {
            source = source.replace(legacy, current);
        }
        if let Some((normalized, unknown)) = vmux_api::icon::StoredPageIcons(&source).normalize() {
            warn!(
                ?unknown,
                "Replacing unavailable bookmark icons while loading"
            );
            source = normalized;
        }
        Ok(LoadWorld::<BookmarkFilter>::from_stream(
            std::io::Cursor::new(source.into_bytes()),
        ))
    }
}

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
    let mut store = commands.spawn((
        Name::new("Bookmark store"),
        AutoSave::<BookmarkStore>::default(),
        StorePath::new(path),
        AutoSavePolicy::immediate().with_retry(BackoffPolicy::exponential(
            Duration::from_secs(60),
            Duration::from_secs(60),
        )),
    ));
    if SessionEnvironment::is_test() {
        store.insert(AutoSavePaused);
    }
}

fn load_bookmarks_on_startup(
    store: Single<Entity, With<AutoSave<BookmarkStore>>>,
    mut state: Single<&mut State, With<AutoSave<BookmarkStore>>>,
    path: Single<&StorePath, With<AutoSave<BookmarkStore>>>,
    mut commands: Commands,
) {
    if SessionEnvironment::is_test() {
        return;
    }
    if !path.0.exists() {
        commands.entity(*store).insert(Ready);
        commands.trigger(SeedBookmarkDefaults);
        return;
    }
    let load = match BookmarkStore::load(&path.0) {
        Ok(load) => load,
        Err(error) => {
            error!(path = %path.0.display(), %error, "bookmark load failed");
            let message = error.to_string();
            state.fail(message.clone());
            commands.entity(*store).insert(Failure::new(message));
            return;
        }
    };
    state.materialize();
    commands.entity(*store).insert(Loading);
    commands.trigger_load(load);
}

fn seed_defaults(
    _trigger: On<Loaded>,
    pending: Option<Single<Entity, (With<AutoSave<BookmarkStore>>, With<Loading>)>>,
    mut state: Single<&mut State, With<AutoSave<BookmarkStore>>>,
    mut commands: Commands,
) {
    let Some(pending) = pending else {
        return;
    };
    state.ready();
    commands.entity(*pending).insert(Ready);
    commands.trigger(SeedBookmarkDefaults);
}

#[derive(Event)]
struct SeedBookmarkDefaults;

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
    const RETIRED_DEFAULT_BOOKMARKS: [&'static str; 2] = ["vmux://spaces/", "vmux://team/"];
    const PIN_ORDER_REVISION: u32 = 5;
    const LEGACY_PIN_ORDER_REVISION: u32 = 4;

    fn key(url: &str) -> String {
        if let Some(canonical) = vmux_api::VmuxRoute::canonical(url) {
            return canonical.trim_end_matches('/').to_ascii_lowercase();
        }
        url.trim().trim_end_matches('/').to_ascii_lowercase()
    }
}

#[derive(SystemParam)]
struct BookmarkSeed<'w, 's> {
    pins: Query<'w, 's, (Entity, &'static PageMetadata, &'static BookmarkOrder), With<Pin>>,
    folders:
        Query<'w, 's, (Entity, &'static Name, Option<&'static SmartBookmarkFolder>), With<Folder>>,
    orders: Query<'w, 's, &'static BookmarkOrder, BookmarkFilter>,
    offered: ResMut<'w, OfferedBookmarkDefaults>,
    auto: Single<'w, 's, (Entity, &'static mut State), With<AutoSave<BookmarkStore>>>,
    commands: Commands<'w, 's>,
}

#[derive(Component)]
struct Seeding {
    settings: BookmarkDefaults,
    pin_revision: u32,
    changed: bool,
}

#[derive(Component)]
struct NextOrder(u32);

#[derive(Component)]
struct ClaimedPin(Entity);

#[derive(EntityEvent)]
struct MigrateVm {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct RetireDefaults {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct ClaimPins {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct OrderLegacy {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct OrderVm {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct PublishPins {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct SeedFolders {
    #[event_target]
    entity: Entity,
}

#[derive(EntityEvent)]
struct FinishDefaults {
    #[event_target]
    entity: Entity,
}

fn insert_defaults(
    _trigger: On<SeedBookmarkDefaults>,
    settings: Res<BookmarkDefaults>,
    running: Query<(), With<Seeding>>,
    mut seed: BookmarkSeed,
) {
    let entity = seed.auto.0;
    if running.contains(entity) {
        return;
    }
    let next = seed
        .orders
        .iter()
        .map(|order| order.0)
        .max()
        .map_or(0, |order| order.saturating_add(1));
    seed.commands.entity(entity).insert((
        Seeding {
            settings: settings.clone(),
            pin_revision: seed.offered.pin_order_revision,
            changed: false,
        },
        NextOrder(next),
    ));
    seed.commands.trigger(MigrateVm { entity });
}

fn migrate_vm(trigger: On<MigrateVm>, mut roots: Query<&mut Seeding>, mut seed: BookmarkSeed) {
    let entity = trigger.event().entity;
    let Ok(mut state) = roots.get_mut(entity) else {
        return;
    };
    let vm = BookmarkSeed::key("vmux://vm/");
    if seed.offered.vm_root_revision == 0
        && state
            .settings
            .urls
            .iter()
            .any(|url| BookmarkSeed::key(url) == vm)
    {
        let existing = seed
            .pins
            .iter()
            .any(|(_, metadata, _)| BookmarkSeed::key(&metadata.url) == vm);
        if !existing {
            let containers = BookmarkSeed::key("vmux://vm/docker/containers");
            for url in &mut seed.offered.urls {
                if BookmarkSeed::key(url) == containers {
                    *url = "vmux://vm/".into();
                }
            }
            for (entity, metadata, _) in &seed.pins {
                if BookmarkSeed::key(&metadata.url) != containers {
                    continue;
                }
                let mut metadata = metadata.clone();
                metadata.url = "vmux://vm/".into();
                metadata.title = "VM".into();
                metadata.icon = PageIcon::Builtin(vmux_api::BuiltinIcon::Computer);
                seed.commands.entity(entity).insert(metadata);
            }
        }
        seed.offered.vm_root_revision = 1;
        state.changed = true;
    }
    seed.commands.trigger(RetireDefaults { entity });
}

fn retire_defaults(
    trigger: On<RetireDefaults>,
    mut roots: Query<&mut Seeding>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok(mut state) = roots.get_mut(entity) else {
        return;
    };
    if state.pin_revision < BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
        let mut retired = HashSet::new();
        for url in BookmarkSeed::RETIRED_DEFAULT_BOOKMARKS {
            retired.insert(BookmarkSeed::key(url));
        }
        let mut offered = HashSet::new();
        for url in &seed.offered.urls {
            offered.insert(BookmarkSeed::key(url));
        }
        for (pin, metadata, _) in &seed.pins {
            let key = BookmarkSeed::key(&metadata.url);
            if retired.contains(&key) && offered.contains(&key) {
                seed.commands.entity(pin).despawn();
                state.changed = true;
            }
        }
        seed.offered
            .urls
            .retain(|url| !retired.contains(&BookmarkSeed::key(url)));
    }
    seed.commands.trigger(ClaimPins { entity });
}

fn claim_pins(
    trigger: On<ClaimPins>,
    mut roots: Query<(&mut Seeding, &NextOrder)>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok((mut state, next)) = roots.get_mut(entity) else {
        return;
    };
    let mut offered = HashSet::new();
    for url in &seed.offered.urls {
        offered.insert(BookmarkSeed::key(url));
    }
    let mut pinned = HashSet::new();
    for (_, metadata, _) in &seed.pins {
        pinned.insert(BookmarkSeed::key(&metadata.url));
    }
    let mut order = next.0;
    let mut changed = false;
    for url in &state.settings.urls {
        let key = BookmarkSeed::key(url);
        if key.is_empty() || !offered.insert(key.clone()) {
            continue;
        }
        seed.offered.urls.push(url.clone());
        changed = true;
        if !pinned.insert(key) {
            continue;
        }
        seed.commands.spawn((
            ClaimedPin(entity),
            Uuid(uuid::Uuid::new_v4().to_string()),
            PageMetadata {
                title: url.clone(),
                url: url.clone(),
                icon: PageIcon::None,
                bg_color: None,
            },
            BookmarkOrder(order),
        ));
        order = order.saturating_add(1);
    }
    state.changed |= changed;
    seed.commands.trigger(OrderLegacy { entity });
}

fn order_legacy(
    trigger: On<OrderLegacy>,
    mut roots: Query<&mut Seeding>,
    claims: Query<(Entity, &PageMetadata, &ClaimedPin)>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok(mut state) = roots.get_mut(entity) else {
        return;
    };
    if state.pin_revision < BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
        let mut pinned = HashSet::new();
        for (_, metadata, _) in &seed.pins {
            pinned.insert(BookmarkSeed::key(&metadata.url));
        }
        for (_, metadata, claim) in &claims {
            if claim.0 == entity {
                pinned.insert(BookmarkSeed::key(&metadata.url));
            }
        }
        let mut default_orders = HashMap::new();
        for url in &state.settings.urls {
            let key = BookmarkSeed::key(url);
            if pinned.contains(&key) && !default_orders.contains_key(&key) {
                default_orders.insert(key, default_orders.len() as u32);
            }
        }
        let mut custom = Vec::new();
        for (pin, metadata, order) in &seed.pins {
            let key = BookmarkSeed::key(&metadata.url);
            match default_orders.get(&key) {
                Some(order) => {
                    seed.commands.entity(pin).insert(BookmarkOrder(*order));
                }
                None => custom.push((pin, order.0)),
            }
        }
        for (pin, metadata, claim) in &claims {
            if claim.0 != entity {
                continue;
            }
            if let Some(order) = default_orders.get(&BookmarkSeed::key(&metadata.url)) {
                seed.commands.entity(pin).insert(BookmarkOrder(*order));
            }
        }
        custom.sort_by_key(|(pin, order)| (*order, pin.to_bits()));
        let mut order = default_orders.len() as u32;
        for (pin, _) in custom {
            seed.commands.entity(pin).insert(BookmarkOrder(order));
            order = order.saturating_add(1);
        }
        state.changed = true;
    }
    seed.commands.trigger(OrderVm { entity });
}

fn order_vm(
    trigger: On<OrderVm>,
    mut roots: Query<&mut Seeding>,
    claims: Query<(Entity, &PageMetadata, &BookmarkOrder, &ClaimedPin)>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok(mut state) = roots.get_mut(entity) else {
        return;
    };
    if state.pin_revision < BookmarkSeed::PIN_ORDER_REVISION {
        if state.pin_revision >= BookmarkSeed::LEGACY_PIN_ORDER_REVISION {
            let mut pins = Vec::new();
            for (pin, metadata, order) in &seed.pins {
                pins.push((pin, BookmarkSeed::key(&metadata.url), order.0, false));
            }
            let mut claimed = HashMap::new();
            for (pin, metadata, order, claim) in &claims {
                if claim.0 == entity {
                    claimed.insert(BookmarkSeed::key(&metadata.url), (pin, order.0));
                }
            }
            let mut seen = HashSet::new();
            for url in &state.settings.urls {
                let key = BookmarkSeed::key(url);
                if !seen.insert(key.clone()) {
                    continue;
                }
                if let Some((pin, order)) = claimed.get(&key) {
                    pins.push((*pin, key, *order, true));
                }
            }
            pins.sort_by_key(|(pin, _, order, claimed)| {
                (
                    *order,
                    if *claimed {
                        Entity::PLACEHOLDER.to_bits()
                    } else {
                        pin.to_bits()
                    },
                )
            });
            let vm = BookmarkSeed::key("vmux://vm/");
            let simulator = BookmarkSeed::key("vmux://simulator/");
            if let (Some(vm), Some(simulator)) = (
                pins.iter().position(|(_, key, _, _)| *key == vm),
                pins.iter().position(|(_, key, _, _)| *key == simulator),
            ) && vm != simulator + 1
            {
                let pin = pins.remove(vm);
                let simulator = if vm < simulator {
                    simulator - 1
                } else {
                    simulator
                };
                pins.insert(simulator + 1, pin);
                for (order, (pin, _, previous, claimed)) in pins.into_iter().enumerate() {
                    let order = order as u32;
                    if claimed || previous != order {
                        seed.commands.entity(pin).insert(BookmarkOrder(order));
                    }
                }
            }
        }
        seed.offered.pin_order_revision = BookmarkSeed::PIN_ORDER_REVISION;
        state.changed = true;
    }
    seed.commands.trigger(PublishPins { entity });
}

fn publish_pins(
    trigger: On<PublishPins>,
    mut roots: Query<(&Seeding, &mut NextOrder)>,
    claims: Query<(Entity, &PageMetadata, &BookmarkOrder, &ClaimedPin)>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok((state, mut next)) = roots.get_mut(entity) else {
        return;
    };
    let mut claimed = HashMap::new();
    for (pin, metadata, order, claim) in &claims {
        if claim.0 == entity {
            claimed.insert(BookmarkSeed::key(&metadata.url), (pin, order.0));
        }
    }
    let mut seen = HashSet::new();
    for url in &state.settings.urls {
        let key = BookmarkSeed::key(url);
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some((pin, order)) = claimed.get(&key) else {
            continue;
        };
        seed.commands
            .entity(*pin)
            .remove::<ClaimedPin>()
            .insert(Pin);
        if *order == next.0 {
            next.0 = next.0.saturating_add(1);
        }
    }
    seed.commands.trigger(SeedFolders { entity });
}

fn seed_folders(
    trigger: On<SeedFolders>,
    mut roots: Query<(&mut Seeding, &mut NextOrder)>,
    mut seed: BookmarkSeed,
) {
    let entity = trigger.event().entity;
    let Ok((mut state, mut next)) = roots.get_mut(entity) else {
        return;
    };
    let mut offered = HashSet::new();
    for name in &seed.offered.folders {
        offered.insert(BookmarkSeed::key(name));
    }
    let mut existing = HashMap::new();
    for (folder, name, smart) in &seed.folders {
        existing.insert(BookmarkSeed::key(name.as_str()), (folder, smart.copied()));
    }
    let mut changed = !seed.offered.folder_bookmarks.is_empty();
    for order in &seed.orders {
        next.0 = next.0.max(order.0.saturating_add(1));
    }
    seed.offered.folder_bookmarks.clear();
    for setting in &state.settings.folders {
        let key = BookmarkSeed::key(&setting.name);
        if key.is_empty() || !offered.insert(key.clone()) {
            continue;
        }
        seed.offered.folders.push(setting.name.clone());
        changed = true;
        if existing.contains_key(&key) {
            continue;
        }
        let mut folder = seed.commands.spawn((
            Folder,
            Uuid(uuid::Uuid::new_v4().to_string()),
            Name::new(setting.name.clone()),
            BookmarkOrder(next.0),
        ));
        if let Some(smart) = setting.smart {
            folder.insert(smart);
        }
        existing.insert(key, (folder.id(), setting.smart));
        next.0 = next.0.saturating_add(1);
    }
    for setting in &state.settings.folders {
        let Some(smart) = setting.smart else {
            continue;
        };
        let Some((folder, current)) = existing.get(&BookmarkSeed::key(&setting.name)) else {
            continue;
        };
        if *current != Some(smart) {
            seed.commands.entity(*folder).insert(smart);
            changed = true;
        }
    }
    state.changed |= changed;
    seed.commands.trigger(FinishDefaults { entity });
}

fn finish_defaults(trigger: On<FinishDefaults>, roots: Query<&Seeding>, mut seed: BookmarkSeed) {
    let entity = trigger.event().entity;
    let Ok(state) = roots.get(entity) else {
        return;
    };
    if state.changed {
        seed.auto.1.advance();
    }
    seed.commands
        .entity(entity)
        .remove::<(Seeding, NextOrder)>();
}

fn write_snapshot(
    request: On<StoreWriteRequest<BookmarkStore>>,
    types: Res<BookmarkStoreTypes>,
    mut commands: Commands,
) {
    if SessionEnvironment::is_test() {
        commands.trigger(StoreWriteResult::<BookmarkStore>::ok(
            request.entity,
            request.revision,
        ));
        return;
    }
    if let Some(parent) = request.path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut save = SaveWorld::<BookmarkFilter>::into_file(request.path.clone());
    types.apply(&mut save);
    commands.queue(WriteSnapshot {
        store: request.entity,
        revision: request.revision,
        save,
    });
}

struct WriteSnapshot {
    store: Entity,
    revision: u64,
    save: SaveWorld<BookmarkFilter>,
}

impl Command for WriteSnapshot {
    type Out = ();

    fn apply(self, world: &mut World) {
        let result = moonshine_save::save::SaveCommand(self.save)
            .apply(world)
            .map_err(|error| error.to_string());
        if let Err(error) = &result {
            error!(%error, "bookmark write failed; retrying with backoff");
        }
        world.trigger(StoreWriteResult::<BookmarkStore>::new(
            self.store,
            self.revision,
            result,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_icons_do_not_prevent_saved_bookmarks_from_loading() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.ron");
        let body = r#"(resources: {}, entities: { 1: (components: {
            "vmux_ecs::Pin": (),
            "vmux_ecs::Uuid": ("retained"),
            "vmux_ecs::BookmarkOrder": (0),
            "vmux_header::system::PageMetadata": (
                title: "Saved bookmark",
                url: "vmux://containers/",
                icon: Builtin(UnsupportedFutureIcon),
                bg_color: None,
            ),
        }) })"#;
        std::fs::write(&path, body).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
        ))
        .add_plugins(SchemaPlugin::<BookmarkStore>::default())
        .insert_resource(OfferedBookmarkDefaults::default())
        .add_observer(load_on::<LoadWorld<BookmarkFilter>>);
        let load = BookmarkStore::load(&path).unwrap();
        app.world_mut().commands().trigger_load(load);
        app.update();
        app.update();
        let mut pins = app
            .world_mut()
            .query_filtered::<(&Uuid, &PageMetadata), With<Pin>>();
        let (id, metadata) = pins.single(app.world()).unwrap();
        assert_eq!(id.0, "retained");
        assert_eq!(metadata.url, "vmux://containers/");
        assert!(metadata.icon.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), body);
    }

    #[test]
    fn failed_bookmark_write_retries_after_storage_recovers_without_a_new_edit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.ron");
        std::fs::create_dir(&path).unwrap();
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .add_plugins((
                MinimalPlugins,
                bevy::asset::AssetPlugin::default(),
                vmux_ecs::PrimitivesPlugin,
            ))
            .add_plugins(BookmarkStorePlugin);
        let store = app
            .world_mut()
            .spawn((
                AutoSave::<BookmarkStore>::default(),
                StorePath::new(path.clone()),
                AutoSavePolicy::immediate().with_retry(BackoffPolicy::exponential(
                    Duration::from_secs(60),
                    Duration::from_secs(60),
                )),
            ))
            .id();
        app.world_mut().spawn((
            Bookmark,
            Uuid("retry".into()),
            PageMetadata {
                url: "https://example.test/".into(),
                ..Default::default()
            },
        ));
        app.world_mut().run_schedule(PostUpdate);
        assert!(path.is_dir());
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert!(app.world().get::<State>(store).unwrap().error().is_some());
        std::fs::remove_dir(&path).unwrap();
        clock.advance(Duration::from_secs(60));
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
        let load = BookmarkStore::load(&path).unwrap();
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
    fn migration_preserves_distinct_existing_vm_and_containers_pins() {
        let mut settings = AppSettings::default();
        settings.browser.bookmarks =
            vec!["vmux://vm/".into(), "vmux://vm/docker/containers".into()];
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
            BookmarkStorePlugin,
        ))
        .insert_resource(BookmarkDefaults::from(&settings))
        .insert_resource(settings)
        .insert_resource(OfferedBookmarkDefaults {
            urls: vec!["vmux://vm/".into(), "vmux://vm/docker/containers".into()],
            pin_order_revision: 5,
            ..default()
        });
        app.world_mut().spawn(AutoSave::<BookmarkStore>::default());
        for (order, url) in ["vmux://vm/", "vmux://vm/docker/containers"]
            .into_iter()
            .enumerate()
        {
            app.world_mut().spawn((
                Pin,
                Uuid(url.into()),
                BookmarkOrder(order as u32),
                PageMetadata {
                    url: url.into(),
                    ..default()
                },
            ));
        }
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .resource::<OfferedBookmarkDefaults>()
                .vm_root_revision,
            1
        );
        let mut urls = app
            .world_mut()
            .query_filtered::<&PageMetadata, With<Pin>>()
            .iter(app.world())
            .map(|metadata| metadata.url.clone())
            .collect::<Vec<_>>();
        urls.sort();
        assert_eq!(urls, ["vmux://vm/", "vmux://vm/docker/containers"]);
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
        .add_plugins(BookmarkStorePlugin)
        .insert_resource(BookmarkDefaults::from(&settings))
        .insert_resource(settings)
        .insert_resource(OfferedBookmarkDefaults {
            urls: vec!["vmux://simulator/".into(), "vmux://containers/".into()],
            pin_order_revision: 4,
            ..default()
        });
        app.world_mut().spawn(AutoSave::<BookmarkStore>::default());
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
        app.world_mut().flush();
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
        app.world_mut().flush();
        assert_eq!(app.world().get::<BookmarkOrder>(container).unwrap().0, 10);
        app.world_mut().despawn(container);
        {
            let mut offered = app.world_mut().resource_mut::<OfferedBookmarkDefaults>();
            offered.pin_order_revision = 4;
            offered.vm_root_revision = 0;
            offered.urls = vec!["vmux://simulator/".into(), "vmux://containers/".into()];
        }
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.world_mut().flush();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Pin>>()
                .iter(app.world())
                .count(),
            3
        );
    }

    #[test]
    fn new_folders_follow_root_bookmarks_after_vm_pin_migration() {
        let mut settings = AppSettings::default();
        settings.browser.bookmarks = vec!["vmux://simulator/".into(), "vmux://vm/".into()];
        settings.browser.bookmark_folders = vec![vmux_setting::BookmarkFolderSettings {
            name: "Notes".into(),
            smart: None,
        }];
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            vmux_ecs::PrimitivesPlugin,
            BookmarkStorePlugin,
        ))
        .insert_resource(BookmarkDefaults::from(&settings))
        .insert_resource(settings)
        .insert_resource(OfferedBookmarkDefaults {
            urls: vec!["vmux://simulator/".into()],
            pin_order_revision: 4,
            ..default()
        });
        app.world_mut().spawn(AutoSave::<BookmarkStore>::default());
        app.world_mut().spawn((
            Pin,
            Uuid("simulator".into()),
            BookmarkOrder(0),
            PageMetadata {
                url: "vmux://simulator/".into(),
                ..default()
            },
        ));
        let bookmark = app
            .world_mut()
            .spawn((
                Pin,
                Bookmark,
                Uuid("custom".into()),
                BookmarkOrder(1),
                PageMetadata {
                    url: "https://example.test/".into(),
                    ..default()
                },
            ))
            .id();
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.world_mut().flush();
        let mut folders = app
            .world_mut()
            .query_filtered::<(Entity, &BookmarkOrder), With<Folder>>();
        let (folder, order) = folders.single(app.world()).unwrap();
        let order = order.0;
        assert!(order > app.world().get::<BookmarkOrder>(bookmark).unwrap().0);
        assert_eq!(
            app.world().get::<PageMetadata>(bookmark).unwrap().url,
            "https://example.test/"
        );
        app.world_mut().trigger(SeedBookmarkDefaults);
        app.world_mut().flush();
        assert_eq!(folders.iter(app.world()).count(), 1);
        assert_eq!(app.world().get::<BookmarkOrder>(folder).unwrap().0, order);
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
        let store = load_app
            .world_mut()
            .spawn((AutoSave::<BookmarkStore>::default(), Loading))
            .id();
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
            .add_observer(insert_defaults)
            .add_observer(migrate_vm)
            .add_observer(retire_defaults)
            .add_observer(claim_pins)
            .add_observer(order_legacy)
            .add_observer(order_vm)
            .add_observer(publish_pins)
            .add_observer(seed_folders)
            .add_observer(finish_defaults);
        load_app
            .world_mut()
            .commands()
            .trigger_load(LoadWorld::<BookmarkFilter>::from_file(path));
        load_app.update();
        load_app.update();

        assert!(load_app.world().get::<Ready>(store).is_some());
        assert!(load_app.world().get::<Loading>(store).is_none());

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
