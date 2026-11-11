use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_app::VmuxPlugin;
use vmux_ecs::blocking::{BlockingCancellation, BlockingPool, BlockingRuntime};
use vmux_ecs::{
    Job, JobOwner, JobPlugin, JobReady, JobRequest, JobResult, JobSet, Operation,
    OperationCompleted, OperationError, OperationState, OperationTarget,
};

struct Feature;

impl Plugin for Feature {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            JobPlugin::<Rename>::default(),
        ))
        .add_message::<Commit>()
        .add_message::<Reparent>()
        .add_systems(Update, (reparent, commit).chain().in_set(JobSet::Apply));
    }
}

#[derive(Clone)]
struct Rename(String);

impl Job for Rename {
    type Output = Result<String, String>;

    fn run(self) -> Self::Output {
        let name = self.0.trim();
        if name == "panic" {
            panic!("worker panic");
        }
        if name.is_empty() {
            return Err("empty name".to_string());
        }
        Ok(name.to_string())
    }
}

#[derive(Message)]
struct Commit(Entity);

#[derive(Message)]
struct Reparent {
    operation: Entity,
    parent: Entity,
}

fn reparent(mut requests: MessageReader<Reparent>, mut commands: Commands) {
    for request in requests.read() {
        commands
            .entity(request.operation)
            .insert(ChildOf(request.parent));
    }
}

fn commit(
    mut requests: MessageReader<Commit>,
    results: Query<(
        &ChildOf,
        &JobOwner<Rename>,
        &JobResult<Rename>,
        &OperationState,
    )>,
    mut names: Query<&mut Name>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((owner, accepted, result, state)) = results.get(request.0) else {
            continue;
        };
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !accepted.accepts(Some(owner))
        {
            continue;
        }
        let outcome = match &result.0 {
            Ok(Ok(name)) => {
                *names.get_mut(owner.parent()).unwrap() = Name::new(name.clone());
                Ok(())
            }
            Ok(Err(error)) | Err(error) => Err(error.clone()),
        };
        commands.trigger(OperationCompleted {
            entity: request.0,
            result: outcome,
        });
    }
}

struct Consumer {
    app: App,
    owner: Entity,
}

impl Consumer {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins(Feature);
        let owner = app.world_mut().spawn(Name::new("Original")).id();
        Self { app, owner }
    }

    fn submit(&mut self, name: &str) -> Entity {
        self.app
            .world_mut()
            .spawn((
                Operation::bundle("rename").unwrap(),
                JobRequest(Rename(name.into())),
                ChildOf(self.owner),
            ))
            .id()
    }

    fn result(&mut self, entity: Entity) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.app.update();
            let ready = self
                .app
                .world_mut()
                .resource_mut::<Messages<JobReady<Rename>>>()
                .drain()
                .any(|ready| ready.entity == entity);
            if ready {
                assert!(self.app.world().get::<JobResult<Rename>>(entity).is_some());
                return;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    fn commit(&mut self, entity: Entity) {
        self.app.world_mut().write_message(Commit(entity));
        self.app.update();
    }
}

#[test]
fn successful_work_waits_for_domain_commit_and_releases_its_payload_afterward() {
    let mut consumer = Consumer::new();
    let operation = consumer.submit("Updated");
    consumer.result(operation);
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Running)
    );
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Original"
    );
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Updated"
    );
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Succeeded)
    );
    assert!(
        consumer
            .app
            .world()
            .get::<JobResult<Rename>>(operation)
            .is_none()
    );
    assert!(consumer.app.world().get::<Operation>(operation).is_some());
}

#[test]
fn a_domain_failure_does_not_become_success_when_the_worker_returns() {
    let mut consumer = Consumer::new();
    let operation = consumer.submit("");
    consumer.result(operation);
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Running)
    );
    consumer.commit(operation);
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Failed)
    );
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Original"
    );
}

#[test]
fn full_lanes_retain_requests_until_work_can_be_accepted() {
    let mut consumer = Consumer::new();
    let runtime = BlockingRuntime::new(
        BlockingPool::new("bounded", 1, 1),
        BlockingPool::new("long", 1, 1),
        BlockingPool::new("process", 1, 1),
    );
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let running = runtime
        .spawn(None, move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let queued = runtime.spawn(None, || ()).unwrap();
    consumer.app.insert_resource(runtime);
    let operation = consumer.submit("After backpressure");
    consumer.app.update();
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Pending)
    );
    assert!(
        consumer
            .app
            .world()
            .get::<JobRequest<Rename>>(operation)
            .is_some()
    );
    release_tx.send(()).unwrap();
    consumer.result(operation);
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "After backpressure"
    );
    drop((running, queued));
}

#[derive(Clone)]
struct Held {
    started: mpsc::SyncSender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
    completed: mpsc::SyncSender<bool>,
}

impl Job for Held {
    type Output = String;

    fn run(self) -> Self::Output {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        self.completed
            .send(BlockingCancellation::canceled())
            .unwrap();
        "stale".to_string()
    }
}

struct HeldConsumer {
    app: App,
    owner: Entity,
    operation: Entity,
    release: mpsc::SyncSender<()>,
    completed: mpsc::Receiver<bool>,
}

impl HeldConsumer {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            JobPlugin::<Held>::default(),
        ));
        let owner = app.world_mut().spawn(Name::new("Original")).id();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (completed_tx, completed_rx) = mpsc::sync_channel(1);
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("held").unwrap(),
                ChildOf(owner),
                JobRequest(Held {
                    started: started_tx,
                    release: Arc::new(Mutex::new(release_rx)),
                    completed: completed_tx,
                }),
            ))
            .id();
        app.update();
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        Self {
            app,
            owner,
            operation,
            release: release_tx,
            completed: completed_rx,
        }
    }

    fn finish(&mut self) -> bool {
        self.release.send(()).unwrap();
        let canceled = self.completed.recv_timeout(Duration::from_secs(3)).unwrap();
        self.app.update();
        assert!(
            self.app
                .world()
                .get::<JobResult<Held>>(self.operation)
                .is_none()
        );
        assert!(
            self.app
                .world_mut()
                .resource_mut::<Messages<JobReady<Held>>>()
                .drain()
                .next()
                .is_none()
        );
        canceled
    }
}

#[test]
fn reparenting_cancels_the_original_job_and_discards_its_late_output() {
    let mut consumer = HeldConsumer::new();
    let replacement = consumer
        .app
        .world_mut()
        .spawn(Name::new("Replacement"))
        .id();
    consumer
        .app
        .world_mut()
        .entity_mut(consumer.operation)
        .insert(ChildOf(replacement));
    consumer.app.update();
    assert_eq!(
        consumer
            .app
            .world()
            .get::<OperationState>(consumer.operation),
        Some(&OperationState::Canceled)
    );
    assert!(consumer.finish());
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(replacement)
            .unwrap()
            .as_str(),
        "Replacement"
    );
}

#[test]
fn owner_removal_cancels_running_work() {
    let mut consumer = HeldConsumer::new();
    consumer
        .app
        .world_mut()
        .entity_mut(consumer.owner)
        .despawn();
    assert!(consumer.app.world().get_entity(consumer.operation).is_err());
    assert!(consumer.finish());
}

#[test]
fn target_removal_preserves_accepted_work_until_domain_commit() {
    let mut consumer = Consumer::new();
    let target = consumer.app.world_mut().spawn_empty().id();
    let operation = consumer.submit("Committed without the view");
    consumer
        .app
        .world_mut()
        .entity_mut(operation)
        .insert(OperationTarget::new(target));
    consumer.app.update();
    consumer.app.world_mut().entity_mut(target).despawn();
    consumer.result(operation);
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Committed without the view"
    );
}

#[test]
fn changing_the_owner_after_publication_discards_the_uncommitted_result() {
    let mut consumer = Consumer::new();
    let operation = consumer.submit("Stale");
    consumer.result(operation);
    let replacement = consumer
        .app
        .world_mut()
        .spawn(Name::new("Replacement"))
        .id();
    consumer
        .app
        .world_mut()
        .entity_mut(operation)
        .insert(ChildOf(replacement));
    consumer.app.update();
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Canceled)
    );
    assert!(
        consumer
            .app
            .world()
            .get::<JobResult<Rename>>(operation)
            .is_none()
    );
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(replacement)
            .unwrap()
            .as_str(),
        "Replacement"
    );
}

#[test]
fn canceling_a_published_result_refuses_a_late_domain_commit() {
    let mut consumer = Consumer::new();
    let operation = consumer.submit("Canceled");
    consumer.result(operation);
    consumer
        .app
        .world_mut()
        .entity_mut(operation)
        .insert(OperationState::Canceled);
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Original"
    );
    assert!(
        consumer
            .app
            .world()
            .get::<JobResult<Rename>>(operation)
            .is_none()
    );
}

#[test]
fn application_checks_captured_ownership_when_reparenting_happens_in_the_apply_frame() {
    let mut consumer = Consumer::new();
    let operation = consumer.submit("Stale");
    consumer.result(operation);
    let replacement = consumer
        .app
        .world_mut()
        .spawn(Name::new("Replacement"))
        .id();
    consumer.app.world_mut().write_message(Reparent {
        operation,
        parent: replacement,
    });
    consumer.commit(operation);
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(replacement)
            .unwrap()
            .as_str(),
        "Replacement"
    );
    assert_eq!(
        consumer
            .app
            .world()
            .get::<Name>(consumer.owner)
            .unwrap()
            .as_str(),
        "Original"
    );
    consumer.app.update();
    assert_eq!(
        consumer.app.world().get::<OperationState>(operation),
        Some(&OperationState::Canceled)
    );
}

#[test]
fn worker_panics_and_unavailable_lanes_publish_failures_for_domain_completion() {
    for unavailable in [false, true] {
        let mut consumer = Consumer::new();
        if unavailable {
            consumer.app.insert_resource(BlockingRuntime::new(
                BlockingPool::new("unavailable", 0, 1),
                BlockingPool::new("long", 0, 1),
                BlockingPool::new("process", 0, 1),
            ));
        }
        let operation = consumer.submit("panic");
        consumer.result(operation);
        let error = if unavailable {
            "blocking worker pool is unavailable"
        } else {
            "worker panic"
        };
        assert_eq!(
            consumer
                .app
                .world()
                .get::<JobResult<Rename>>(operation)
                .unwrap()
                .0,
            Err(error.to_string())
        );
        consumer.commit(operation);
        assert_eq!(
            consumer.app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            consumer
                .app
                .world()
                .get::<OperationError>(operation)
                .unwrap()
                .message(),
            error
        );
    }
}
