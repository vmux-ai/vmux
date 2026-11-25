extern crate self as vmux_store;

pub use bevy_app::App;
pub use bevy_ecs::prelude::With;
pub use bevy_reflect::TypeRegistry;
pub use bevy_world_serialization::WorldFilter;
pub use moonshine_save::prelude::Save;
pub use vmux_macro::store;

pub use autosave::{
    AutoSave, AutoSavePaused, AutoSavePlugin, AutoSavePolicy, AutoSaveSet, Dirty, Phase, State,
    StoreAppExt, StorePath, StoreWriteRequest, StoreWriteResult,
};
pub use schema::{Schema, SchemaPlugin, StoreDomain, Types};
pub use validate::{RestoreValidator, StoreValidator, StoreValidators};

mod autosave;
mod schema;
mod validate;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy_app::AppExit;
    use bevy_ecs::prelude::*;
    use bevy_ecs::reflect::AppTypeRegistry;
    use bevy_reflect::Reflect;
    use bevy_world_serialization::{DynamicEntity, DynamicWorld};
    use vmux_time::{BackoffPolicy, Clock};
    use web_time::Duration;

    use super::*;

    enum SharedDomain {}

    type SharedStored = StoreDomain<SharedDomain>;

    #[test]
    fn save_completion_preserves_newer_edits_and_refuses_stale_results() {
        let mut state = State::default();
        state.advance();
        let first = state.begin().unwrap();
        state.advance();
        assert!(state.succeed(first));
        assert!(state.is_dirty());
        let second = state.begin().unwrap();
        assert!(!state.succeed(first));
        assert_eq!(state.saving(), Some(second));
        assert!(state.retry(second, "disk full".into()));
        assert_eq!(state.begin(), Some(second));
        assert!(state.succeed(second));
        assert_eq!(state.loaded(), Some(second));
        assert!(!state.is_dirty());
    }

    #[test]
    fn restoration_blocks_saves_until_runtime_resolution_commits() {
        let mut state = State::default();
        state.materialize();
        state.advance();
        assert_eq!(state.begin(), None);
        state.resolve();
        assert_eq!(state.begin(), None);
        state.ready();
        assert!(state.begin().is_some());
    }

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Saved;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Transient;

    #[derive(Component, Reflect)]
    struct Root;

    #[test]
    fn restore_ownership_rejects_cycles_and_missing_owners_but_allows_runtime_roots() {
        let root = Entity::from_bits(1);
        let child = Entity::from_bits(2);
        let runtime = Entity::from_bits(3);
        let validator = RestoreValidator::ownership::<Root>();
        let mut snapshot = DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                DynamicEntity {
                    entity: root,
                    components: vec![Box::new(Root), Box::new(ChildOf(runtime))],
                },
                DynamicEntity {
                    entity: child,
                    components: vec![Box::new(ChildOf(root))],
                },
            ],
        };
        assert!((validator.validate)(&snapshot).is_ok());
        snapshot.entities[1].components = vec![Box::new(ChildOf(runtime))];
        assert!(
            (validator.validate)(&snapshot)
                .unwrap_err()
                .contains("missing owner")
        );
        snapshot.entities[1].components = vec![Box::new(ChildOf(root))];
        snapshot.entities[0].components = vec![Box::new(Root), Box::new(ChildOf(child))];
        assert!(
            (validator.validate)(&snapshot)
                .unwrap_err()
                .contains("ownership cycle")
        );
    }

    #[derive(Resource, Default)]
    struct DirtyCount(u32);

    impl DirtyCount {
        fn record(_: On<Dirty<SharedDomain>>, mut count: ResMut<Self>) {
            count.0 += 1;
        }
    }

    #[test]
    fn shared_component_registration_does_not_duplicate_dirty_notifications() {
        let mut app = App::new();
        app.register_domain::<SharedDomain, Saved>()
            .register_domain::<SharedDomain, Saved>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        app.world_mut().spawn((Save, Saved));
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 1);
    }

    #[crate::store(domain = SharedDomain, track; Saved)]
    struct SharedStore;

    #[test]
    fn tracked_schemas_preserve_domain_membership_and_deduplicate_change_tracking() {
        let mut app = App::new();
        app.add_plugins(SchemaPlugin::<SharedStore>::default())
            .register_type::<Transient>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let domain = SharedStored::filter(&registry);
        assert!(domain.is_allowed::<Saved>());
        assert!(!domain.is_allowed::<Transient>());
        assert!(!StoreDomain::<SharedStore>::filter(&registry).is_allowed::<Saved>());
        drop(registry);
        let entity = app.world_mut().spawn((Save, Saved)).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 1);
        app.register_domain::<SharedDomain, Saved>();
        app.world_mut().entity_mut(entity).insert(Saved);
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 2);
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 3);
    }

    #[test]
    fn runtime_component_updates_do_not_request_domain_saves() {
        let mut app = App::new();
        app.register_domain::<SharedDomain, Saved>()
            .track_domain::<SharedDomain, With<Save>, Save>()
            .init_resource::<DirtyCount>()
            .add_observer(DirtyCount::record);
        let entity = app.world_mut().spawn(Saved).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).insert(Saved);
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        app.world_mut().entity_mut(entity).insert((Save, Saved));
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
        app.world_mut().resource_mut::<DirtyCount>().0 = 0;
        app.world_mut().entity_mut(entity).remove::<Saved>();
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
        app.world_mut().resource_mut::<DirtyCount>().0 = 0;
        app.world_mut().entity_mut(entity).despawn();
        app.update();
        assert!(app.world().resource::<DirtyCount>().0 > 0);
    }

    #[crate::store(Saved)]
    struct FeatureStore;

    type FeatureStored = StoreDomain<FeatureStore>;

    #[test]
    fn feature_registration_builds_the_component_allowlist() {
        let mut app = App::new();
        app.register_domain::<SharedDomain, Saved>()
            .register_type::<Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = SharedStored::filter(&registry);

        assert!(filter.is_allowed::<Saved>());
        assert!(!filter.is_allowed::<Transient>());
    }

    #[test]
    fn store_schemas_build_independent_allowlists() {
        let mut app = App::new();
        app.add_plugins(SchemaPlugin::<FeatureStore>::default())
            .register_domain::<SharedDomain, Transient>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let feature = FeatureStored::filter(&registry);
        let shared = SharedStored::filter(&registry);

        assert!(feature.is_allowed::<Saved>());
        assert!(!feature.is_allowed::<Transient>());
        assert!(!shared.is_allowed::<Saved>());
        assert!(shared.is_allowed::<Transient>());
    }

    #[derive(Component, Reflect, Default)]
    #[reflect(Component)]
    struct Note;

    #[derive(Component, Reflect, Default)]
    #[reflect(Component)]
    struct NoteBody(String);

    #[crate::store(filter = With<Note>, track; Note, NoteBody)]
    struct NoteStore;

    #[derive(Resource, Default)]
    struct RecordedWrites(Vec<(Entity, PathBuf, u64, bool)>);

    fn record_note_write(
        request: On<StoreWriteRequest<NoteStore>>,
        mut writes: ResMut<RecordedWrites>,
    ) {
        writes.0.push((
            request.entity,
            request.path.clone(),
            request.revision,
            request.flush,
        ));
    }

    #[test]
    fn autosave_tracks_macro_changes_debounces_and_flushes_periodically() {
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<RecordedWrites>()
            .init_resource::<DirtyCount>()
            .add_plugins((
                SchemaPlugin::<NoteStore>::default(),
                AutoSavePlugin::<NoteStore>::default(),
            ))
            .add_observer(record_note_write)
            .add_observer(DirtyCount::record);

        let store = app
            .world_mut()
            .spawn((
                AutoSave::<NoteStore>::default(),
                StorePath::new("/tmp/notes.ron"),
                AutoSavePolicy::new(
                    Duration::from_millis(500),
                    Some(Duration::from_secs(2)),
                    BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(8)),
                ),
            ))
            .id();

        let note = app.world_mut().spawn((Note, NoteBody("v1".into()))).id();
        app.update();
        assert_eq!(app.world().resource::<DirtyCount>().0, 0);
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(300));
        app.world_mut().get_mut::<NoteBody>(note).unwrap().0 = "v2".into();
        app.update();
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(300));
        app.update();
        assert!(app.world().resource::<RecordedWrites>().0.is_empty());

        clock.advance(Duration::from_millis(200));
        app.update();
        let writes = &app.world().resource::<RecordedWrites>().0;
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, store);
        assert_eq!(writes[0].1, PathBuf::from("/tmp/notes.ron"));
        assert!(!writes[0].3);
        let first_rev = writes[0].2;

        app.world_mut().entity_mut(note).remove::<NoteBody>();
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, first_rev + 99));
        app.update();
        assert_eq!(
            app.world().get::<State>(store).unwrap().saving(),
            Some(first_rev)
        );

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, first_rev));
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);
        let second_rev = app.world().resource::<RecordedWrites>().0[1].2;
        assert!(second_rev > first_rev);
        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::ok(store, second_rev));
        app.update();
        assert!(!app.world().get::<State>(store).unwrap().is_dirty());

        app.world_mut().despawn(note);
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        for _ in 0..5 {
            clock.advance(Duration::from_millis(400));
            app.world_mut().trigger(Dirty::<NoteStore>::new());
            app.update();
        }
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);
    }

    #[test]
    fn autosave_retries_with_backoff_and_flushes_on_exit() {
        let clock = Clock::fake(1_700_000_000_000);
        let mut app = App::new();
        app.insert_resource(clock.clone())
            .init_resource::<RecordedWrites>()
            .add_plugins((
                SchemaPlugin::<NoteStore>::default(),
                AutoSavePlugin::<NoteStore>::default(),
            ))
            .add_observer(record_note_write);

        let store = app
            .world_mut()
            .spawn((
                AutoSave::<NoteStore>::default(),
                StorePath::new("/tmp/retry-notes.ron"),
                AutoSavePolicy::new(
                    Duration::from_millis(100),
                    None,
                    BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(4))
                        .with_max_attempts(2),
                ),
            ))
            .id();

        app.world_mut().spawn((Note, NoteBody("initial".into())));
        app.update();
        clock.advance(Duration::from_millis(100));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);
        let rev = app.world().resource::<RecordedWrites>().0[0].2;

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "disk full"));
        app.update();
        assert!(app.world().get::<State>(store).unwrap().is_dirty());
        assert_eq!(
            app.world().get::<State>(store).unwrap().error(),
            Some("disk full")
        );

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 1);

        clock.advance(Duration::from_millis(500));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "still full"));
        app.update();

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 2);

        clock.advance(Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);

        app.world_mut()
            .trigger(StoreWriteResult::<NoteStore>::err(store, rev, "exhausted"));
        app.update();

        clock.advance(Duration::from_secs(10));
        app.update();
        assert_eq!(app.world().resource::<RecordedWrites>().0.len(), 3);

        app.world_mut().write_message(AppExit::Success);
        app.update();
        let writes = &app.world().resource::<RecordedWrites>().0;
        assert_eq!(writes.len(), 4);
        assert!(writes[3].3);
    }
}
