use std::path::PathBuf;
use std::time::Duration;

use bevy_app::{App, AppExit};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::store::{
    AutoSave, AutoSavePlugin, AutoSavePolicy, SchemaPlugin, State, StorePath, StoreWriteRequest,
    StoreWriteResult,
};
use vmux_app::ecs::time::{BackoffPolicy, Clock};

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
struct Entry;

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
struct Label(String);

#[vmux_ecs::store(track; Entry, Label)]
struct EntryStore;

#[derive(Resource, Default)]
struct WriteLog {
    requests: Vec<(Entity, PathBuf, u64, bool)>,
    fail_remaining: usize,
    auto_complete: bool,
}

fn handle_write(
    request: On<StoreWriteRequest<EntryStore>>,
    mut log: ResMut<WriteLog>,
    mut commands: Commands,
) {
    log.requests.push((
        request.entity,
        request.path.clone(),
        request.revision,
        request.flush,
    ));
    if !log.auto_complete {
        return;
    }
    if log.fail_remaining > 0 {
        log.fail_remaining -= 1;
        commands.trigger(StoreWriteResult::<EntryStore>::err(
            request.entity,
            request.revision,
            "simulated disk error",
        ));
    } else {
        commands.trigger(StoreWriteResult::<EntryStore>::ok(
            request.entity,
            request.revision,
        ));
    }
}

#[test]
fn macro_dirty_tracking_debounces_tracks_single_flight_and_flushes_on_exit() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .insert_resource(WriteLog {
            requests: Vec::new(),
            fail_remaining: 0,
            auto_complete: false,
        })
        .add_plugins((
            VmuxPlugin::builder().build(),
            SchemaPlugin::<EntryStore>::default(),
            AutoSavePlugin::<EntryStore>::default(),
        ))
        .add_observer(handle_write);

    let store = app
        .world_mut()
        .spawn((
            AutoSave::<EntryStore>::default(),
            StorePath::new("/tmp/consumer-entries.ron"),
            AutoSavePolicy::new(
                Duration::from_millis(500),
                Some(Duration::from_secs(2)),
                BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(8)),
            ),
        ))
        .id();

    let entry = app.world_mut().spawn((Entry, Label("first".into()))).id();
    app.update();
    assert!(app.world().get::<State>(store).unwrap().is_dirty());
    assert!(app.world().resource::<WriteLog>().requests.is_empty());

    clock.advance(Duration::from_millis(300));
    app.world_mut().get_mut::<Label>(entry).unwrap().0 = "second".into();
    app.update();
    assert!(app.world().resource::<WriteLog>().requests.is_empty());

    clock.advance(Duration::from_millis(300));
    app.update();
    assert!(app.world().resource::<WriteLog>().requests.is_empty());

    clock.advance(Duration::from_millis(200));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 1);
    let first_rev = app.world().resource::<WriteLog>().requests[0].2;
    assert_eq!(
        app.world().get::<State>(store).unwrap().saving(),
        Some(first_rev)
    );

    app.world_mut().entity_mut(entry).remove::<Label>();
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 1);

    app.world_mut()
        .trigger(StoreWriteResult::<EntryStore>::ok(store, first_rev + 10));
    app.update();
    assert_eq!(
        app.world().get::<State>(store).unwrap().saving(),
        Some(first_rev)
    );

    app.world_mut()
        .trigger(StoreWriteResult::<EntryStore>::ok(store, first_rev));
    app.update();
    assert!(app.world().get::<State>(store).unwrap().is_dirty());
    assert_eq!(
        app.world().get::<State>(store).unwrap().loaded(),
        Some(first_rev)
    );

    clock.advance(Duration::from_millis(500));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 2);
    let second_rev = app.world().resource::<WriteLog>().requests[1].2;
    assert!(second_rev > first_rev);
    app.world_mut()
        .trigger(StoreWriteResult::<EntryStore>::ok(store, second_rev));
    app.update();
    assert!(!app.world().get::<State>(store).unwrap().is_dirty());

    app.world_mut().despawn(entry);
    app.update();
    assert!(app.world().get::<State>(store).unwrap().is_dirty());
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 2);

    app.world_mut().write_message(AppExit::Success);
    app.update();
    let requests = &app.world().resource::<WriteLog>().requests;
    assert_eq!(requests.len(), 3);
    assert!(requests[2].3);
    assert_eq!(requests[2].1, PathBuf::from("/tmp/consumer-entries.ron"));
}

#[test]
fn autosave_backs_off_exponentially_and_recovers_when_storage_succeeds() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .insert_resource(WriteLog {
            requests: Vec::new(),
            fail_remaining: 2,
            auto_complete: true,
        })
        .add_plugins((
            VmuxPlugin::builder().build(),
            SchemaPlugin::<EntryStore>::default(),
            AutoSavePlugin::<EntryStore>::default(),
        ))
        .add_observer(handle_write);

    let store = app
        .world_mut()
        .spawn((
            AutoSave::<EntryStore>::default(),
            StorePath::new("/tmp/consumer-retry.ron"),
            AutoSavePolicy::new(
                Duration::from_millis(100),
                None,
                BackoffPolicy::exponential(Duration::from_secs(1), Duration::from_secs(4)),
            ),
        ))
        .id();

    app.world_mut().spawn((Entry, Label("retry".into())));
    app.update();
    clock.advance(Duration::from_millis(100));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 1);
    assert!(app.world().get::<State>(store).unwrap().is_dirty());
    assert_eq!(
        app.world().get::<State>(store).unwrap().error(),
        Some("simulated disk error")
    );

    clock.advance(Duration::from_millis(900));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 1);

    clock.advance(Duration::from_millis(100));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 2);
    assert!(app.world().get::<State>(store).unwrap().is_dirty());

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 2);

    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().resource::<WriteLog>().requests.len(), 3);
    let state = app.world().get::<State>(store).unwrap();
    assert!(!state.is_dirty());
    assert!(state.error().is_none());
    assert!(state.loaded().is_some());
}
