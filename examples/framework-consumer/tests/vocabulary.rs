use std::path::Path;

use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::ecs::{
    Busy, Collapsed, ContentDigest, Description, Disabled, Failure, FilePath, Icon, Loading,
    Operation, OperationPlugin, OperationState, OperationTarget, Order, OrderPlugin, Pin,
    PrimitivesPlugin, ProfileId, Progress, Ready, RenamePlugin, RenameRequest, Renamed,
    ReorderRequest, Reordered, Retired, StatusPlugin, Toggle, TogglePlugin, Toggled, TogglesPlugin,
};

#[derive(Component, Default)]
struct Archived;

#[derive(Resource, Default)]
struct ObservedEvents {
    renamed: Vec<(Entity, Option<String>, String)>,
    reordered: Vec<(Entity, u32)>,
    toggled_disabled: Vec<(Entity, bool)>,
    toggled_archived: Vec<(Entity, bool)>,
}

fn on_renamed(trigger: On<Renamed>, mut observed: ResMut<ObservedEvents>) {
    let event = trigger.event();
    observed
        .renamed
        .push((event.entity, event.previous.clone(), event.name.clone()));
}

fn on_reordered(trigger: On<Reordered>, mut observed: ResMut<ObservedEvents>) {
    let event = trigger.event();
    observed.reordered.push((event.entity, event.order));
}

fn on_disabled(trigger: On<Toggled<Disabled>>, mut observed: ResMut<ObservedEvents>) {
    let event = trigger.event();
    observed.toggled_disabled.push((event.entity, event.enabled));
}

fn on_archived(trigger: On<Toggled<Archived>>, mut observed: ResMut<ObservedEvents>) {
    let event = trigger.event();
    observed.toggled_archived.push((event.entity, event.enabled));
}

#[test]
fn vocabulary_plugins_compose_headlessly_through_vmux_app_ecs() {
    let mut app = App::new();
    app.add_plugins((
        PrimitivesPlugin,
        OperationPlugin,
        RenamePlugin,
        OrderPlugin,
        TogglesPlugin,
        TogglePlugin::<Archived>::default(),
        StatusPlugin,
    ))
    .init_resource::<ObservedEvents>()
    .add_observer(on_renamed)
    .add_observer(on_reordered)
    .add_observer(on_disabled)
    .add_observer(on_archived);

    let root = app
        .world_mut()
        .spawn((
            Name::new("Workspace"),
            Description("Headless consumer workspace".into()),
            Icon::new("folder"),
            ProfileId("default".into()),
        ))
        .id();

    app.world_mut()
        .trigger(RenameRequest::new(root, "   "));
    app.world_mut()
        .trigger(RenameRequest::new(root, "  Notes Workspace  "));
    app.world_mut().flush();

    assert_eq!(
        app.world().get::<Name>(root).map(Name::as_str),
        Some("Notes Workspace")
    );
    assert_eq!(
        app.world().get::<Icon>(root).map(Icon::as_str),
        Some("folder")
    );
    assert_eq!(
        app.world().get::<ProfileId>(root).map(|id| id.0.as_str()),
        Some("default")
    );

    let first = app
        .world_mut()
        .spawn((Name::new("First"), Order(10), ChildOf(root)))
        .id();
    let second = app
        .world_mut()
        .spawn((Name::new("Second"), Order(2), ChildOf(root)))
        .id();
    let third = app
        .world_mut()
        .spawn((Name::new("Third"), Order::END, ChildOf(root)))
        .id();
    app.update();

    assert_eq!(app.world().get::<Order>(second), Some(&Order(0)));
    assert_eq!(app.world().get::<Order>(first), Some(&Order(1)));
    assert_eq!(app.world().get::<Order>(third), Some(&Order(2)));

    app.world_mut().trigger(ReorderRequest::before(third, second));
    app.world_mut().flush();
    assert_eq!(app.world().get::<Order>(third), Some(&Order(0)));
    assert_eq!(app.world().get::<Order>(second), Some(&Order(1)));
    assert_eq!(app.world().get::<Order>(first), Some(&Order(2)));

    app.world_mut().despawn(second);
    app.update();
    assert_eq!(app.world().get::<Order>(third), Some(&Order(0)));
    assert_eq!(app.world().get::<Order>(first), Some(&Order(1)));

    app.world_mut().trigger(Toggle::<Pin>::new(first));
    app.world_mut().trigger(Toggle::<Collapsed>::new(first));
    app.world_mut().trigger(Toggle::<Disabled>::new(first));
    app.world_mut().trigger(Toggle::<Archived>::new(first));
    app.world_mut().flush();
    assert!(app.world().get::<Pin>(first).is_some());
    assert!(app.world().get::<Collapsed>(first).is_some());
    assert!(app.world().get::<Disabled>(first).is_some());
    assert!(app.world().get::<Archived>(first).is_some());

    app.world_mut().trigger(Toggle::<Disabled>::new(first));
    app.world_mut().flush();
    assert!(app.world().get::<Disabled>(first).is_none());

    let observed = app.world().resource::<ObservedEvents>();
    assert_eq!(
        observed.renamed,
        vec![(root, Some("Workspace".into()), "Notes Workspace".into())]
    );
    assert_eq!(observed.reordered, vec![(third, 0)]);
    assert_eq!(
        observed.toggled_disabled,
        vec![(first, true), (first, false)]
    );
    assert_eq!(observed.toggled_archived, vec![(first, true)]);
}

#[test]
fn status_hooks_and_resource_addressing_compose_without_host_features() {
    let mut app = App::new();
    app.add_plugins((PrimitivesPlugin, OperationPlugin, StatusPlugin));

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sub/../doc.txt");
    std::fs::write(dir.path().join("doc.txt"), b"vmux").unwrap();
    let path = FilePath::resolve(&file);
    let digest = ContentDigest::from_file(&path).unwrap();
    assert_eq!(path.as_path(), dir.path().canonicalize().unwrap().join("doc.txt"));
    assert_eq!(FilePath::lexical("a/./b/../c").as_path(), Path::new("a/c"));
    assert!(digest.matches_bytes(b"vmux"));
    assert_eq!(ContentDigest::parse(digest.as_str()).unwrap(), digest);

    let document = app
        .world_mut()
        .spawn((
            path,
            digest,
            Loading,
            Busy,
            Progress::new(1, Some(2)).with_label("Syncing"),
        ))
        .id();
    app.world_mut().flush();
    assert!(app.world().get::<Loading>(document).is_some());

    app.world_mut()
        .entity_mut(document)
        .insert(Failure::new("io error"));
    app.world_mut().flush();
    assert!(app.world().get::<Loading>(document).is_none());
    assert_eq!(
        app.world().get::<Failure>(document).map(Failure::message),
        Some("io error")
    );

    app.world_mut().entity_mut(document).insert(Ready);
    app.world_mut().flush();
    assert!(app.world().get::<Ready>(document).is_some());
    assert!(app.world().get::<Failure>(document).is_none());

    let child_op = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.sync").unwrap().running(),
            ChildOf(document),
        ))
        .id();
    let targeted_op = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.index").unwrap(),
            OperationTarget::new(document),
        ))
        .id();
    let finished_op = app
        .world_mut()
        .spawn((Operation, OperationState::Succeeded, ChildOf(document)))
        .id();

    app.world_mut().entity_mut(document).insert(Retired);
    app.world_mut().flush();

    assert!(app.world().get::<Retired>(document).is_some());
    assert!(app.world().get::<Busy>(document).is_none());
    assert_eq!(
        app.world().get::<OperationState>(child_op),
        Some(&OperationState::Canceled)
    );
    assert_eq!(
        app.world().get::<OperationState>(targeted_op),
        Some(&OperationState::Canceled)
    );
    assert_eq!(
        app.world().get::<OperationState>(finished_op),
        Some(&OperationState::Succeeded)
    );
}
