use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::Id;
use vmux_auth::{
    AccountExternalId, CanAccess, Connected, Connection, ConnectionName, ConnectionState,
    CredentialBroker, CredentialSource, ProviderKey,
};
use vmux_connector::{
    ExternalId, ExternalKey, ExternalLink, ExternalLinkIdentity, ExternalLinkIndex,
    ExternalLinkSpec, ExternalScope, ExternalScopeRef, ExternalUrl, ResourceKind, ReviewArtifact,
    SyncPolicy, SynchronizedFields,
};
use vmux_ecs::{BlockingCancellation, BlockingRuntime, OperationError, OperationState, Revision};
use vmux_editor::Project;
use vmux_session::{Session, SessionId};

use crate::api::{GithubAccountAccess, GithubApi};
use crate::git_credentials::GitCredential;
use crate::model::{
    GithubCoordinates, GithubIssue, GithubPullRequest, GithubRepository, IssueSearchResult,
    PullRequestCreateResult, RepositoryCloneResult, RepositorySearchResult, github_scope_owner_id,
};
use crate::request::{
    IssueAssociateRequest, IssueDetachRequest, IssueSearchRequest, PullRequestCreateRequest,
    RefreshRequest, RepositoryAssociateRequest, RepositoryCloneRequest, RepositorySearchRequest,
};

use super::association::{IssueAssociation, RepositoryAssociation};
use super::{
    GithubCompletion, GithubConnections, GithubFailure, GithubRequesterTaskKind, GithubScopeAccess,
    GithubSet, GithubTask, GithubTaskTarget, OperationResult, TaskPlugin,
};

pub(super) struct RequestPlugin;

impl Plugin for RequestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingRepositorySearches>()
            .init_resource::<PendingIssueSearches>()
            .init_resource::<RequestWake>()
            .add_message::<RepositorySearchRequest>()
            .add_message::<RepositoryAssociateRequest>()
            .add_message::<RepositoryCloneRequest>()
            .add_message::<IssueSearchRequest>()
            .add_message::<IssueAssociateRequest>()
            .add_message::<IssueDetachRequest>()
            .add_message::<PullRequestCreateRequest>()
            .add_message::<RefreshRequest>()
            .add_message::<RepositorySearchResult>()
            .add_message::<IssueSearchResult>()
            .add_message::<RepositoryCloneResult>()
            .add_message::<GithubCompletion<AccountOutput>>()
            .add_message::<GithubCompletion<RepositorySearchOutput>>()
            .add_message::<GithubCompletion<IssueSearchOutput>>()
            .add_message::<GithubCompletion<RepositoryCloneOutput>>()
            .add_message::<GithubCompletion<PullRequestOutput>>()
            .add_message::<GithubCompletion<RefreshOutput>>()
            .add_message::<GithubFailure<AccountOutput>>()
            .add_message::<GithubFailure<RepositorySearchOutput>>()
            .add_message::<GithubFailure<IssueSearchOutput>>()
            .add_message::<GithubFailure<RepositoryCloneOutput>>()
            .add_message::<GithubFailure<PullRequestOutput>>()
            .add_message::<GithubFailure<RefreshOutput>>()
            .add_observer(connection_ready)
            .add_systems(Update, retry_account_validation.in_set(GithubSet::Request))
            .add_systems(
                Update,
                (
                    repository_search,
                    repository_associate,
                    repository_clone,
                    issue_search,
                    issue_associate,
                    issue_detach,
                    create_pull,
                    refresh,
                )
                    .in_set(GithubSet::Request),
            )
            .add_plugins((
                TaskPlugin::<AccountOutput>::default(),
                TaskPlugin::<RepositorySearchOutput>::default(),
                TaskPlugin::<IssueSearchOutput>::default(),
                TaskPlugin::<RepositoryCloneOutput>::default(),
                TaskPlugin::<PullRequestOutput>::default(),
                TaskPlugin::<RefreshOutput>::default(),
            ))
            .add_systems(
                Update,
                (
                    finish_account,
                    finish_account_failure,
                    finish_repository_search,
                    finish_repository_search_failure,
                    finish_issue_search,
                    finish_issue_search_failure,
                    finish_repository_clone,
                    finish_repository_clone_failure,
                    finish_pull,
                    finish_refresh,
                )
                    .in_set(GithubSet::Complete),
            )
            .add_systems(Last, wake_requests);
    }
}

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

struct PendingSearch<T> {
    request: T,
    due: Instant,
}

#[derive(Resource, Default)]
struct PendingRepositorySearches(HashMap<(Entity, String), PendingSearch<RepositorySearchRequest>>);

#[derive(Resource, Default)]
struct PendingIssueSearches(HashMap<(Entity, String), PendingSearch<IssueSearchRequest>>);

#[derive(Resource, Default)]
struct RequestWake {
    deadline: Option<Instant>,
    generation: Arc<AtomicU64>,
}

#[derive(Component)]
struct AccountValidationRetry {
    failures: u32,
    due: Instant,
}

impl AccountValidationRetry {
    fn after_failure(failures: u32) -> Self {
        let shift = failures.saturating_sub(1).min(6);
        Self {
            failures,
            due: Instant::now() + Duration::from_secs(1_u64 << shift),
        }
    }

    fn defer(&mut self) {
        *self = Self::after_failure(self.failures.saturating_add(1));
    }
}

struct AccountOutput(GithubAccountAccess);

struct RepositorySearchOutput {
    generation: u64,
    connection: Id<Connection>,
    repositories: Vec<GithubRepository>,
}

struct IssueSearchOutput {
    generation: u64,
    connection: Id<Connection>,
    issues: Vec<GithubIssue>,
}

struct RepositoryCloneOutput {
    generation: u64,
    connection: Id<Connection>,
    destination: std::path::PathBuf,
}

struct PullRequestOutput {
    connection: Id<Connection>,
    pull: GithubPullRequest,
}

enum RefreshOutput {
    Issue(GithubIssue),
    PullRequest(GithubPullRequest),
}

#[derive(SystemParam)]
struct GithubRequestContext<'w, 's> {
    connections: GithubConnections<'w, 's>,
    broker: CredentialBroker<'w, 's>,
    runtime: Res<'w, BlockingRuntime>,
    wake: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
}

impl GithubRequestContext<'_, '_> {
    fn access(
        &self,
        connection: &Id<Connection>,
        commands: &mut Commands,
    ) -> Option<(Entity, CredentialSource)> {
        let Some((entity, _, provider, _, _)) =
            self.connections
                .iter()
                .find(|(_, held, provider, state, _)| {
                    *held == connection
                        && provider.0 == "github"
                        && **state == ConnectionState::Connected
                })
        else {
            commands.queue(OperationResult::failed(
                "github.operation",
                "GitHub connection is unavailable",
            ));
            return None;
        };
        match self.broker.source(provider, connection) {
            Ok(source) => Some((entity, source)),
            Err(error) => {
                commands.queue(OperationResult::failed("github.operation", error));
                None
            }
        }
    }

    fn access_scope(
        &self,
        connection: &Id<Connection>,
        scope: &Id<ExternalScope>,
        commands: &mut Commands,
    ) -> Option<(Entity, CredentialSource)> {
        let authorized = self
            .connections
            .iter()
            .any(|(_, held, provider, state, access)| {
                held == connection
                    && provider.0 == "github"
                    && *state == ConnectionState::Connected
                    && access.0.contains(scope)
            });
        if !authorized {
            commands.queue(OperationResult::failed(
                "github.operation",
                "GitHub connection cannot access this resource",
            ));
            return None;
        }
        self.access(connection, commands)
    }

    fn validation(
        &self,
        connection: &Id<Connection>,
        entity: Entity,
        commands: &mut Commands,
    ) -> Option<CredentialSource> {
        let valid = self
            .connections
            .get(entity)
            .is_ok_and(|(_, id, provider, state, _)| {
                id == connection && provider.0 == "github" && *state == ConnectionState::Connected
            });
        if !valid {
            commands.queue(OperationResult::failed(
                "github.account",
                "GitHub connection is unavailable",
            ));
            return None;
        }
        match self.broker.validation_source(connection) {
            Ok(source) => Some(source),
            Err(error) => {
                commands.queue(OperationResult::failed("github.account", error));
                None
            }
        }
    }

    fn start_account_validation(
        &self,
        connection: &Id<Connection>,
        entity: Entity,
        commands: &mut Commands,
    ) -> Option<bool> {
        let source = self.validation(connection, entity, commands)?;
        Some(self.start(
            commands,
            "github.account",
            GithubTaskTarget::BoundConnection { connection: entity },
            move || Ok(AccountOutput(GithubApi::new(source)?.account()?)),
        ))
    }

    fn start<T: Send + Sync + 'static>(
        &self,
        commands: &mut Commands,
        kind: &str,
        target: GithubTaskTarget,
        run: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> bool {
        let Some(task) = GithubTask::start(&self.runtime, target, self.wake(), run) else {
            commands.queue(OperationResult::failed(
                kind,
                "GitHub operation queue is full",
            ));
            return false;
        };
        commands
            .spawn(vmux_ecs::Operation::bundle(kind).expect("valid operation kind"))
            .insert((
                OperationState::Running,
                vmux_ecs::OperationTarget::new(task.target.entity()),
                task,
            ));
        true
    }

    fn start_cancelable<T: Send + Sync + 'static>(
        &self,
        commands: &mut Commands,
        kind: &str,
        target: GithubTaskTarget,
        run: impl FnOnce(BlockingCancellation) -> Result<T, String> + Send + 'static,
    ) -> bool {
        let Some(task) = GithubTask::start_cancelable(&self.runtime, target, self.wake(), run)
        else {
            commands.queue(OperationResult::failed(
                kind,
                "GitHub operation queue is full",
            ));
            return false;
        };
        commands
            .spawn(vmux_ecs::Operation::bundle(kind).expect("valid operation kind"))
            .insert((
                OperationState::Running,
                vmux_ecs::OperationTarget::new(task.target.entity()),
                task,
            ));
        true
    }

    fn start_process<T: Send + Sync + 'static>(
        &self,
        commands: &mut Commands,
        kind: &str,
        target: GithubTaskTarget,
        run: impl FnOnce(BlockingCancellation) -> Result<T, String> + Send + 'static,
    ) -> bool {
        let Some(task) = GithubTask::start_process(&self.runtime, target, self.wake(), run) else {
            commands.queue(OperationResult::failed(
                kind,
                "GitHub operation queue is full",
            ));
            return false;
        };
        commands
            .spawn(vmux_ecs::Operation::bundle(kind).expect("valid operation kind"))
            .insert((
                OperationState::Running,
                vmux_ecs::OperationTarget::new(task.target.entity()),
                task,
            ));
        true
    }

    fn wake(&self) -> Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>> {
        self.wake.as_deref().map(|wake| (**wake).clone())
    }
}

fn connection_ready(trigger: On<Connected>, context: GithubRequestContext, mut commands: Commands) {
    if trigger.event().provider.0 != "github" {
        return;
    }
    if context.start_account_validation(&trigger.event().id, trigger.event_target(), &mut commands)
        == Some(false)
    {
        commands
            .entity(trigger.event_target())
            .insert(AccountValidationRetry::after_failure(1));
    }
}

fn retry_account_validation(
    mut retries: Query<
        (
            Entity,
            &Id<Connection>,
            &ConnectionState,
            &mut AccountValidationRetry,
        ),
        With<Connection>,
    >,
    active: Query<&GithubTask<AccountOutput>>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    let now = Instant::now();
    for (entity, id, state, mut retry) in &mut retries {
        if *state != ConnectionState::Connected {
            commands.entity(entity).remove::<AccountValidationRetry>();
            continue;
        }
        if retry.due > now || active.iter().any(|task| task.target.entity() == entity) {
            continue;
        }
        match context.start_account_validation(id, entity, &mut commands) {
            Some(true) | None => {
                commands.entity(entity).remove::<AccountValidationRetry>();
            }
            Some(false) => retry.defer(),
        }
    }
}

fn repository_search(
    mut requests: MessageReader<RepositorySearchRequest>,
    mut pending: ResMut<PendingRepositorySearches>,
    active: TaskReplacements<RepositorySearchOutput>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    let now = Instant::now();
    for request in requests.read() {
        let key = (request.requester, request.connection.as_str().to_string());
        if pending
            .0
            .get(&key)
            .is_none_or(|held| held.request.generation <= request.generation)
        {
            pending.0.insert(
                key,
                PendingSearch {
                    request: request.clone(),
                    due: now + SEARCH_DEBOUNCE,
                },
            );
        }
    }
    let ready = pending
        .0
        .iter()
        .filter_map(|(key, search)| (search.due <= now).then_some(key.clone()))
        .collect::<Vec<_>>();
    for key in ready {
        let Some(search) = pending.0.remove(&key) else {
            continue;
        };
        let request = search.request;
        if !active.replace(
            GithubRequesterTaskKind::RepositorySearch,
            request.requester,
            request.generation,
            &request.connection,
            &mut commands,
        ) {
            continue;
        }
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let query = request.query.clone();
        let generation = request.generation;
        let connection_id = request.connection.clone();
        context.start_cancelable(
            &mut commands,
            "github.repository.search",
            GithubTaskTarget::Requester {
                requester: request.requester,
                generation,
                kind: GithubRequesterTaskKind::RepositorySearch,
                id: request.connection.clone(),
            },
            move |cancellation| {
                if cancellation.is_canceled() {
                    return Err("GitHub repository search canceled".into());
                }
                let repositories = GithubApi::new(source)?.repositories(&query)?;
                if cancellation.is_canceled() {
                    return Err("GitHub repository search canceled".into());
                }
                Ok(RepositorySearchOutput {
                    generation,
                    connection: connection_id,
                    repositories,
                })
            },
        );
    }
}

fn issue_search(
    mut requests: MessageReader<IssueSearchRequest>,
    mut pending: ResMut<PendingIssueSearches>,
    active: TaskReplacements<IssueSearchOutput>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    let now = Instant::now();
    for request in requests.read() {
        let key = (request.requester, request.connection.as_str().to_string());
        if pending
            .0
            .get(&key)
            .is_none_or(|held| held.request.generation <= request.generation)
        {
            pending.0.insert(
                key,
                PendingSearch {
                    request: request.clone(),
                    due: now + SEARCH_DEBOUNCE,
                },
            );
        }
    }
    let ready = pending
        .0
        .iter()
        .filter_map(|(key, search)| (search.due <= now).then_some(key.clone()))
        .collect::<Vec<_>>();
    for key in ready {
        let Some(search) = pending.0.remove(&key) else {
            continue;
        };
        let request = search.request;
        if !active.replace(
            GithubRequesterTaskKind::IssueSearch,
            request.requester,
            request.generation,
            &request.connection,
            &mut commands,
        ) {
            continue;
        }
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let query = request.query.clone();
        let generation = request.generation;
        let connection_id = request.connection.clone();
        context.start_cancelable(
            &mut commands,
            "github.issue.search",
            GithubTaskTarget::Requester {
                requester: request.requester,
                generation,
                kind: GithubRequesterTaskKind::IssueSearch,
                id: request.connection.clone(),
            },
            move |cancellation| {
                if cancellation.is_canceled() {
                    return Err("GitHub issue search canceled".into());
                }
                let issues = GithubApi::new(source)?.issues(&query)?;
                if cancellation.is_canceled() {
                    return Err("GitHub issue search canceled".into());
                }
                Ok(IssueSearchOutput {
                    generation,
                    connection: connection_id,
                    issues,
                })
            },
        );
    }
}

fn wake_requests(
    repositories: Res<PendingRepositorySearches>,
    issues: Res<PendingIssueSearches>,
    validations: Query<&AccountValidationRetry>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut wake: ResMut<RequestWake>,
) {
    let deadline = repositories
        .0
        .values()
        .map(|search| search.due)
        .chain(issues.0.values().map(|search| search.due))
        .chain(validations.iter().map(|retry| retry.due))
        .min();
    if wake.deadline == deadline {
        return;
    }
    wake.deadline = deadline;
    let generation = wake.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let Some(deadline) = deadline else {
        return;
    };
    let Some(proxy) = proxy else {
        wake.deadline = None;
        return;
    };
    let proxy = (**proxy).clone();
    let active = Arc::clone(&wake.generation);
    let _ = std::thread::Builder::new()
        .name("vmux-github-request-wake".into())
        .spawn(move || {
            if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
                std::thread::sleep(delay);
            }
            if active.load(Ordering::Acquire) == generation {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        });
}

fn repository_clone(
    mut requests: MessageReader<RepositoryCloneRequest>,
    projects: vmux_ecs::profile::Projects,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    for request in requests.read() {
        let destination = match projects.path().and_then(|root| {
            vmux_path::ScopedPath::resolve(root, &request.destination)
                .map(vmux_path::ScopedPath::into_path_buf)
                .map_err(|error| error.to_string())
        }) {
            Ok(destination) => destination,
            Err(error) => {
                commands.queue(OperationResult::failed(
                    "github.repository.clone",
                    format!("Invalid clone destination: {error}"),
                ));
                continue;
            }
        };
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let owner = request.repository.owner_login.clone();
        let repository = request.repository.name.clone();
        let result_destination = destination.clone();
        let generation = request.generation;
        let connection_id = request.connection.clone();
        context.start_process(
            &mut commands,
            "github.repository.clone",
            GithubTaskTarget::Requester {
                requester: request.requester,
                generation,
                kind: GithubRequesterTaskKind::RepositoryClone,
                id: request.connection.clone(),
            },
            move |cancellation| {
                let lease = source.lease()?;
                GitCredential::clone(&owner, &repository, &destination, &lease, &cancellation)?;
                Ok(RepositoryCloneOutput {
                    generation,
                    connection: connection_id,
                    destination: result_destination,
                })
            },
        );
    }
}

fn repository_associate(
    mut requests: MessageReader<RepositoryAssociateRequest>,
    projects: Query<(Entity, &Id<Project>), With<Project>>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some((project, _)) = projects.iter().find(|(_, id)| *id == &request.project) else {
            commands.queue(OperationResult::failed(
                "github.repository.associate",
                "Project is unavailable",
            ));
            continue;
        };
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let connection = request.connection.clone();
        let owner = request.repository.owner_login.clone();
        let repository = request.repository.name.clone();
        let expected_id = request.repository.external_id();
        context.start(
            &mut commands,
            "github.repository.associate",
            GithubTaskTarget::BoundEntity { entity: project },
            move || {
                let repository = GithubApi::new(source)?.repository(&owner, &repository)?;
                if repository.external_id() != expected_id {
                    return Err("GitHub repository identity changed before association".into());
                }
                Ok(RepositoryAssociation {
                    connection,
                    repository,
                })
            },
        );
    }
}

fn issue_associate(
    mut requests: MessageReader<IssueAssociateRequest>,
    sessions: Query<
        (
            Entity,
            &SessionId,
            Option<&vmux_session::mutation::Revision>,
        ),
        With<Session>,
    >,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some((session, _, revision)) =
            sessions.iter().find(|(_, id, _)| *id == &request.session)
        else {
            commands.queue(OperationResult::failed(
                "github.issue.associate",
                "Session is unavailable",
            ));
            continue;
        };
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let connection = request.connection.clone();
        let coordinates = GithubCoordinates {
            owner: request.issue.owner.clone(),
            repository: request.issue.repository.clone(),
            number: Some(request.issue.number),
        };
        let sync_policy = request.sync_policy;
        let expected = revision.copied().unwrap_or_default();
        let expected_id = request.issue.external_id();
        let owner_id = (request.issue.owner_id > 0).then_some(request.issue.owner_id);
        context.start(
            &mut commands,
            "github.issue.associate",
            GithubTaskTarget::BoundEntity { entity: session },
            move || {
                let issue = GithubApi::new(source)?.issue_with_owner_id(&coordinates, owner_id)?;
                if issue.external_id() != expected_id {
                    return Err("GitHub issue identity changed before association".into());
                }
                Ok(IssueAssociation {
                    connection,
                    issue,
                    sync_policy,
                    expected,
                })
            },
        );
    }
}

fn issue_detach(
    mut requests: MessageReader<IssueDetachRequest>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    links: Query<(Entity, &ProviderKey, &ExternalId, &ResourceKind, &ChildOf), With<ExternalLink>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some((session, _)) = sessions.iter().find(|(_, id)| *id == &request.session) else {
            continue;
        };
        let Some((link, _, _, _, _)) = links.iter().find(|(_, provider, id, kind, parent)| {
            parent.parent() == session
                && provider.0 == "github"
                && **kind == ResourceKind::issue()
                && id.0 == request.external_id
        }) else {
            continue;
        };
        commands.entity(link).despawn();
        commands.queue(OperationResult::succeeded("github.issue.detach"));
    }
}

fn create_pull(
    mut requests: MessageReader<PullRequestCreateRequest>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Some((session, _)) = sessions.iter().find(|(_, id)| *id == &request.session) else {
            commands.queue(OperationResult::failed(
                "github.pull_request.create",
                "Session is unavailable",
            ));
            continue;
        };
        let Some((_, source)) = context.access(&request.connection, &mut commands) else {
            continue;
        };
        let connection = request.connection.clone();
        let request = request.clone();
        context.start(
            &mut commands,
            "github.pull_request.create",
            GithubTaskTarget::BoundEntity { entity: session },
            move || {
                Ok(PullRequestOutput {
                    connection,
                    pull: GithubApi::new(source)?.create_pull_request(&request)?,
                })
            },
        );
    }
}

type RefreshLinks<'w, 's> = Query<
    'w,
    's,
    (
        &'static ResourceKind,
        &'static GithubCoordinates,
        Option<&'static Revision>,
        &'static ExternalId,
        &'static ExternalScopeRef,
    ),
    With<ExternalLink>,
>;

fn refresh(
    mut requests: MessageReader<RefreshRequest>,
    links: RefreshLinks,
    active: Query<&GithubTask<RefreshOutput>>,
    context: GithubRequestContext,
    mut commands: Commands,
) {
    let mut pending = active
        .iter()
        .filter_map(|task| task.target.revision())
        .collect::<HashSet<_>>();
    for request in requests.read() {
        let Ok((kind, coordinates, revision, external_id, scope)) = links.get(request.link) else {
            commands.queue(OperationResult::failed(
                "github.refresh",
                "GitHub link is unavailable",
            ));
            continue;
        };
        let Some((_, source)) = context.access_scope(&request.connection, &scope.0, &mut commands)
        else {
            continue;
        };
        let kind = kind.clone();
        let coordinates = coordinates.clone();
        let owner_id = github_scope_owner_id(&scope.0);
        let target_revision = revision.copied().unwrap_or(Revision::new(0));
        if !pending.insert((request.link, target_revision)) {
            continue;
        }
        context.start(
            &mut commands,
            "github.refresh",
            GithubTaskTarget::BoundRevision {
                entity: request.link,
                revision: target_revision,
                external_id: external_id.0.clone(),
                scope: scope.0.clone(),
            },
            move || {
                let api = GithubApi::new(source)?;
                if kind == ResourceKind::issue() {
                    Ok(RefreshOutput::Issue(
                        api.issue_with_owner_id(&coordinates, owner_id)?,
                    ))
                } else if kind == ResourceKind::pull_request() {
                    Ok(RefreshOutput::PullRequest(api.pull_request(&coordinates)?))
                } else {
                    Err("GitHub link cannot be refreshed".to_string())
                }
            },
        );
    }
}

fn finish_account(
    mut completed: MessageReader<GithubCompletion<AccountOutput>>,
    connections: Query<&ConnectionState, With<Connection>>,
    mut scope_registry: GithubScopeAccess,
    mut commands: Commands,
) {
    for completion in completed.read() {
        if connections.get(completion.target) != Ok(&ConnectionState::Connected) {
            commands
                .entity(completion.operation)
                .insert(OperationState::Succeeded);
            continue;
        }
        let account = &completion.output.0;
        let own = Id::<ExternalScope>::new(format!("github.com/accounts/{}", account.id));
        scope_registry.ensure(&own, &account.login);
        let name = if account.name.trim().is_empty() {
            account.login.clone()
        } else {
            account.name.clone()
        };
        commands.entity(completion.target).insert((
            ConnectionName(name),
            AccountExternalId(account.id.to_string()),
            CanAccess(vec![own]),
        ));
        commands
            .entity(completion.operation)
            .insert(OperationState::Succeeded);
    }
}

fn finish_account_failure(
    mut failures: MessageReader<GithubFailure<AccountOutput>>,
    mut connections: Query<&mut ConnectionState, With<Connection>>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        if let Ok(mut state) = connections.get_mut(failure.target)
            && *state == ConnectionState::Connected
        {
            *state = ConnectionState::Failed(failure.error.clone());
            commands.entity(failure.target).insert(CanAccess::default());
        }
        commands
            .entity(failure.operation)
            .insert(OperationError::new(failure.error.clone()).expect("valid operation metadata"));
    }
}

fn finish_repository_search(
    mut completed: MessageReader<GithubCompletion<RepositorySearchOutput>>,
    mut connections: Query<(&Id<Connection>, &ConnectionState, &mut CanAccess), With<Connection>>,
    mut results: MessageWriter<RepositorySearchResult>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        if let Some(mut access) = connections.iter_mut().find_map(|(id, state, access)| {
            (id == &completion.output.connection && *state == ConnectionState::Connected)
                .then_some(access)
        }) {
            for scope in completion
                .output
                .repositories
                .iter()
                .filter_map(GithubRepository::scope_id)
            {
                if !access.0.contains(&scope) {
                    access.0.push(scope);
                }
            }
        }
        results.write(RepositorySearchResult {
            requester: completion.target,
            generation: completion.output.generation,
            connection: completion.output.connection.clone(),
            result: Ok(completion.output.repositories.clone()),
        });
        commands
            .entity(completion.operation)
            .insert(OperationState::Succeeded);
    }
}

fn finish_repository_search_failure(
    mut failures: MessageReader<GithubFailure<RepositorySearchOutput>>,
    mut results: MessageWriter<RepositorySearchResult>,
) {
    for failure in failures.read() {
        let Some((requester, generation, connection)) = failure
            .binding
            .requester(GithubRequesterTaskKind::RepositorySearch)
        else {
            continue;
        };
        results.write(RepositorySearchResult {
            requester,
            generation,
            connection: connection.clone(),
            result: Err(failure.error.clone()),
        });
    }
}

fn finish_issue_search(
    mut completed: MessageReader<GithubCompletion<IssueSearchOutput>>,
    mut connections: Query<(&Id<Connection>, &ConnectionState, &mut CanAccess), With<Connection>>,
    mut results: MessageWriter<IssueSearchResult>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        if let Some(mut access) = connections.iter_mut().find_map(|(id, state, access)| {
            (id == &completion.output.connection && *state == ConnectionState::Connected)
                .then_some(access)
        }) {
            for scope in completion
                .output
                .issues
                .iter()
                .filter_map(GithubIssue::scope_id)
            {
                if !access.0.contains(&scope) {
                    access.0.push(scope);
                }
            }
        }
        results.write(IssueSearchResult {
            requester: completion.target,
            generation: completion.output.generation,
            connection: completion.output.connection.clone(),
            result: Ok(completion.output.issues.clone()),
        });
        commands
            .entity(completion.operation)
            .insert(OperationState::Succeeded);
    }
}

fn finish_issue_search_failure(
    mut failures: MessageReader<GithubFailure<IssueSearchOutput>>,
    mut results: MessageWriter<IssueSearchResult>,
) {
    for failure in failures.read() {
        let Some((requester, generation, connection)) = failure
            .binding
            .requester(GithubRequesterTaskKind::IssueSearch)
        else {
            continue;
        };
        results.write(IssueSearchResult {
            requester,
            generation,
            connection: connection.clone(),
            result: Err(failure.error.clone()),
        });
    }
}

fn finish_repository_clone(
    mut completed: MessageReader<GithubCompletion<RepositoryCloneOutput>>,
    mut results: MessageWriter<RepositoryCloneResult>,
    mut project_index: MessageWriter<vmux_editor::ProjectIndexRefresh>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        project_index.write(vmux_editor::ProjectIndexRefresh);
        results.write(RepositoryCloneResult {
            requester: completion.target,
            generation: completion.output.generation,
            connection: completion.output.connection.clone(),
            result: Ok(completion.output.destination.clone()),
        });
        commands
            .entity(completion.operation)
            .insert(OperationState::Succeeded);
    }
}

fn finish_repository_clone_failure(
    mut failures: MessageReader<GithubFailure<RepositoryCloneOutput>>,
    mut results: MessageWriter<RepositoryCloneResult>,
) {
    for failure in failures.read() {
        let Some((requester, generation, connection)) = failure
            .binding
            .requester(GithubRequesterTaskKind::RepositoryClone)
        else {
            continue;
        };
        results.write(RepositoryCloneResult {
            requester,
            generation,
            connection: connection.clone(),
            result: Err(failure.error.clone()),
        });
    }
}

fn finish_pull(
    mut completed: MessageReader<GithubCompletion<PullRequestOutput>>,
    sessions: Query<(), With<Session>>,
    mut connections: Query<(&Id<Connection>, &ConnectionState, &mut CanAccess), With<Connection>>,
    mut index: ResMut<ExternalLinkIndex>,
    mut scope_registry: GithubScopeAccess,
    mut commands: Commands,
) {
    for completion in completed.read() {
        if !sessions.contains(completion.target) {
            commands
                .entity(completion.operation)
                .insert(OperationState::Canceled);
            continue;
        }
        let pull = &completion.output.pull;
        let Some(scope) = pull.scope_id() else {
            commands.entity(completion.operation).insert((
                OperationState::Failed,
                OperationError::new("GitHub pull request owner identity is unavailable")
                    .expect("valid operation metadata"),
            ));
            continue;
        };
        if let Some(mut access) = connections.iter_mut().find_map(|(id, state, access)| {
            (id == &completion.output.connection && *state == ConnectionState::Connected)
                .then_some(access)
        }) && !access.0.contains(&scope)
        {
            access.0.push(scope.clone());
        }
        if let Err(conflict) = index.reserve(ExternalLinkSpec {
            identity: ExternalLinkIdentity {
                provider: ProviderKey("github".into()),
                scope: scope.clone(),
                kind: ResourceKind::pull_request(),
                id: ExternalId(pull.external_id()),
            },
            parent: completion.target,
            primary: false,
            sync_policy: SyncPolicy::ReferenceOnly,
            fields: &[],
        }) {
            commands.entity(completion.operation).insert((
                OperationState::Failed,
                OperationError::new(conflict.to_string()).expect("valid operation metadata"),
            ));
            continue;
        }
        scope_registry.ensure(&scope, &pull.owner);
        commands.spawn((
            ExternalLink,
            ChildOf(completion.target),
            ProviderKey("github".into()),
            ExternalScopeRef(scope),
            ResourceKind::pull_request(),
            ExternalId(pull.external_id()),
            ExternalKey(format!(
                "{}/{}#{}",
                pull.owner, pull.repository, pull.number
            )),
            ExternalUrl(pull.html_url.clone()),
            SyncPolicy::ReferenceOnly,
            SynchronizedFields::default(),
            ReviewArtifact,
            GithubCoordinates {
                owner: pull.owner.clone(),
                repository: pull.repository.clone(),
                number: Some(pull.number),
            },
            pull.clone(),
            Revision::new(0),
        ));
        commands.entity(completion.operation).insert((
            OperationState::Succeeded,
            PullRequestCreateResult(pull.clone()),
        ));
    }
}

fn finish_refresh(
    mut completed: MessageReader<GithubCompletion<RefreshOutput>>,
    revisions: Query<&Revision>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        match &completion.output {
            RefreshOutput::Issue(issue) => {
                let external_id = issue.external_id();
                let scope = issue.scope_id();
                if completion.binding.expected_external_id() != Some(external_id.as_str())
                    || scope.as_ref() != completion.binding.expected_scope()
                {
                    commands.entity(completion.operation).insert((
                        OperationState::Failed,
                        OperationError::new(
                            "GitHub issue identity or scope changed during refresh",
                        )
                        .expect("valid operation metadata"),
                    ));
                    continue;
                }
                commands.entity(completion.target).insert((
                    ExternalUrl(issue.html_url.clone()),
                    issue.clone(),
                    next_revision(completion.target, &revisions),
                ));
            }
            RefreshOutput::PullRequest(pull) => {
                let external_id = pull.external_id();
                let scope = pull.scope_id();
                if completion.binding.expected_external_id() != Some(external_id.as_str())
                    || scope.as_ref() != completion.binding.expected_scope()
                {
                    commands.entity(completion.operation).insert((
                        OperationState::Failed,
                        OperationError::new(
                            "GitHub pull request identity or scope changed during refresh",
                        )
                        .expect("valid operation metadata"),
                    ));
                    continue;
                }
                commands.entity(completion.target).insert((
                    ExternalUrl(pull.html_url.clone()),
                    pull.clone(),
                    next_revision(completion.target, &revisions),
                ));
            }
        }
        commands
            .entity(completion.operation)
            .insert(OperationState::Succeeded);
    }
}

fn next_revision(entity: Entity, revisions: &Query<&Revision>) -> Revision {
    Revision::new(
        revisions
            .get(entity)
            .map_or(1, |revision| revision.get().saturating_add(1)),
    )
}

#[derive(SystemParam)]
struct TaskReplacements<'w, 's, T: Send + Sync + 'static> {
    active: Query<'w, 's, (Entity, &'static GithubTask<T>)>,
}

impl<T: Send + Sync + 'static> TaskReplacements<'_, '_, T> {
    fn replace(
        &self,
        kind: GithubRequesterTaskKind,
        requester: Entity,
        generation: u64,
        connection: &Id<Connection>,
        commands: &mut Commands,
    ) -> bool {
        let mut newest = true;
        for (operation, task) in self.active.iter() {
            let Some((held_requester, held_generation, held_connection)) =
                task.target.requester(kind)
            else {
                continue;
            };
            if held_requester != requester || held_connection != connection {
                continue;
            }
            if held_generation >= generation {
                newest = false;
                continue;
            }
            commands
                .entity(operation)
                .remove::<GithubTask<T>>()
                .insert(OperationState::Canceled);
        }
        newest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;
    use vmux_ecs::Operation;

    #[test]
    fn failed_account_validation_fails_the_connection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GithubFailure<AccountOutput>>()
            .add_systems(Update, finish_account_failure);
        let connection = app
            .world_mut()
            .spawn((Connection, ConnectionState::Connected, CanAccess::default()))
            .id();
        let operation = app.world_mut().spawn(Operation).id();
        app.world_mut()
            .resource_mut::<Messages<GithubFailure<AccountOutput>>>()
            .write(GithubFailure {
                operation,
                target: connection,
                binding: GithubTaskTarget::BoundConnection { connection },
                error: "validation failed".into(),
                marker: PhantomData,
            });

        app.update();

        assert_eq!(
            app.world().get::<ConnectionState>(connection),
            Some(&ConnectionState::Failed("validation failed".into()))
        );
    }

    #[test]
    fn failed_account_validation_does_not_reopen_a_disconnected_connection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GithubFailure<AccountOutput>>()
            .add_systems(Update, finish_account_failure);
        let connection = app
            .world_mut()
            .spawn((
                Connection,
                ConnectionState::Disconnected,
                CanAccess::default(),
            ))
            .id();
        let operation = app.world_mut().spawn(Operation).id();
        app.world_mut()
            .resource_mut::<Messages<GithubFailure<AccountOutput>>>()
            .write(GithubFailure {
                operation,
                target: connection,
                binding: GithubTaskTarget::BoundConnection { connection },
                error: "validation failed".into(),
                marker: PhantomData,
            });

        app.update();

        assert_eq!(
            app.world().get::<ConnectionState>(connection),
            Some(&ConnectionState::Disconnected)
        );
    }
}
