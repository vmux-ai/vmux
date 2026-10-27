use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::Id;
use vmux_api::project::{Project, ProjectRef};
use vmux_api::session::Session;
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_ecs::{
    Actor, ArchivedAt, CommittedOperation, CreatedAt, Cwd, Description, LastActivatedAt, Operation,
    OperationSequence, Revision, UnixMillis,
};

use crate::taxonomy::TaxonomyMutation;
use crate::{
    AgentConversationTitle, AgentRef, TitlePending, WorkflowState, WorkflowStateChangedAt,
    WorkflowStateRef,
};

pub(crate) struct MutationPlugin;

impl Plugin for MutationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::model::SessionIdentityPlugin>() {
            app.add_plugins(crate::model::SessionIdentityPlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::ReplicationPlugin>() {
            app.add_plugins(vmux_ecs::ReplicationPlugin);
        }
        app.register_persisted::<SessionMutation>()
            .register_type::<CreatedMutation>()
            .register_type::<TaxonomyMutation>()
            .add_message::<SessionMutationRequest>()
            .add_message::<SessionMutationCommitted>()
            .add_message::<SessionMutationRejected>()
            .configure_sets(
                Update,
                (SessionMutationReceiveSet, SessionMutationCommitSet)
                    .chain()
                    .in_set(crate::model::MutationSet),
            )
            .add_systems(
                Update,
                (commit, bevy_ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(SessionMutationCommitSet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionMutationReceiveSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionMutationCommitSet;

#[derive(Component, Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub enum SessionMutation {
    Created(CreatedMutation),
    Rename(String),
    Describe(String),
    ChangeWorkflowState(Id<WorkflowState>),
    Archive(Option<i64>),
    ChangeProject(Option<Id<Project>>),
    ChangeTaxonomy(TaxonomyMutation),
}

#[derive(Clone, Debug, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_session"]
pub struct CreatedMutation {
    pub name: String,
    pub description: String,
    pub cwd: PathBuf,
    pub agent: Option<AgentRef>,
    pub workflow_state: Id<WorkflowState>,
    pub created_at: i64,
    pub activated_at: i64,
}

#[derive(Message, Clone, Debug)]
pub struct SessionMutationRequest {
    operation: Id<Operation>,
    actor: Id<Actor>,
    session: Id<Session>,
    expected: Option<Revision>,
    mutation: SessionMutation,
}

impl SessionMutationRequest {
    pub fn local(session: Id<Session>, mutation: SessionMutation) -> Self {
        Self {
            operation: Id::new(uuid::Uuid::new_v4().to_string()),
            actor: Self::local_actor(),
            session,
            expected: None,
            mutation,
        }
    }

    pub fn replicated(
        operation: Id<Operation>,
        actor: Id<Actor>,
        session: Id<Session>,
        expected: Revision,
        mutation: SessionMutation,
    ) -> Self {
        Self {
            operation,
            actor,
            session,
            expected: Some(expected),
            mutation,
        }
    }

    pub(crate) fn local_actor() -> Id<Actor> {
        Id::new("local")
    }
}

#[derive(Message, Clone, Debug)]
pub struct SessionMutationCommitted {
    pub operation: Id<Operation>,
    pub session: Id<Session>,
    pub revision: Revision,
}

#[derive(Message, Clone, Debug)]
pub struct SessionMutationRejected {
    pub operation: Id<Operation>,
    pub session: Id<Session>,
    pub reason: SessionMutationRejection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionMutationRejection {
    MissingSession,
    SessionAlreadyExists,
    Stale {
        expected: Revision,
        actual: Revision,
    },
    MissingWorkflowState(Id<WorkflowState>),
    RevisionExhausted,
    InvalidOperation(String),
}

#[derive(SystemParam)]
struct SessionMutations<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Entity,
            &'static Id<Session>,
            Option<&'static Revision>,
            Option<&'static Children>,
        ),
        With<Session>,
    >,
    session_index: Res<'w, vmux_ecs::IdentityIndex<Session>>,
    workflow_states: Option<Res<'w, vmux_ecs::IdentityIndex<WorkflowState>>>,
    operations: Query<
        'w,
        's,
        (
            Entity,
            &'static Id<Operation>,
            &'static OperationSequence,
            &'static ChildOf,
        ),
        With<CommittedOperation>,
    >,
    label_assignments:
        Query<'w, 's, (Entity, &'static crate::LabelRef), With<crate::LabelAssignment>>,
    commands: Commands<'w, 's>,
}

impl SessionMutations<'_, '_> {
    fn commit(
        &mut self,
        request: &SessionMutationRequest,
        revisions: &mut HashMap<Entity, Revision>,
        sequences: &mut HashMap<Entity, u64>,
        operations: &mut HashMap<(Entity, Id<Operation>), Entity>,
        sessions: &mut HashMap<Id<Session>, Entity>,
    ) -> Result<SessionMutationCommitted, SessionMutationRejection> {
        let existing = sessions
            .get(&request.session)
            .copied()
            .or_else(|| self.session_index.entity(&request.session).ok().flatten());
        let created = existing.is_none();
        let session = match (existing, &request.mutation) {
            (Some(session), SessionMutation::Created(_)) => {
                if operations.contains_key(&(session, request.operation.clone())) {
                    session
                } else {
                    return Err(SessionMutationRejection::SessionAlreadyExists);
                }
            }
            (Some(session), _) => session,
            (None, SessionMutation::Created(created)) => {
                let session = self.spawn(created, request.session.clone());
                sessions.insert(request.session.clone(), session);
                revisions.insert(session, Revision::new(0));
                session
            }
            (None, _) => return Err(SessionMutationRejection::MissingSession),
        };
        if operations.contains_key(&(session, request.operation.clone())) {
            let revision = revisions
                .get(&session)
                .copied()
                .or_else(|| {
                    self.sessions
                        .get(session)
                        .ok()
                        .and_then(|(_, _, revision, _)| revision.copied())
                })
                .unwrap_or_else(|| Revision::new(0));
            return Ok(SessionMutationCommitted {
                operation: request.operation.clone(),
                session: request.session.clone(),
                revision,
            });
        }
        let (persisted_revision, children) = match self.sessions.get(session) {
            Ok((_, _, revision, children)) => (revision.copied(), children),
            Err(_) if created => (None, None),
            Err(_) => return Err(SessionMutationRejection::MissingSession),
        };
        let current = revisions
            .entry(session)
            .or_insert(persisted_revision.unwrap_or_else(|| Revision::new(0)));
        if let Some(expected) = request.expected
            && expected != *current
        {
            return Err(SessionMutationRejection::Stale {
                expected,
                actual: *current,
            });
        }
        request.mutation.validate(self.workflow_states.as_deref())?;
        let next = current
            .next()
            .ok_or(SessionMutationRejection::RevisionExhausted)?;
        let sequence = sequences.entry(session).or_default();
        *sequence = sequence.saturating_add(1);
        let children = children
            .into_iter()
            .flat_map(|children| children.iter())
            .collect::<Vec<_>>();
        self.apply(&request.mutation, session, &children);
        self.commands
            .entity(session)
            .insert((vmux_ecs::ReplicationRoot, next));
        let spec = vmux_ecs::CommittedOperationSpec::new(
            request.operation.clone(),
            request.actor.clone(),
            request.session.clone(),
            *current,
            next,
            *sequence,
            UnixMillis::now(),
        )
        .map_err(SessionMutationRejection::InvalidOperation)?;
        let bundle = CommittedOperation::bundle(session, spec);
        let operation = self.commands.spawn((bundle, request.mutation.clone())).id();
        operations.insert((session, request.operation.clone()), operation);
        *current = next;
        Ok(SessionMutationCommitted {
            operation: request.operation.clone(),
            session: request.session.clone(),
            revision: next,
        })
    }

    fn spawn(&mut self, created: &CreatedMutation, id: Id<Session>) -> Entity {
        let mut session = self.commands.spawn((
            Save,
            Session,
            id,
            Name::new(created.name.clone()),
            Description(created.description.clone()),
            Cwd(created.cwd.clone()),
            CreatedAt::new(created.created_at),
            LastActivatedAt::new(created.activated_at),
            WorkflowStateRef::new(created.workflow_state.clone()),
            WorkflowStateChangedAt::new(created.activated_at),
        ));
        if let Some(agent) = created.agent.clone() {
            session.insert(agent);
        }
        session.id()
    }

    fn apply(&mut self, mutation: &SessionMutation, session: Entity, children: &[Entity]) {
        match mutation {
            SessionMutation::Created(_) => {}
            SessionMutation::Rename(name) => {
                self.commands.entity(session).insert((
                    Name::new(name.clone()),
                    AgentConversationTitle(name.clone()),
                ));
                self.commands.entity(session).remove::<TitlePending>();
            }
            SessionMutation::Describe(description) => {
                self.commands
                    .entity(session)
                    .insert(Description(description.clone()));
            }
            SessionMutation::ChangeWorkflowState(workflow_state) => {
                self.commands.entity(session).insert((
                    WorkflowStateRef::new(workflow_state.clone()),
                    WorkflowStateChangedAt::new(UnixMillis::now().0),
                ));
            }
            SessionMutation::Archive(archived_at) => match archived_at {
                Some(archived_at) => {
                    self.commands
                        .entity(session)
                        .insert(ArchivedAt::new((*archived_at).max(1)));
                }
                None => {
                    self.commands.entity(session).remove::<ArchivedAt>();
                }
            },
            SessionMutation::ChangeProject(project) => match project {
                Some(project) => {
                    self.commands
                        .entity(session)
                        .insert(ProjectRef::new(project.clone()));
                }
                None => {
                    self.commands.entity(session).remove::<ProjectRef>();
                }
            },
            SessionMutation::ChangeTaxonomy(taxonomy) => {
                self.apply_taxonomy(taxonomy, session, children);
            }
        }
    }

    fn apply_taxonomy(
        &mut self,
        taxonomy: &TaxonomyMutation,
        session: Entity,
        children: &[Entity],
    ) {
        let mut entity = self.commands.entity(session);
        match &taxonomy.cycle {
            Some(cycle) => {
                entity.insert(crate::CycleRef::new(cycle.clone()));
            }
            None => {
                entity.remove::<(crate::CycleRef, crate::CycleTarget)>();
            }
        }
        match &taxonomy.assignee {
            Some(assignee) => {
                entity.insert(crate::AssigneeRef::new(assignee.clone()));
            }
            None => {
                entity.remove::<(crate::AssigneeRef, crate::AssigneeTarget)>();
            }
        }
        match taxonomy.priority {
            Some(priority) => {
                entity.insert(priority);
            }
            None => {
                entity.remove::<crate::Priority>();
            }
        }
        match taxonomy.estimate {
            Some(estimate) => {
                entity.insert(estimate);
            }
            None => {
                entity.remove::<crate::Estimate>();
            }
        }
        match taxonomy.due_at {
            Some(due_at) => {
                entity.insert(due_at);
            }
            None => {
                entity.remove::<crate::DueAt>();
            }
        }

        let requested = taxonomy.labels.iter().cloned().collect::<HashSet<_>>();
        let mut existing = HashMap::<Id<crate::Label>, Entity>::new();
        for child in children {
            if let Ok((assignment, label)) = self.label_assignments.get(*child) {
                existing.insert(label.id().clone(), assignment);
            }
        }
        for (label, assignment) in &existing {
            if !requested.contains(label) {
                self.commands.entity(*assignment).despawn();
            }
        }
        for label in &taxonomy.labels {
            if !existing.contains_key(label) {
                self.commands.spawn((
                    crate::LabelAssignment,
                    crate::LabelRef::new(label.clone()),
                    ChildOf(session),
                ));
            }
        }
    }
}

impl SessionMutation {
    fn validate(
        &self,
        workflow_states: Option<&vmux_ecs::IdentityIndex<WorkflowState>>,
    ) -> Result<(), SessionMutationRejection> {
        if let Self::ChangeWorkflowState(id) = self
            && workflow_states
                .and_then(|workflow_states| workflow_states.entity(id).ok().flatten())
                .is_none()
        {
            return Err(SessionMutationRejection::MissingWorkflowState(id.clone()));
        }
        Ok(())
    }
}

fn commit(
    mut requests: MessageReader<SessionMutationRequest>,
    mut committed: MessageWriter<SessionMutationCommitted>,
    mut rejected: MessageWriter<SessionMutationRejected>,
    mut mutations: SessionMutations,
) {
    if requests.is_empty() {
        return;
    }
    let mut revisions = HashMap::<Entity, Revision>::new();
    let mut sequences = HashMap::<Entity, u64>::new();
    let mut operations = HashMap::<(Entity, Id<Operation>), Entity>::new();
    let mut sessions = mutations
        .sessions
        .iter()
        .map(|(entity, id, _, _)| (id.clone(), entity))
        .collect::<HashMap<_, _>>();
    for (entity, operation, sequence, owner) in &mutations.operations {
        operations.insert((owner.parent(), operation.clone()), entity);
        let current = sequences.entry(owner.parent()).or_default();
        *current = (*current).max(sequence.0);
    }
    for request in requests.read() {
        match mutations.commit(
            request,
            &mut revisions,
            &mut sequences,
            &mut operations,
            &mut sessions,
        ) {
            Ok(result) => {
                committed.write(result);
            }
            Err(reason) => {
                rejected.write(SessionMutationRejected {
                    operation: request.operation.clone(),
                    session: request.session.clone(),
                    reason,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateRequest, SessionDomainPlugin};

    #[test]
    fn created_operation_materializes_a_missing_session_root() {
        let mut app = App::new();
        app.add_plugins(SessionDomainPlugin);
        let session_id = Id::<Session>::new("session-remote");
        app.world_mut()
            .resource_mut::<Messages<SessionMutationRequest>>()
            .write(SessionMutationRequest::replicated(
                Id::new("operation-create"),
                Id::new("member-1"),
                session_id.clone(),
                Revision::new(0),
                SessionMutation::Created(CreatedMutation {
                    name: "Remote".to_string(),
                    description: "Synced".to_string(),
                    cwd: PathBuf::from("/tmp/remote"),
                    agent: Some(AgentRef::new("codex")),
                    workflow_state: Id::new("in_progress"),
                    created_at: 1,
                    activated_at: 2,
                }),
            ));

        app.update();

        let session = app
            .world()
            .resource::<vmux_ecs::IdentityIndex<Session>>()
            .entity(&session_id)
            .unwrap()
            .unwrap();
        assert_eq!(app.world().get::<Name>(session).unwrap().as_str(), "Remote");
        assert_eq!(
            app.world().get::<Revision>(session),
            Some(&Revision::new(1))
        );
        assert_eq!(
            app.world().get::<WorkflowStateRef>(session).unwrap().id(),
            &Id::new("in_progress")
        );
    }

    #[test]
    fn replicated_mutations_are_idempotent_and_reject_stale_revisions() {
        let mut app = App::new();
        app.add_plugins(SessionDomainPlugin);
        let create = CreateRequest::new("one", "", PathBuf::from("/tmp"), None);
        let session_id = create.id().clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(create);
        app.update();

        let operation = Id::<Operation>::new("operation-1");
        let request = SessionMutationRequest::replicated(
            operation.clone(),
            Id::new("member-1"),
            session_id.clone(),
            Revision::new(1),
            SessionMutation::Rename("two".to_string()),
        );
        app.world_mut()
            .resource_mut::<Messages<SessionMutationRequest>>()
            .write(request.clone());
        app.world_mut()
            .resource_mut::<Messages<SessionMutationRequest>>()
            .write(request);
        app.update();

        let session = app
            .world()
            .resource::<vmux_ecs::IdentityIndex<Session>>()
            .entity(&session_id)
            .unwrap()
            .unwrap();
        assert_eq!(app.world().get::<Name>(session).unwrap().as_str(), "two");
        assert_eq!(
            app.world().get::<Revision>(session),
            Some(&Revision::new(2))
        );
        let committed = app
            .world_mut()
            .query_filtered::<&Id<Operation>, With<CommittedOperation>>()
            .iter(app.world())
            .filter(|id| **id == operation)
            .count();
        assert_eq!(committed, 1);

        app.world_mut()
            .resource_mut::<Messages<SessionMutationRequest>>()
            .write(SessionMutationRequest::replicated(
                Id::new("operation-2"),
                Id::new("member-1"),
                session_id,
                Revision::new(1),
                SessionMutation::Rename("stale".to_string()),
            ));
        app.update();

        assert_eq!(app.world().get::<Name>(session).unwrap().as_str(), "two");
    }
}
