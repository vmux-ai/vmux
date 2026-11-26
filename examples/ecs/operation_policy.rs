use bevy_app::App;
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{
    Operation, OperationCompleted, OperationPolicy, OperationState, OperationTarget,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().jobs(true).build());

    let target = app.world_mut().spawn(Name::new("Document")).id();
    let first = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.save").unwrap(),
            OperationTarget::new(target),
            OperationPolicy::SerializePerTarget,
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.save").unwrap(),
            OperationTarget::new(target),
            OperationPolicy::SerializePerTarget,
        ))
        .id();

    app.update();
    println!(
        "queued: first={:?} second={:?}",
        app.world().get::<OperationState>(first).unwrap(),
        app.world().get::<OperationState>(second).unwrap(),
    );

    app.world_mut().trigger(OperationCompleted {
        entity: first,
        result: Ok(()),
    });
    app.update();
    println!(
        "after first completed: first={:?} second={:?}",
        app.world().get::<OperationState>(first).unwrap(),
        app.world().get::<OperationState>(second).unwrap(),
    );

    let older = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.sync").unwrap().running(),
            OperationTarget::new(target),
            OperationPolicy::SupersedePrevious,
        ))
        .id();
    let newer = app
        .world_mut()
        .spawn((
            Operation::bundle("doc.sync").unwrap(),
            OperationTarget::new(target),
            OperationPolicy::SupersedePrevious,
        ))
        .id();
    app.update();
    println!(
        "superseded: older={:?} newer={:?}",
        app.world().get::<OperationState>(older).unwrap(),
        app.world().get::<OperationState>(newer).unwrap(),
    );
}
