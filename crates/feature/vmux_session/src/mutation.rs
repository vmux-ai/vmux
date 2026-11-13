use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
pub use vmux_ecs::revision::{BaseRevision, Record, Revision};
use vmux_ecs::store::SchemaPlugin;
use vmux_ecs::{ArchivedAt, CreatedAt, Cwd, Description, LastActivatedAt, UnixMillis, Uuid};

use crate::{
    AgentConversationTitle, AgentId, Created, LocalTask, Session, SessionId, Stage, StageChangedAt,
    StageDefinition, StageId, TitlePending,
};

pub(crate) struct MutationPlugin;

impl Plugin for MutationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SchemaPlugin::<MutationStore>::default())
            .register_type::<Creation>()
            .add_message::<Request>()
            .add_message::<Committed>()
            .add_message::<Rejected>()
            .configure_sets(
                Update,
                (ReceiveSet, CommitSet)
                    .chain()
                    .in_set(crate::model::MutationSet),
            )
            .add_systems(
                Update,
                (ApplyDeferred, receive, ApplyDeferred)
                    .chain()
                    .in_set(CommitSet),
            );
    }
}

#[vmux_ecs::store(domain = vmux_ecs::store::WorkspaceStore, track;
    Cwd,
    Description,
    CreatedAt,
    ArchivedAt,
    LastActivatedAt,
    vmux_ecs::Order,
    Mutation,
    Record,
    Revision,
    BaseRevision,
    Uuid,
)]
struct MutationStore;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ReceiveSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CommitSet;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub enum Mutation {
    Create(Creation),
    Rename(String),
    Describe(String),
    Summarize {
        title: String,
        description: String,
    },
    ChangeWorkspace(std::path::PathBuf),
    BindAgent {
        agent: AgentId,
        cwd: std::path::PathBuf,
    },
    ChangeStage(StageId),
    Archive(Option<i64>),
}

#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
#[type_path = "vmux_session"]
pub struct Creation {
    pub name: String,
    pub description: String,
    pub cwd: std::path::PathBuf,
    pub agent: Option<AgentId>,
    pub stage: StageId,
    pub created_at: i64,
    pub activated_at: i64,
    pub local: bool,
}

#[derive(Message, Clone, Debug)]
pub struct Request {
    pub operation: String,
    pub session: SessionId,
    pub expected: Option<Revision>,
    pub mutation: Mutation,
    lease: Option<Entity>,
}

impl Request {
    pub fn local(session: SessionId, mutation: Mutation) -> Self {
        Self {
            operation: uuid::Uuid::new_v4().to_string(),
            session,
            expected: None,
            mutation,
            lease: None,
        }
    }

    pub(crate) fn leased(mut self, operation: Entity, uuid: String) -> Self {
        self.lease = Some(operation);
        self.operation = uuid;
        self
    }

    fn creation(&self) -> Option<&Creation> {
        match &self.mutation {
            Mutation::Create(created) => Some(created),
            _ => None,
        }
    }
}

impl Command for Request {
    type Out = ();

    fn apply(self, world: &mut World) {
        let result = 'commit: {
            if self.operation.trim().is_empty() {
                break 'commit Err(Rejection::InvalidOperation);
            }
            let mut entity = None;
            for (candidate, id) in world
                .query_filtered::<(Entity, &SessionId), With<Session>>()
                .iter(world)
            {
                if id != &self.session {
                    continue;
                }
                if entity.replace(candidate).is_some() {
                    break 'commit Err(Rejection::DuplicateSession);
                }
            }
            let mut committed = None;
            for (id, revision, mutation, owner) in world
                .query_filtered::<(&Uuid, &Revision, &Mutation, &ChildOf), With<Record>>()
                .iter(world)
            {
                if id.0 != self.operation {
                    continue;
                }
                if entity != Some(owner.parent())
                    || mutation != &self.mutation
                    || committed.is_some()
                {
                    break 'commit Err(Rejection::OperationConflict);
                }
                committed = Some(*revision);
            }
            if let Some(revision) = committed {
                break 'commit Ok(Committed {
                    operation: self.operation.clone(),
                    session: self.session.clone(),
                    revision,
                });
            }
            if let Some(operation) = self.lease {
                let mut leases =
                    bevy_ecs::system::SystemState::<crate::provider::ImportLeases>::new(world);
                if world.get::<SessionId>(operation) != Some(&self.session)
                    || !leases
                        .get(world)
                        .expect("import lease parameters")
                        .active(operation)
                {
                    break 'commit Err(Rejection::Canceled);
                }
            }
            let current = entity
                .and_then(|entity| world.get::<Revision>(entity))
                .copied()
                .unwrap_or_default();
            if let Some(expected) = self.expected
                && expected != current
            {
                break 'commit Err(Rejection::Stale {
                    expected,
                    actual: current,
                });
            }
            let Some(next) = current.next() else {
                break 'commit Err(Rejection::RevisionExhausted);
            };
            let stage = match self.creation() {
                Some(created) => Some(&created.stage),
                None => match &self.mutation {
                    Mutation::ChangeStage(stage) => Some(stage),
                    _ => None,
                },
            };
            if let Some(stage) = stage
                && !world
                    .query_filtered::<&StageId, With<StageDefinition>>()
                    .iter(world)
                    .any(|id| id == stage)
            {
                break 'commit Err(Rejection::MissingStage);
            }
            let entity = match (self.creation(), entity) {
                (Some(_), Some(_)) => break 'commit Err(Rejection::SessionExists),
                (Some(created), None) => {
                    let mut entity = world.spawn((
                        Session,
                        self.session.clone(),
                        Name::new(created.name.clone()),
                        Description(created.description.clone()),
                        Cwd(created.cwd.clone()),
                        CreatedAt(created.created_at),
                        LastActivatedAt(created.activated_at),
                        Stage(created.stage.clone()),
                        StageChangedAt(created.activated_at),
                    ));
                    if created.local {
                        entity.insert(LocalTask);
                    }
                    if let Some(agent) = created.agent.clone() {
                        entity.insert(agent);
                    }
                    entity.id()
                }
                (_, Some(entity)) => entity,
                (_, None) => break 'commit Err(Rejection::MissingSession),
            };
            'change: {
                match &self.mutation {
                    Mutation::Create(_) => {}
                    Mutation::Rename(name) => {
                        world
                            .entity_mut(entity)
                            .insert((
                                Name::new(name.clone()),
                                AgentConversationTitle(name.clone()),
                            ))
                            .remove::<TitlePending>();
                    }
                    Mutation::Describe(description) => {
                        world
                            .entity_mut(entity)
                            .insert(Description(description.clone()));
                    }
                    Mutation::Summarize { title, description } => {
                        world
                            .entity_mut(entity)
                            .insert((
                                Name::new(title.clone()),
                                AgentConversationTitle(title.clone()),
                                Description(description.clone()),
                            ))
                            .remove::<TitlePending>();
                    }
                    Mutation::ChangeWorkspace(cwd) => {
                        world.entity_mut(entity).insert(Cwd(cwd.clone()));
                    }
                    Mutation::BindAgent { agent, cwd } => {
                        world
                            .entity_mut(entity)
                            .insert((agent.clone(), Cwd(cwd.clone())));
                    }
                    Mutation::ChangeStage(stage) => {
                        if world
                            .get::<Stage>(entity)
                            .is_some_and(|current| current.0 == *stage)
                        {
                            break 'change;
                        }
                        let previous = world
                            .get::<StageChangedAt>(entity)
                            .map(|at| at.0)
                            .unwrap_or_default();
                        world.entity_mut(entity).insert((
                            Stage(stage.clone()),
                            StageChangedAt(UnixMillis::now().0.max(previous.saturating_add(1))),
                        ));
                    }
                    Mutation::Archive(Some(at)) => {
                        world.entity_mut(entity).insert(ArchivedAt((*at).max(1)));
                    }
                    Mutation::Archive(None) => {
                        world.entity_mut(entity).remove::<ArchivedAt>();
                    }
                }
            }
            world.entity_mut(entity).insert(next);
            let record = Record::bundle(
                entity,
                Uuid(self.operation.clone()),
                current,
                next,
                CreatedAt(UnixMillis::now().0),
            )
            .unwrap();
            world.spawn((record, self.mutation.clone()));
            if matches!(self.mutation, Mutation::Create(_)) {
                world.trigger(Created {
                    entity,
                    id: self.session.clone(),
                });
            }
            Ok(Committed {
                operation: self.operation.clone(),
                session: self.session.clone(),
                revision: next,
            })
        };
        match result {
            Ok(committed) => {
                world.write_message(committed);
            }
            Err(reason) => {
                world.write_message(Rejected {
                    operation: self.operation,
                    session: self.session,
                    reason,
                });
            }
        }
    }
}

#[derive(Message, Clone, Debug)]
pub struct Committed {
    pub operation: String,
    pub session: SessionId,
    pub revision: Revision,
}

#[derive(Message, Clone, Debug)]
pub struct Rejected {
    pub operation: String,
    pub session: SessionId,
    pub reason: Rejection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    Canceled,
    InvalidOperation,
    OperationConflict,
    MissingSession,
    DuplicateSession,
    SessionExists,
    MissingStage,
    Stale {
        expected: Revision,
        actual: Revision,
    },
    RevisionExhausted,
}

fn receive(mut requests: MessageReader<Request>, mut commands: Commands) {
    for request in requests.read() {
        commands.queue(request.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_revision_refuses_the_mutation_without_changing_state_or_adding_a_record() {
        let mut app = App::new();
        app.add_plugins(MutationPlugin);
        let id = SessionId("exhausted".into());
        let entity = app
            .world_mut()
            .spawn((
                Session,
                id.clone(),
                Name::new("Original"),
                Revision(u64::MAX),
            ))
            .id();
        app.world_mut()
            .write_message(Request::local(id, Mutation::Rename("Changed".into())));
        app.update();
        assert_eq!(
            app.world().get::<Name>(entity).unwrap().as_str(),
            "Original"
        );
        assert_eq!(
            app.world().get::<Revision>(entity),
            Some(&Revision(u64::MAX))
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Record>>()
                .iter(app.world())
                .count(),
            0
        );
        let rejected = app
            .world_mut()
            .resource_mut::<Messages<Rejected>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(rejected.reason, Rejection::RevisionExhausted);
    }

    #[test]
    fn committed_records_replay_creation_and_later_mutations_through_the_same_path() {
        let mut source = App::new();
        source.add_plugins(MutationPlugin);
        source
            .world_mut()
            .spawn((StageDefinition, StageId("active".into())));
        let id = SessionId("replayed".into());
        let changes = [
            Mutation::Create(Creation {
                name: "Initial".into(),
                description: String::new(),
                cwd: std::path::PathBuf::from("/tmp/project"),
                agent: None,
                stage: StageId("active".into()),
                created_at: 100,
                activated_at: 100,
                local: true,
            }),
            Mutation::Rename("Final".into()),
            Mutation::Describe("Committed summary".into()),
            Mutation::Archive(Some(200)),
        ];
        for (base, mutation) in changes.into_iter().enumerate() {
            source.world_mut().write_message(Request {
                lease: None,
                operation: format!("operation-{base}"),
                session: id.clone(),
                expected: Some(Revision(base as u64)),
                mutation,
            });
        }
        source.update();
        let mut replay = Vec::new();
        for (operation, base, mutation) in source
            .world_mut()
            .query_filtered::<(&Uuid, &BaseRevision, &Mutation), With<Record>>()
            .iter(source.world())
        {
            replay.push(Request {
                lease: None,
                operation: operation.0.clone(),
                session: id.clone(),
                expected: Some(Revision(base.0)),
                mutation: mutation.clone(),
            });
        }
        replay.sort_by_key(|request| request.expected.unwrap().0);
        assert_eq!(replay.len(), 4);
        let mut restored = App::new();
        restored.add_plugins(MutationPlugin);
        restored
            .world_mut()
            .spawn((StageDefinition, StageId("active".into())));
        for request in replay {
            restored.world_mut().write_message(request);
        }
        restored.update();
        let (name, description, cwd, archived, revision) = restored
            .world_mut()
            .query_filtered::<(&Name, &Description, &Cwd, &ArchivedAt, &Revision), With<Session>>()
            .single(restored.world())
            .unwrap();
        assert_eq!(name.as_str(), "Final");
        assert_eq!(description.0, "Committed summary");
        assert_eq!(cwd.0, std::path::PathBuf::from("/tmp/project"));
        assert_eq!(archived.0, 200);
        assert_eq!(*revision, Revision(4));
    }

    #[test]
    fn repeated_and_stale_mutations_cannot_overwrite_a_newer_commit() {
        let mut app = App::new();
        app.add_plugins(MutationPlugin);
        let id = SessionId("task".into());
        let entity = app
            .world_mut()
            .spawn((Session, id.clone(), Name::new("Original")))
            .id();
        let request = Request {
            lease: None,
            operation: "rename".into(),
            session: id.clone(),
            expected: Some(Revision(0)),
            mutation: Mutation::Rename("First".into()),
        };
        app.world_mut().write_message(request.clone());
        app.update();
        app.world_mut().write_message(Request {
            lease: None,
            operation: "describe".into(),
            session: id.clone(),
            expected: Some(Revision(1)),
            mutation: Mutation::Describe("New".into()),
        });
        app.update();
        app.world_mut().write_message(request);
        app.world_mut().write_message(Request {
            lease: None,
            operation: "stale".into(),
            session: id,
            expected: Some(Revision(0)),
            mutation: Mutation::Rename("Stale".into()),
        });
        app.update();
        assert_eq!(app.world().get::<Name>(entity).unwrap().as_str(), "First");
        assert_eq!(app.world().get::<Description>(entity).unwrap().0, "New");
        assert_eq!(app.world().get::<Revision>(entity), Some(&Revision(2)));
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Record>>()
                .iter(app.world())
                .count(),
            2
        );
        let rejected = app
            .world_mut()
            .resource_mut::<Messages<Rejected>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            rejected[0].reason,
            Rejection::Stale {
                expected: Revision(0),
                actual: Revision(2)
            }
        );
    }

    #[test]
    fn ambiguous_identity_and_invalid_stage_leave_no_partial_commit() {
        let mut app = App::new();
        app.add_plugins(MutationPlugin);
        let id = SessionId("duplicate".into());
        let first = app
            .world_mut()
            .spawn((Session, id.clone(), Name::new("First")))
            .id();
        let second = app
            .world_mut()
            .spawn((Session, id.clone(), Name::new("Second")))
            .id();
        app.world_mut()
            .write_message(Request::local(id.clone(), Mutation::Rename("Wrong".into())));
        app.update();
        assert_eq!(app.world().get::<Name>(first).unwrap().as_str(), "First");
        assert_eq!(app.world().get::<Name>(second).unwrap().as_str(), "Second");
        app.world_mut().entity_mut(second).despawn();
        app.world_mut().write_message(Request::local(
            id,
            Mutation::ChangeStage(StageId("missing".into())),
        ));
        app.update();
        assert!(app.world().get::<Revision>(first).is_none());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Record>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
