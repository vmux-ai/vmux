use std::collections::BTreeSet;
use std::path::Path;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemState;
use vmux_ecs::{Operation, OperationState};
use vmux_project::{
    Error, LoadCompleted, LoadRequest, Mutation, ProjectId, ProjectSaved, Registrations, Root,
    Snapshot, WriteCompleted, WriteRequest, WriteResult, WriteTaskRequest,
};

struct Consumer(App);

impl Consumer {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins(vmux_app::VmuxPlugin::builder().project(true).build());
        Self(app)
    }

    fn root(&mut self, path: &str) -> Entity {
        let owner = self.0.world_mut().spawn(Root(path.into())).id();
        self.0.update();
        let request = self
            .0
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .find(|request| request.ticket.owner == owner)
            .unwrap();
        self.0.world_mut().write_message(LoadCompleted {
            ticket: request.ticket,
            result: Ok(Snapshot::default()),
        });
        self.0.update();
        owner
    }

    fn snapshot(&mut self, path: &str) -> Snapshot {
        let mut state = SystemState::<Registrations>::new(self.0.world_mut());
        state.get(self.0.world()).unwrap().snapshot(Path::new(path))
    }

    fn submit(&mut self, root: &str, mutation: Mutation) -> Entity {
        let operation = Operation::bundle("consumer-project").unwrap();
        let entity = self
            .0
            .world_mut()
            .spawn((
                operation,
                WriteRequest {
                    root: root.into(),
                    mutation,
                },
            ))
            .id();
        self.0.update();
        entity
    }

    fn save(&mut self, root: &str, name: &str, revision: u64) -> Entity {
        self.submit(
            root,
            Mutation::Save {
                expected: revision,
                project: ProjectSaved {
                    root: root.into(),
                    id: ProjectId("shared-id".into()),
                    directory: Path::new(root).join("checkout"),
                    name: name.into(),
                    repository: None,
                    revision: 0,
                },
            },
        )
    }

    fn requests(&mut self) -> Vec<WriteTaskRequest> {
        self.0
            .world_mut()
            .resource_mut::<Messages<WriteTaskRequest>>()
            .drain()
            .collect()
    }

    fn complete(&mut self, request: WriteTaskRequest, result: Result<BTreeSet<String>, Error>) {
        self.0.world_mut().write_message(WriteResult {
            ticket: request.ticket,
            result,
        });
        self.0.update();
    }
}

#[test]
fn duplicate_storage_roots_refuse_writes_before_dispatching_io() {
    let mut consumer = Consumer::new();
    consumer.root("/same");
    consumer.0.world_mut().spawn(Root("/same".into()));
    consumer.0.update();
    let operation = consumer.save("/same", "Conflicting owner", 0);
    assert!(consumer.requests().is_empty());
    assert!(consumer.snapshot("/same").projects.is_empty());
    assert_eq!(
        consumer.0.world().get::<OperationState>(operation),
        Some(&OperationState::Failed)
    );
    let results: Vec<_> = consumer
        .0
        .world_mut()
        .resource_mut::<Messages<WriteCompleted>>()
        .drain()
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].result, Err(Error::Duplicate));
}

#[test]
fn failed_restore_refuses_mutations_and_finishes_the_operation() {
    let mut consumer = Consumer::new();
    let owner = consumer.0.world_mut().spawn(Root("/broken".into())).id();
    consumer.0.update();
    let load = consumer
        .0
        .world_mut()
        .resource_mut::<Messages<LoadRequest>>()
        .drain()
        .find(|load| load.ticket.owner == owner)
        .unwrap();
    consumer.0.world_mut().write_message(LoadCompleted {
        ticket: load.ticket,
        result: Err(Error::Unavailable("corrupt store".into())),
    });
    consumer.0.update();
    let operation = consumer.save("/broken", "Rejected", 0);
    assert!(consumer.requests().is_empty());
    assert!(consumer.snapshot("/broken").projects.is_empty());
    assert_eq!(
        consumer.0.world().get::<OperationState>(operation),
        Some(&OperationState::Failed)
    );
    let results: Vec<_> = consumer
        .0
        .world_mut()
        .resource_mut::<Messages<WriteCompleted>>()
        .drain()
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].result,
        Err(Error::Unavailable("corrupt store".into()))
    );
}

#[test]
fn independent_roots_commit_only_their_own_successful_save() {
    let mut consumer = Consumer::new();
    let first = consumer.root("/first");
    let second = consumer.root("/second");
    consumer.save("/first", "First", 0);
    consumer.save("/second", "Second", 0);
    let requests = consumer.requests();
    assert_eq!(requests.len(), 2);
    assert!(consumer.snapshot("/first").projects.is_empty());
    assert!(consumer.snapshot("/second").projects.is_empty());
    for request in requests {
        if request.ticket.owner == first {
            consumer.complete(request, Err(Error::Unavailable("write failed".into())));
        } else {
            assert_eq!(request.ticket.owner, second);
            consumer.complete(request, Ok(BTreeSet::new()));
        }
    }
    assert!(consumer.snapshot("/first").projects.is_empty());
    assert_eq!(
        consumer.snapshot("/second").projects["shared-id"].name,
        "Second"
    );
}

#[test]
fn an_accepted_save_survives_requester_removal_and_rejects_stale_edits() {
    let mut consumer = Consumer::new();
    consumer.root("/projects");
    let requester = consumer.save("/projects", "Original", 0);
    let request = consumer.requests().pop().unwrap();
    consumer.0.world_mut().despawn(requester);
    consumer.complete(request, Ok(BTreeSet::new()));
    assert_eq!(
        consumer.snapshot("/projects").projects["shared-id"].revision,
        1
    );
    let rejected = consumer.save("/projects", "Stale", 0);
    assert!(consumer.requests().is_empty());
    assert_eq!(
        consumer.snapshot("/projects").projects["shared-id"].name,
        "Original"
    );
    assert_eq!(
        consumer.0.world().get::<OperationState>(rejected),
        Some(&OperationState::Failed)
    );
}

#[test]
fn archive_changes_commit_after_success_and_preserve_state_on_failure() {
    let mut consumer = Consumer::new();
    consumer.root("/projects");
    consumer.submit(
        "/projects",
        Mutation::Archive {
            url: "discovered-project".into(),
            archived: true,
        },
    );
    let request = consumer.requests().pop().unwrap();
    assert!(consumer.snapshot("/projects").archived.is_empty());
    consumer.complete(request, Ok(BTreeSet::new()));
    consumer.submit(
        "/projects",
        Mutation::Archive {
            url: "discovered-project".into(),
            archived: false,
        },
    );
    let request = consumer.requests().pop().unwrap();
    consumer.complete(request, Err(Error::Unavailable("write failed".into())));
    assert_eq!(
        consumer.snapshot("/projects").archived,
        BTreeSet::from(["discovered-project".into()])
    );
}

#[test]
fn replacing_a_root_cancels_accepted_work_and_ignores_its_late_result() {
    let mut consumer = Consumer::new();
    let owner = consumer.root("/old");
    let operation = consumer.save("/old", "Stale", 0);
    let request = consumer.requests().pop().unwrap();
    consumer
        .0
        .world_mut()
        .entity_mut(owner)
        .insert(Root("/new".into()));
    consumer.0.update();
    let load = consumer
        .0
        .world_mut()
        .resource_mut::<Messages<LoadRequest>>()
        .drain()
        .next()
        .unwrap();
    consumer.0.world_mut().write_message(LoadCompleted {
        ticket: load.ticket,
        result: Ok(Snapshot::default()),
    });
    consumer.complete(request, Ok(BTreeSet::new()));
    assert!(consumer.snapshot("/new").projects.is_empty());
    assert_eq!(
        consumer.0.world().get::<OperationState>(operation),
        Some(&OperationState::Failed)
    );
    let results: Vec<_> = consumer
        .0
        .world_mut()
        .resource_mut::<Messages<WriteCompleted>>()
        .drain()
        .collect();
    assert_eq!(results.len(), 1);
    assert!(results[0].result.is_err());
}
