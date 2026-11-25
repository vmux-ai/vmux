use bevy_app::{App, PostUpdate};
use bevy_ecs::prelude::*;
use vmux_app::ecs::{
    Dirty, DirtyPlugin, DirtySet, Generation, GenerationPlugin, Operation, OperationState, Stale,
};

#[derive(Clone, Copy)]
struct DocEpoch;

#[derive(Clone, Copy)]
struct OutlineView;

#[derive(Component)]
struct Title(&'static str);

#[derive(Component, Default)]
struct Outline(String);

fn project_outline(mut query: Query<(&Title, &mut Outline), With<Dirty<OutlineView>>>) {
    for (title, mut outline) in &mut query {
        outline.0 = title.0.to_uppercase();
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        GenerationPlugin::<DocEpoch>::default(),
        DirtyPlugin::<OutlineView>::default(),
    ))
    .add_systems(
        PostUpdate,
        project_outline.before(DirtySet::of::<OutlineView>()),
    );

    let doc = app
        .world_mut()
        .spawn((
            Generation::<DocEpoch>::new(1),
            Title("hello"),
            Outline::default(),
            Dirty::<OutlineView>::new(),
        ))
        .id();
    let snapshot = app
        .world()
        .get::<Generation<DocEpoch>>(doc)
        .unwrap()
        .snapshot();
    let index_op = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.index").unwrap().running(),
            ChildOf(doc),
            snapshot,
        ))
        .id();

    app.update();
    println!(
        "v1 outline={} op_state={:?} stale={}",
        app.world().get::<Outline>(doc).unwrap().0,
        app.world().get::<OperationState>(index_op).unwrap(),
        app.world().get::<Stale<DocEpoch>>(index_op).is_some(),
    );

    app.world_mut()
        .get_mut::<Generation<DocEpoch>>(doc)
        .unwrap()
        .advance();
    app.world_mut()
        .entity_mut(doc)
        .insert((Title("updated"), Dirty::<OutlineView>::new()));
    app.update();
    println!(
        "v2 outline={} op_state={:?} stale={}",
        app.world().get::<Outline>(doc).unwrap().0,
        app.world().get::<OperationState>(index_op).unwrap(),
        app.world().get::<Stale<DocEpoch>>(index_op).is_some(),
    );
}
