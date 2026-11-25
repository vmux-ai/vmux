use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::time::{Clock, DeadlineScheduler};
use vmux_app::ecs::{
    Operation, OperationAdmitted, OperationCompleted, OperationError, OperationPolicy,
    OperationRetry, OperationRetryRequested, OperationRetryWaiting, OperationSet, OperationState,
    OperationTarget, OperationTimeout,
};

#[derive(Component)]
struct Label(&'static str);

#[derive(Resource, Default)]
struct Log(Vec<&'static str>);

fn record_admitted(
    admitted: Query<&Label, Added<OperationAdmitted>>,
    mut log: ResMut<Log>,
) {
    for label in &admitted {
        log.0.push(label.0);
    }
}

#[test]
fn operation_policies_cover_fifo_supersede_timeout_retry_and_target_despawn_via_vmux_app() {
    let clock = Clock::fake(1_700_000_000_000);
    let mut app = App::new();
    app.insert_resource(clock.clone())
        .init_resource::<Log>()
        .add_plugins(VmuxPlugin::builder().jobs(true).build())
        .add_systems(Update, record_admitted.after(OperationSet::Admit));

    let fifo_target = app.world_mut().spawn_empty().id();
    let first = app
        .world_mut()
        .spawn((
            Operation::bundle("queue.step").unwrap(),
            OperationTarget::new(fifo_target),
            OperationPolicy::SerializePerTarget,
            Label("first"),
        ))
        .id();
    let second = app
        .world_mut()
        .spawn((
            Operation::bundle("queue.step").unwrap(),
            OperationTarget::new(fifo_target),
            OperationPolicy::SerializePerTarget,
            Label("second"),
        ))
        .id();
    let third = app
        .world_mut()
        .spawn((
            Operation::bundle("queue.step").unwrap(),
            OperationTarget::new(fifo_target),
            OperationPolicy::SerializePerTarget,
            Label("third"),
        ))
        .id();

    app.update();
    assert_eq!(app.world().resource::<Log>().0, vec!["first"]);
    assert_eq!(
        app.world().get::<OperationState>(first),
        Some(&OperationState::Running)
    );
    assert_eq!(
        app.world().get::<OperationState>(second),
        Some(&OperationState::Pending)
    );
    assert_eq!(
        app.world().get::<OperationState>(third),
        Some(&OperationState::Pending)
    );

    app.world_mut().despawn(first);
    app.update();
    assert_eq!(app.world().resource::<Log>().0, vec!["first", "second"]);
    assert_eq!(
        app.world().get::<OperationState>(second),
        Some(&OperationState::Running)
    );

    app.world_mut().trigger(OperationCompleted {
        entity: second,
        result: Ok(()),
    });
    app.update();
    assert_eq!(
        app.world().resource::<Log>().0,
        vec!["first", "second", "third"]
    );
    assert_eq!(
        app.world().get::<OperationState>(third),
        Some(&OperationState::Running)
    );
    app.world_mut().trigger(OperationCompleted {
        entity: third,
        result: Ok(()),
    });
    app.update();

    let supersede_target = app.world_mut().spawn_empty().id();
    let older = app
        .world_mut()
        .spawn((
            Operation::bundle("sync.state").unwrap().running(),
            OperationTarget::new(supersede_target),
            OperationPolicy::SupersedePrevious,
            OperationTimeout::from_secs(30),
        ))
        .id();
    let unrelated = app
        .world_mut()
        .spawn((
            Operation::bundle("sync.other").unwrap().running(),
            OperationTarget::new(supersede_target),
            OperationPolicy::SupersedePrevious,
        ))
        .id();
    let newer = app
        .world_mut()
        .spawn((
            Operation::bundle("sync.state").unwrap(),
            OperationTarget::new(supersede_target),
            OperationPolicy::SupersedePrevious,
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(older),
        Some(&OperationState::Canceled)
    );
    assert_eq!(
        app.world().get::<OperationState>(newer),
        Some(&OperationState::Pending)
    );
    assert_eq!(
        app.world().get::<OperationState>(unrelated),
        Some(&OperationState::Running)
    );
    assert_eq!(
        app.world().resource::<DeadlineScheduler>().pending_count(),
        0
    );

    let timed = app
        .world_mut()
        .spawn((
            Operation::bundle("rpc.call").unwrap().running(),
            OperationTimeout::from_secs(4),
        ))
        .id();
    clock.advance(Duration::from_secs(3));
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(timed),
        Some(&OperationState::Running)
    );
    clock.advance(Duration::from_secs(1));
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(timed),
        Some(&OperationState::Failed)
    );
    assert_eq!(
        app.world().get::<OperationError>(timed).unwrap().message(),
        "Operation timed out"
    );

    let retried = app
        .world_mut()
        .spawn((
            Operation::bundle("sync.push").unwrap().running(),
            OperationRetry::exponential(Duration::from_secs(2), Duration::from_secs(6))
                .with_max_attempts(2),
        ))
        .id();
    app.world_mut()
        .trigger(OperationRetryRequested::new(retried, "transient timeout"));
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(retried),
        Some(&OperationState::Pending)
    );
    assert!(app.world().get::<OperationRetryWaiting>(retried).is_some());
    clock.advance(Duration::from_secs(2));
    app.update();
    assert!(app.world().get::<OperationRetryWaiting>(retried).is_none());
    app.world_mut()
        .entity_mut(retried)
        .insert(OperationState::Running);
    app.world_mut()
        .trigger(OperationRetryRequested::new(retried, "transient 502"));
    app.update();
    clock.advance(Duration::from_secs(4));
    app.update();
    assert!(app.world().get::<OperationRetryWaiting>(retried).is_none());
    app.world_mut()
        .entity_mut(retried)
        .insert(OperationState::Running);
    app.world_mut()
        .trigger(OperationRetryRequested::new(retried, "permanent 502"));
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(retried),
        Some(&OperationState::Failed)
    );
    assert_eq!(
        app.world()
            .get::<OperationError>(retried)
            .unwrap()
            .message(),
        "permanent 502"
    );

    let despawn_target = app.world_mut().spawn_empty().id();
    let watched = app
        .world_mut()
        .spawn((
            Operation::bundle("target.work").unwrap(),
            OperationTarget::new(despawn_target),
            OperationPolicy::SerializePerTarget,
            OperationTimeout::from_secs(10),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(watched),
        Some(&OperationState::Running)
    );
    assert_eq!(
        app.world().resource::<DeadlineScheduler>().pending_count(),
        1
    );
    app.world_mut().despawn(despawn_target);
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(watched),
        Some(&OperationState::Canceled)
    );
    assert!(app.world().get::<OperationAdmitted>(watched).is_none());
    assert_eq!(
        app.world().resource::<DeadlineScheduler>().pending_count(),
        0
    );
}
