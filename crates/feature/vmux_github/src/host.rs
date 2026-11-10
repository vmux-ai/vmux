use std::collections::HashSet;
use std::marker::PhantomData;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use moonshine_save::prelude::Save;
use vmux_api::Id;
use vmux_auth::{CanAccess, Connection, ConnectionState, ProviderKey};
use vmux_connector::{
    ExternalHost, ExternalId, ExternalLink, ExternalLinkIndex, ExternalNamespace, ExternalScope,
    ExternalScopeRef,
};
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{
    BlockingCancellation, BlockingRuntime, BlockingTask, Operation, OperationError, OperationState,
    Revision,
};

use crate::model::{GithubCoordinates, GithubIssue, GithubPullRequest, GithubRepository};

mod association;
mod request;

pub struct GithubHostPlugin;

impl Plugin for GithubHostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockingRuntime>()
            .init_resource::<ExternalLinkIndex>()
            .init_resource::<GithubScopes>()
            .add_message::<vmux_editor::ProjectIndexRefresh>()
            .register_stored::<GithubRepository>()
            .register_stored::<GithubIssue>()
            .register_stored::<GithubPullRequest>()
            .register_stored::<GithubCoordinates>()
            .configure_sets(
                Update,
                (
                    GithubSet::Request,
                    GithubSet::Poll.after(GithubSet::Request),
                    GithubSet::Complete.after(GithubSet::Poll),
                ),
            )
            .add_plugins((request::RequestPlugin, association::AssociationPlugin));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum GithubSet {
    Request,
    Poll,
    Complete,
}

#[derive(Component)]
pub(super) struct GithubTask<T: Send + Sync + 'static> {
    pub(super) target: GithubTaskTarget,
    task: BlockingTask<Result<T, String>>,
}

#[derive(Message)]
pub(super) struct GithubCompletion<T: Send + Sync + 'static> {
    pub(super) operation: Entity,
    pub(super) target: Entity,
    pub(super) binding: GithubTaskTarget,
    pub(super) output: T,
}

#[derive(Message)]
pub(super) struct GithubFailure<T: Send + Sync + 'static> {
    pub(super) operation: Entity,
    pub(super) target: Entity,
    pub(super) binding: GithubTaskTarget,
    pub(super) error: String,
    marker: PhantomData<fn() -> T>,
}

#[derive(Clone)]
pub(super) enum GithubTaskTarget {
    BoundConnection {
        connection: Entity,
    },
    Requester {
        requester: Entity,
        generation: u64,
        kind: GithubRequesterTaskKind,
        id: Id<Connection>,
    },
    BoundEntity {
        entity: Entity,
    },
    BoundRevision {
        entity: Entity,
        revision: Revision,
        external_id: String,
        scope: Id<ExternalScope>,
    },
}

pub(super) type GithubConnections<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionState,
        &'static CanAccess,
    ),
    With<Connection>,
>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GithubRequesterTaskKind {
    RepositorySearch,
    IssueSearch,
    RepositoryClone,
}

impl GithubTaskTarget {
    fn entity(&self) -> Entity {
        match self {
            Self::BoundConnection { connection, .. } => *connection,
            Self::Requester { requester, .. } => *requester,
            Self::BoundEntity { entity, .. } | Self::BoundRevision { entity, .. } => *entity,
        }
    }

    fn destination(
        &self,
        exists: bool,
        identity: Option<(&Revision, &ExternalId, &ExternalScopeRef)>,
    ) -> Option<Entity> {
        match self {
            Self::BoundConnection { connection, .. } => exists.then_some(*connection),
            Self::Requester { requester, .. } => exists.then_some(*requester),
            Self::BoundEntity { entity, .. } => exists.then_some(*entity),
            Self::BoundRevision {
                entity,
                revision,
                external_id,
                scope,
                ..
            } => identity
                .is_some_and(|(held_revision, held_id, held_scope)| {
                    held_revision == revision && held_id.0 == *external_id && held_scope.0 == *scope
                })
                .then_some(*entity),
        }
    }

    pub(super) fn requester(
        &self,
        kind: GithubRequesterTaskKind,
    ) -> Option<(Entity, u64, &Id<Connection>)> {
        let Self::Requester {
            requester,
            generation,
            kind: held_kind,
            id,
            ..
        } = self
        else {
            return None;
        };
        (*held_kind == kind).then_some((*requester, *generation, id))
    }

    pub(super) fn revision(&self) -> Option<(Entity, Revision)> {
        match self {
            Self::BoundRevision {
                entity, revision, ..
            } => Some((*entity, *revision)),
            _ => None,
        }
    }

    pub(super) fn expected_external_id(&self) -> Option<&str> {
        match self {
            Self::BoundRevision { external_id, .. } => Some(external_id),
            _ => None,
        }
    }

    pub(super) fn expected_scope(&self) -> Option<&Id<ExternalScope>> {
        match self {
            Self::BoundRevision { scope, .. } => Some(scope),
            _ => None,
        }
    }
}

impl<T: Send + Sync + 'static> GithubTask<T> {
    pub(super) fn start(
        runtime: &BlockingRuntime,
        target: GithubTaskTarget,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
        run: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Option<Self> {
        Some(Self {
            target,
            task: runtime.spawn_long(wake, run)?,
        })
    }

    pub(super) fn start_cancelable(
        runtime: &BlockingRuntime,
        target: GithubTaskTarget,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
        run: impl FnOnce(BlockingCancellation) -> Result<T, String> + Send + 'static,
    ) -> Option<Self> {
        Some(Self {
            target,
            task: runtime.spawn_long_cancelable(wake, run)?,
        })
    }

    pub(super) fn start_process(
        runtime: &BlockingRuntime,
        target: GithubTaskTarget,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
        run: impl FnOnce(BlockingCancellation) -> Result<T, String> + Send + 'static,
    ) -> Option<Self> {
        Some(Self {
            target,
            task: runtime.spawn_process_cancelable(wake, run)?,
        })
    }
}

pub(super) struct TaskPlugin<T>(PhantomData<fn() -> T>);

impl<T> Default for TaskPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Send + Sync + 'static> Plugin for TaskPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_message::<GithubCompletion<T>>()
            .add_message::<GithubFailure<T>>()
            .add_systems(Update, poll::<T>.in_set(GithubSet::Poll));
    }
}

fn poll<T: Send + Sync + 'static>(
    mut tasks: Query<(Entity, &mut GithubTask<T>)>,
    entities: Query<()>,
    link_identity: Query<(&Revision, &ExternalId, &ExternalScopeRef), With<ExternalLink>>,
    mut completed: MessageWriter<GithubCompletion<T>>,
    mut failed_tasks: MessageWriter<GithubFailure<T>>,
    mut commands: Commands,
) {
    for (operation, mut task) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        commands.entity(operation).remove::<GithubTask<T>>();
        let result = match result {
            Ok(result) => result,
            Err(_) => Err("GitHub operation panicked".to_string()),
        };
        match result {
            Ok(output) => {
                let Some(target) = task.target.destination(
                    entities.contains(task.target.entity()),
                    link_identity.get(task.target.entity()).ok(),
                ) else {
                    commands.entity(operation).insert(OperationState::Succeeded);
                    continue;
                };
                completed.write(GithubCompletion {
                    operation,
                    target,
                    binding: task.target.clone(),
                    output,
                });
            }
            Err(error) => {
                let target = task.target.entity();
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error.clone()).expect("valid operation metadata"),
                ));
                failed_tasks.write(GithubFailure {
                    operation,
                    target,
                    binding: task.target.clone(),
                    error,
                    marker: PhantomData,
                });
            }
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct GithubScopes {
    known: HashSet<Id<ExternalScope>>,
}

#[derive(SystemParam)]
pub(super) struct GithubScopeAccess<'w, 's> {
    registry: ResMut<'w, GithubScopes>,
    scopes: Query<'w, 's, &'static Id<ExternalScope>, With<ExternalScope>>,
    commands: Commands<'w, 's>,
}

impl GithubScopeAccess<'_, '_> {
    pub(super) fn ensure(&mut self, id: &Id<ExternalScope>, namespace: &str) {
        self.registry.known.extend(self.scopes.iter().cloned());
        if !self.registry.known.insert(id.clone()) {
            return;
        }
        self.commands.spawn((
            Save,
            ExternalScope,
            id.clone(),
            ProviderKey("github".into()),
            ExternalHost("github.com".into()),
            ExternalNamespace(namespace.to_string()),
            Name::new(namespace.to_string()),
        ));
    }
}

pub(super) struct OperationResult {
    kind: String,
    result: Result<(), String>,
}

impl OperationResult {
    pub(super) fn succeeded(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            result: Ok(()),
        }
    }

    pub(super) fn failed(kind: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            result: Err(error.into()),
        }
    }
}

impl Command for OperationResult {
    type Out = ();
    fn apply(self, world: &mut World) {
        let operation = world
            .spawn(Operation::bundle(self.kind).expect("valid operation kind"))
            .id();
        match self.result {
            Ok(()) => {
                world
                    .entity_mut(operation)
                    .insert(OperationState::Succeeded);
            }
            Err(error) => {
                world.entity_mut(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error).expect("valid operation error"),
                ));
            }
        }
    }
}
