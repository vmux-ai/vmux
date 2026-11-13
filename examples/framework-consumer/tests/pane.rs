use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemState;
use moonshine_save::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::LastActivatedAt;
use vmux_app::ecs::store::Types;
use vmux_app::layout::{
    Hierarchy, Pane, PaneId, PaneSplit, PaneSplitDirection, PaneStore, SelectRequest, StorePlugin,
};

#[test]
fn a_public_pane_plugin_selects_current_scoped_leaves_without_a_host() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().layout(true).build());
    let root = app.world_mut().spawn(Name::new("Application root")).id();
    let other = app.world_mut().spawn(Name::new("Other root")).id();
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            ChildOf(root),
        ))
        .id();
    let first = app
        .world_mut()
        .spawn((Pane, PaneId("shared".into()), ChildOf(split)))
        .id();
    let second = app
        .world_mut()
        .spawn((Pane, PaneId("second".into()), ChildOf(split)))
        .id();
    let foreign = app
        .world_mut()
        .spawn((Pane, PaneId("shared".into()), ChildOf(other)))
        .id();
    app.world_mut()
        .write_message(SelectRequest { root, pane: second });
    app.world_mut().write_message(SelectRequest {
        root,
        pane: foreign,
    });
    app.update();
    assert!(app.world().get::<LastActivatedAt>(second).is_some());
    assert!(app.world().get::<LastActivatedAt>(foreign).is_none());
    assert!(app.world().get::<Save>(first).is_none());
    let mut state = SystemState::<Hierarchy>::new(app.world_mut());
    let hierarchy = state.get(app.world()).unwrap();
    assert_eq!(
        hierarchy.neighbor_in(root, second, PaneSplitDirection::Row, false),
        Some(first)
    );
}

#[test]
fn opt_in_pane_store_restores_the_historical_components_and_topology() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("panes.ron");
    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().layout(true).build(),
        bevy_app::TaskPoolPlugin::default(),
        bevy_asset::AssetPlugin::default(),
        StorePlugin,
    ))
    .add_observer(save_on::<SaveWorld<With<Pane>>>);
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneId("root".into()),
            PaneSplit {
                direction: PaneSplitDirection::Column,
            },
        ))
        .id();
    app.world_mut()
        .spawn((Pane, PaneId("leaf".into()), ChildOf(root)));
    let mut save = SaveWorld::<With<Pane>>::into_file(path.clone());
    app.world().resource::<Types<PaneStore>>().apply(&mut save);
    app.world_mut().trigger_save(save);
    app.update();
    let mut restored = App::new();
    restored
        .add_plugins((
            VmuxPlugin::builder().layout(true).build(),
            bevy_app::TaskPoolPlugin::default(),
            bevy_asset::AssetPlugin::default(),
            StorePlugin,
        ))
        .add_observer(load_on::<LoadWorld>);
    restored
        .world_mut()
        .trigger_load(LoadWorld::default_from_file(path));
    restored.update();
    let mut query = restored
        .world_mut()
        .query::<(Entity, &PaneId, Option<&PaneSplit>, Option<&ChildOf>)>();
    let mut parent = None;
    let mut leaf_parent = None;
    for (entity, id, split, owner) in query.iter(restored.world()) {
        if id.0 == "root" {
            assert_eq!(split.unwrap().direction, PaneSplitDirection::Column);
            parent = Some(entity);
        } else if id.0 == "leaf" {
            leaf_parent = owner.map(ChildOf::parent);
        }
    }
    assert_eq!(leaf_parent.unwrap(), parent.unwrap());
}
