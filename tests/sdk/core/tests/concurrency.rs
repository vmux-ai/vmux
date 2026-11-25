use bevy_app::{App, PostUpdate};
use bevy_ecs::prelude::*;
use vmux_app::ecs::{
    AtGeneration, Dirty, DirtyPlugin, DirtySet, Generation, GenerationPlugin, GlobalSequence,
    LeaseGranted, LeaseMode, LeasePlugin, LeaseRequest, Operation, OperationState, OperationTarget,
    SelectRequest, Selected, SelectionPlugin, SequenceCounter, Stale,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FileKey(&'static str);

#[derive(Clone, Copy)]
struct DocumentEpoch;

#[derive(Clone, Copy)]
struct OutlineView;

#[derive(Clone, Copy)]
struct ListScope;

#[derive(Component)]
struct Title(&'static str);

#[derive(Component, Default)]
struct OutlineEntry(String);

#[derive(Resource, Default)]
struct ProjectionPasses(u32);

fn rebuild_outline(
    mut query: Query<(&Title, &mut OutlineEntry), With<Dirty<OutlineView>>>,
    mut passes: ResMut<ProjectionPasses>,
) {
    for (title, mut entry) in &mut query {
        entry.0 = title.0.to_uppercase();
        passes.0 += 1;
    }
}

#[test]
fn sdk_consumer_leases_arbitrate_shared_and_exclusive_waiters_in_fifo_order() {
    let mut app = App::new();
    app.add_plugins(LeasePlugin::<FileKey>::default());

    let writer_a = app
        .world_mut()
        .spawn(LeaseRequest::exclusive(FileKey("a.rs")))
        .id();
    let reader_a = app
        .world_mut()
        .spawn(LeaseRequest::shared(FileKey("a.rs")))
        .id();
    let reader_b = app
        .world_mut()
        .spawn(LeaseRequest::shared(FileKey("a.rs")))
        .id();
    let writer_b = app
        .world_mut()
        .spawn(LeaseRequest::exclusive(FileKey("a.rs")))
        .id();

    app.update();

    assert_eq!(
        app.world()
            .get::<LeaseGranted<FileKey>>(writer_a)
            .map(LeaseGranted::mode),
        Some(LeaseMode::Exclusive)
    );
    assert!(app.world().get::<LeaseGranted<FileKey>>(reader_a).is_none());
    assert!(app.world().get::<LeaseGranted<FileKey>>(reader_b).is_none());
    assert!(app.world().get::<LeaseGranted<FileKey>>(writer_b).is_none());

    app.world_mut().despawn(writer_a);

    assert_eq!(
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_a)
            .map(LeaseGranted::mode),
        Some(LeaseMode::Shared)
    );
    assert_eq!(
        app.world()
            .get::<LeaseGranted<FileKey>>(reader_b)
            .map(LeaseGranted::mode),
        Some(LeaseMode::Shared)
    );
    assert!(app.world().get::<LeaseGranted<FileKey>>(writer_b).is_none());

    app.world_mut()
        .entity_mut(reader_a)
        .remove::<LeaseGranted<FileKey>>();
    assert!(app.world().get::<LeaseGranted<FileKey>>(writer_b).is_none());

    app.world_mut()
        .entity_mut(reader_b)
        .remove::<LeaseGranted<FileKey>>();
    assert_eq!(
        app.world()
            .get::<LeaseGranted<FileKey>>(writer_b)
            .map(LeaseGranted::mode),
        Some(LeaseMode::Exclusive)
    );
}

#[test]
fn sdk_consumer_generation_sequences_and_stale_work_guarding_cancel_outdated_operations() {
    let mut app = App::new();
    app.add_plugins(GenerationPlugin::<DocumentEpoch>::default())
        .init_resource::<GlobalSequence<DocumentEpoch>>();

    let first_global = app
        .world_mut()
        .resource_mut::<GlobalSequence<DocumentEpoch>>()
        .next();
    let second_global = app
        .world_mut()
        .resource_mut::<GlobalSequence<DocumentEpoch>>()
        .next();
    assert_eq!((first_global, second_global), (0, 1));

    let owner = app
        .world_mut()
        .spawn((
            Generation::<DocumentEpoch>::new(1),
            SequenceCounter::<DocumentEpoch>::starting_at(100),
        ))
        .id();
    let seq = app
        .world_mut()
        .get_mut::<SequenceCounter<DocumentEpoch>>(owner)
        .unwrap()
        .next();
    assert_eq!(seq, 100);

    let snapshot = app
        .world()
        .get::<Generation<DocumentEpoch>>(owner)
        .unwrap()
        .snapshot();
    assert_eq!(snapshot, AtGeneration::<DocumentEpoch>::new(1));

    let child_work = app
        .world_mut()
        .spawn((
            Operation::bundle("consumer.index").unwrap().running(),
            ChildOf(owner),
            snapshot,
        ))
        .id();
    let targeted_work = app
        .world_mut()
        .spawn((
            Operation::bundle("consumer.save").unwrap(),
            OperationTarget::new(owner),
            snapshot,
        ))
        .id();

    app.update();
    assert!(app.world().get::<Stale<DocumentEpoch>>(child_work).is_none());
    assert!(
        app.world()
            .get::<Stale<DocumentEpoch>>(targeted_work)
            .is_none()
    );

    app.world_mut()
        .get_mut::<Generation<DocumentEpoch>>(owner)
        .unwrap()
        .advance();
    app.update();

    assert!(app.world().get::<Stale<DocumentEpoch>>(child_work).is_some());
    assert_eq!(
        app.world().get::<OperationState>(child_work),
        Some(&OperationState::Canceled)
    );
    assert!(
        app.world()
            .get::<Stale<DocumentEpoch>>(targeted_work)
            .is_some()
    );
    assert_eq!(
        app.world().get::<OperationState>(targeted_work),
        Some(&OperationState::Canceled)
    );
}

#[test]
fn sdk_consumer_dirty_projections_and_scoped_selection_work_through_vmux_app_ecs() {
    let mut app = App::new();
    app.add_plugins((
        DirtyPlugin::<OutlineView>::default(),
        SelectionPlugin::<ListScope>::default(),
    ))
    .init_resource::<ProjectionPasses>()
    .add_systems(
        PostUpdate,
        rebuild_outline.before(DirtySet::of::<OutlineView>()),
    );

    let list = app.world_mut().spawn_empty().id();
    let first = app
        .world_mut()
        .spawn((
            ChildOf(list),
            Title("alpha"),
            OutlineEntry::default(),
            Dirty::<OutlineView>::new(),
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((ChildOf(list), Title("beta"), OutlineEntry::default()))
        .id();

    app.world_mut()
        .write_message(SelectRequest::<ListScope>::select(first));
    app.update();

    assert_eq!(app.world().get::<OutlineEntry>(first).unwrap().0, "ALPHA");
    assert_eq!(app.world().resource::<ProjectionPasses>().0, 1);
    assert!(app.world().get::<Dirty<OutlineView>>(first).is_none());
    assert!(app.world().get::<Selected<ListScope>>(first).is_some());
    assert!(app.world().get::<Selected<ListScope>>(second).is_none());

    app.world_mut()
        .write_message(SelectRequest::<ListScope>::select(second));
    app.update();

    assert_eq!(app.world().resource::<ProjectionPasses>().0, 1);
    assert!(app.world().get::<Selected<ListScope>>(first).is_none());
    assert!(app.world().get::<Selected<ListScope>>(second).is_some());

    let other_list = app.world_mut().spawn_empty().id();
    app.world_mut()
        .entity_mut(second)
        .insert(ChildOf(other_list));
    assert!(app.world().get::<Selected<ListScope>>(second).is_none());
}
