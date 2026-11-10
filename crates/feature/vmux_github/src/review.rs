use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::Id;
use vmux_auth::{CanAccess, Connection, ConnectionState, CredentialBroker, ProviderKey};
use vmux_connector::{ExternalLink, ExternalScope, ExternalScopeRef, ResourceKind, ReviewArtifact};
use vmux_ecs::{
    BlockingCancellation, BlockingRuntime, BlockingTask, Cwd, Operation, OperationError,
    OperationKind, OperationState, Revision,
};
use vmux_editor::{Project, ProjectUnavailable};
use vmux_git::{GitReviewPublished, GitReviewSubmission, GitReviewSubmissionFinished};
use vmux_session::Session;

use crate::request::{GithubReviewComment, PullRequestReviewRequest};
use crate::{GithubCoordinates, GithubPullRequest, GithubRepository};

pub(crate) struct GithubReviewPlugin;

struct ReviewProject {
    entity: Entity,
    root: PathBuf,
}

impl ReviewProject {
    fn containing<'a>(
        path: &Path,
        projects: impl Iterator<Item = (Entity, &'a Cwd)>,
    ) -> Option<Self> {
        projects
            .filter(|(_, cwd)| path.starts_with(&cwd.0))
            .max_by_key(|(_, cwd)| cwd.0.components().count())
            .map(|(entity, cwd)| Self {
                entity,
                root: cwd.0.clone(),
            })
    }
}

enum ReviewConnectionError {
    Unavailable,
    Ambiguous,
}

impl ReviewConnectionError {
    fn message(&self) -> &'static str {
        match self {
            Self::Unavailable => "Selected GitHub connection is unavailable",
            Self::Ambiguous => "Multiple GitHub connections can access this repository",
        }
    }
}

struct ReviewConnection;

impl ReviewConnection {
    fn resolve<'a>(
        connections: impl Iterator<
            Item = (
                &'a Id<Connection>,
                &'a ProviderKey,
                &'a ConnectionState,
                &'a CanAccess,
            ),
        >,
        scope: &Id<ExternalScope>,
    ) -> Result<Id<Connection>, ReviewConnectionError> {
        let mut candidates = connections.filter(|(_, provider, state, access)| {
            provider.0 == "github"
                && **state == ConnectionState::Connected
                && access.0.contains(scope)
        });
        let Some((connection, _, _, _)) = candidates.next() else {
            return Err(ReviewConnectionError::Unavailable);
        };
        if candidates.next().is_some() {
            return Err(ReviewConnectionError::Ambiguous);
        }
        Ok(connection.clone())
    }
}

impl Plugin for GithubReviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockingRuntime>()
            .init_resource::<vmux_git::GitReviewProviderRegistry>()
            .add_message::<PullRequestReviewRequest>()
            .add_systems(
                Update,
                (
                    sync_capabilities.run_if(capabilities_changed),
                    route,
                    prepare,
                    submit,
                    poll,
                    finish,
                )
                    .chain(),
            );
    }
}

type ChangedProjects<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Project>,
        Or<(Changed<Project>, Changed<ProjectUnavailable>, Changed<Cwd>)>,
    ),
>;

type ChangedSessions<'w, 's> =
    Query<'w, 's, (), (With<Session>, Or<(Changed<Session>, Changed<Cwd>)>)>;

type ChangedLinks<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<ExternalLink>,
        Or<(
            Changed<ExternalLink>,
            Changed<GithubRepository>,
            Changed<GithubPullRequest>,
            Changed<ResourceKind>,
            Changed<ExternalScopeRef>,
            Changed<ChildOf>,
            Changed<ReviewArtifact>,
        )>,
    ),
>;

type ChangedConnections<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Connection>,
        Or<(
            Changed<Connection>,
            Changed<ProviderKey>,
            Changed<ConnectionState>,
            Changed<CanAccess>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct ChangedCapabilities<'w, 's> {
    projects: ChangedProjects<'w, 's>,
    sessions: ChangedSessions<'w, 's>,
    links: ChangedLinks<'w, 's>,
    connections: ChangedConnections<'w, 's>,
}

impl ChangedCapabilities<'_, '_> {
    fn any(&self) -> bool {
        !self.projects.is_empty()
            || !self.sessions.is_empty()
            || !self.links.is_empty()
            || !self.connections.is_empty()
    }
}

#[derive(SystemParam)]
struct CapabilityRemovals<'w, 's> {
    projects: RemovedComponents<'w, 's, Project>,
    unavailable: RemovedComponents<'w, 's, ProjectUnavailable>,
    cwd: RemovedComponents<'w, 's, Cwd>,
    repositories: RemovedComponents<'w, 's, GithubRepository>,
    pulls: RemovedComponents<'w, 's, GithubPullRequest>,
    links: RemovedComponents<'w, 's, ExternalLink>,
    kinds: RemovedComponents<'w, 's, ResourceKind>,
    scopes: RemovedComponents<'w, 's, ExternalScopeRef>,
    parents: RemovedComponents<'w, 's, ChildOf>,
    artifacts: RemovedComponents<'w, 's, ReviewArtifact>,
    sessions: RemovedComponents<'w, 's, Session>,
    connections: RemovedComponents<'w, 's, Connection>,
    providers: RemovedComponents<'w, 's, ProviderKey>,
    states: RemovedComponents<'w, 's, ConnectionState>,
    access: RemovedComponents<'w, 's, CanAccess>,
}

impl CapabilityRemovals<'_, '_> {
    fn any(&mut self) -> bool {
        let mut removed = self.projects.read().count() != 0;
        removed |= self.unavailable.read().count() != 0;
        removed |= self.cwd.read().count() != 0;
        removed |= self.repositories.read().count() != 0;
        removed |= self.pulls.read().count() != 0;
        removed |= self.links.read().count() != 0;
        removed |= self.kinds.read().count() != 0;
        removed |= self.scopes.read().count() != 0;
        removed |= self.parents.read().count() != 0;
        removed |= self.artifacts.read().count() != 0;
        removed |= self.sessions.read().count() != 0;
        removed |= self.connections.read().count() != 0;
        removed |= self.providers.read().count() != 0;
        removed |= self.states.read().count() != 0;
        removed |= self.access.read().count() != 0;
        removed
    }
}

fn capabilities_changed(
    changed: ChangedCapabilities,
    mut removed: CapabilityRemovals,
    mut initialized: Local<bool>,
) -> bool {
    let first = !*initialized;
    *initialized = true;
    first || changed.any() || removed.any()
}

type AvailableProjects<'w, 's> =
    Query<'w, 's, (Entity, &'static Cwd), (With<Project>, Without<ProjectUnavailable>)>;

type RepositoryLinks<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static GithubRepository,
        &'static ResourceKind,
        &'static ExternalScopeRef,
    ),
    With<ExternalLink>,
>;

type ReviewPullLinks<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static GithubPullRequest,
        &'static ExternalScopeRef,
    ),
    (With<ExternalLink>, With<ReviewArtifact>),
>;

type DetailedReviewPullLinks<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GithubPullRequest,
        &'static Revision,
        &'static ChildOf,
        &'static ExternalScopeRef,
    ),
    (With<ExternalLink>, With<ReviewArtifact>),
>;

type ReviewSubmissionLinks<'w, 's> = Query<
    'w,
    's,
    (
        &'static GithubPullRequest,
        &'static Revision,
        &'static ExternalScopeRef,
    ),
    (With<ExternalLink>, With<ReviewArtifact>),
>;

type ReviewConnections<'w, 's> = Query<
    'w,
    's,
    (
        &'static Id<Connection>,
        &'static ProviderKey,
        &'static ConnectionState,
        &'static CanAccess,
    ),
    With<Connection>,
>;

fn sync_capabilities(
    projects: AvailableProjects,
    repositories: RepositoryLinks,
    sessions: Query<&Cwd, With<Session>>,
    pulls: ReviewPullLinks,
    connections: ReviewConnections,
    mut providers: ResMut<vmux_git::GitReviewProviderRegistry>,
) {
    let mut capabilities = Vec::new();
    for (parent, pull, pull_scope) in &pulls {
        let Ok(cwd) = sessions.get(parent.parent()) else {
            continue;
        };
        let Some(project) = ReviewProject::containing(&cwd.0, projects.iter()) else {
            continue;
        };
        let linked = repositories
            .iter()
            .any(|(parent, repository, kind, scope)| {
                parent.parent() == project.entity
                    && *kind == ResourceKind::repository()
                    && repository.owner_login == pull.owner
                    && repository.name == pull.repository
                    && scope.0 == pull_scope.0
            });
        if !linked {
            continue;
        }
        if ReviewConnection::resolve(connections.iter(), &pull_scope.0).is_err() {
            continue;
        }
        capabilities.push(vmux_git::GitReviewProviderCapability {
            repository: project.root.display().to_string(),
            base_sha: pull.base_sha.clone(),
            head_sha: pull.head_sha.clone(),
        });
    }
    if providers
        .bypass_change_detection()
        .replace("github", capabilities)
    {
        providers.set_changed();
    }
}

#[derive(Component)]
struct GithubReviewTask {
    publication: GitReviewPublished,
    task: BlockingTask<Result<(), String>>,
}

#[derive(Component)]
struct GithubReviewPreparation {
    submission: GitReviewSubmission,
    task: BlockingTask<Result<Option<LocalReview>, String>>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GithubReviewKey {
    source: Entity,
    fingerprint: u64,
}

#[derive(Component)]
struct GithubReviewFinished;

impl GithubReviewTask {
    fn spawn(
        runtime: &BlockingRuntime,
        request: PullRequestReviewRequest,
        source: vmux_auth::CredentialSource,
        wake: Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
    ) -> Option<Self> {
        let publication = GitReviewPublished {
            source: request.source,
            notes: request.notes.clone(),
        };
        let task = runtime.spawn_long_cancelable(wake, move |cancellation| {
            let api = crate::api::GithubApi::new(source)?;
            let current = api.pull_request(&GithubCoordinates {
                owner: request.owner.clone(),
                repository: request.repository.clone(),
                number: Some(request.number),
            })?;
            if current.base_sha != request.base_sha || current.head_sha != request.commit_id {
                return Err("GitHub pull request changed before review submission".to_string());
            }
            if cancellation.is_canceled() {
                return Err("GitHub review canceled".to_string());
            }
            api.create_review(&request)
        })?;
        Some(Self { publication, task })
    }
}

struct LocalReview {
    owner: String,
    repository: String,
    head: String,
    head_sha: String,
    base: String,
    base_sha: String,
}

impl LocalReview {
    fn read(
        root: &Path,
        target: &vmux_git::state::GitReviewTarget,
        cancellation: &BlockingCancellation,
    ) -> Result<Option<Self>, String> {
        let vmux_git::state::GitReviewTarget::Branch {
            repository: target_repository,
            base,
            base_revision: _,
            base_sha,
            head,
            head_sha,
        } = target
        else {
            return Err("Only branch reviews can publish to GitHub".to_string());
        };
        if Path::new(target_repository) != root {
            return Err("Git review repository changed before submission".to_string());
        }
        Self::validate_revisions(root, target, cancellation)?;
        let remote = Self::upstream_remote(root, head, cancellation)?;
        let remote_url = Self::git(root, &["remote", "get-url", "--", &remote], cancellation)?;
        let Some((owner, repository)) = Self::remote(&remote_url)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            owner,
            repository,
            head: Self::branch(head, &remote),
            head_sha: head_sha.clone(),
            base: base.clone(),
            base_sha: base_sha.clone(),
        }))
    }

    fn validate_revisions(
        root: &Path,
        target: &vmux_git::state::GitReviewTarget,
        cancellation: &BlockingCancellation,
    ) -> Result<(), String> {
        let vmux_git::state::GitReviewTarget::Branch {
            repository,
            base_revision,
            base_sha,
            head,
            head_sha,
            ..
        } = target
        else {
            return Err("Only branch reviews can publish to GitHub".to_string());
        };
        if Path::new(repository) != root {
            return Err("Git review repository changed before submission".to_string());
        }
        let current_base = Self::commit_id(root, base_revision, cancellation)?;
        let current_head = Self::commit_id(root, head, cancellation)?;
        if current_base != *base_sha || current_head != *head_sha {
            return Err("Git review revisions changed before submission".to_string());
        }
        Ok(())
    }

    fn commit_id(
        root: &Path,
        reference: &str,
        cancellation: &BlockingCancellation,
    ) -> Result<String, String> {
        let revision = format!("{reference}^{{commit}}");
        Self::git(
            root,
            &["rev-parse", "--verify", "--end-of-options", &revision],
            cancellation,
        )
    }

    fn upstream_remote(
        root: &Path,
        reference: &str,
        cancellation: &BlockingCancellation,
    ) -> Result<String, String> {
        let full = Self::git(
            root,
            &[
                "rev-parse",
                "--symbolic-full-name",
                "--end-of-options",
                reference,
            ],
            cancellation,
        )?;
        if let Some(remote) = full.strip_prefix("refs/remotes/") {
            let remote = remote.split('/').next().unwrap_or_default();
            if !remote.is_empty() {
                return Ok(remote.to_string());
            }
        }
        let branch = full.strip_prefix("refs/heads/").unwrap_or(reference);
        let upstream = Self::git(
            root,
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)",
                &format!("refs/heads/{branch}"),
            ],
            cancellation,
        )?;
        if upstream.is_empty() || upstream == "." {
            return Err("Selected branch has no GitHub upstream remote".to_string());
        }
        Ok(upstream)
    }

    fn git(
        root: &Path,
        args: &[&str],
        cancellation: &BlockingCancellation,
    ) -> Result<String, String> {
        let mut command = vmux_git::GitProcess::command();
        command.arg("-C").arg(root).args(args);
        vmux_git::GitProcess::output(
            &mut command,
            Duration::from_secs(15),
            64 * 1024,
            "Git review preparation",
            cancellation,
            || Ok(()),
        )
    }

    fn remote(value: &str) -> Result<Option<(String, String)>, String> {
        let path = if let Some(path) = value.strip_prefix("git@github.com:") {
            path.to_string()
        } else {
            let Ok(url) = url::Url::parse(value) else {
                return Ok(None);
            };
            if url.host_str() != Some("github.com") {
                return Ok(None);
            }
            if !matches!(url.scheme(), "https" | "ssh")
                || url.password().is_some()
                || (!url.username().is_empty() && url.username() != "git")
            {
                return Err("Git remote is not a GitHub repository".to_string());
            }
            url.path().trim_start_matches('/').to_string()
        };
        let path = path.strip_suffix(".git").unwrap_or(&path);
        let mut parts = path.split('/');
        let owner = parts.next().unwrap_or_default();
        let repository = parts.next().unwrap_or_default();
        if owner.is_empty() || repository.is_empty() || parts.next().is_some() {
            return Err("Git remote is not a GitHub repository".to_string());
        }
        Ok(Some((owner.to_string(), repository.to_string())))
    }

    fn branch(value: &str, remote: &str) -> String {
        value
            .strip_prefix("refs/heads/")
            .or_else(|| value.strip_prefix(&format!("refs/remotes/{remote}/")))
            .or_else(|| value.strip_prefix(&format!("{remote}/")))
            .unwrap_or(value)
            .to_string()
    }
}

fn route(
    mut submissions: MessageReader<GitReviewSubmission>,
    active: Query<(&GithubReviewKey, &OperationState), With<Operation>>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut keys = active
        .iter()
        .filter_map(|(key, state)| (*state == OperationState::Running).then_some(*key))
        .collect::<HashSet<_>>();
    for submission in submissions.read() {
        let key = GithubReviewKey {
            source: submission.source,
            fingerprint: submission.fingerprint,
        };
        if !keys.insert(key) {
            continue;
        }
        let root = PathBuf::from(&submission.repo_root);
        let target = submission.snapshot.target.clone();
        let wake = wake.as_deref().map(|wake| (**wake).clone());
        let Some(task) = runtime.spawn_process_cancelable(wake, move |cancellation| {
            LocalReview::read(&root, &target, &cancellation)
        }) else {
            failed(&mut commands, key, "GitHub operation queue is full");
            continue;
        };
        commands.spawn((
            Operation,
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
            OperationKind::new("github.pull_request.review").expect("valid operation metadata"),
            OperationState::Running,
            key,
            GithubReviewPreparation {
                submission: submission.clone(),
                task,
            },
        ));
    }
}

#[derive(SystemParam)]
struct ReviewPreparationContext<'w, 's> {
    projects: AvailableProjects<'w, 's>,
    repositories: RepositoryLinks<'w, 's>,
    pulls: DetailedReviewPullLinks<'w, 's>,
    sessions: Query<'w, 's, &'static Cwd, With<Session>>,
    connections: ReviewConnections<'w, 's>,
}

fn prepare(
    mut preparations: Query<(Entity, &mut GithubReviewPreparation)>,
    context: ReviewPreparationContext,
    mut requests: MessageWriter<PullRequestReviewRequest>,
    mut commands: Commands,
) {
    for (operation, mut preparation) in &mut preparations {
        let Some(result) = preparation.task.poll() else {
            continue;
        };
        commands
            .entity(operation)
            .remove::<GithubReviewPreparation>();
        let local = match result {
            Ok(Ok(Some(local))) => local,
            Ok(Ok(None)) => {
                commands.entity(operation).insert(OperationState::Canceled);
                continue;
            }
            Ok(Err(error)) => {
                commands.queue(ReviewFailure::new(operation, error));
                continue;
            }
            Err(_) => {
                commands.queue(ReviewFailure::new(
                    operation,
                    "GitHub review preparation panicked",
                ));
                continue;
            }
        };
        let submission = &preparation.submission;
        let project_path = PathBuf::from(&submission.repo_root);
        let Some(project) = ReviewProject::containing(&project_path, context.projects.iter())
        else {
            commands.queue(ReviewFailure::new(
                operation,
                "GitHub project link is unavailable",
            ));
            continue;
        };
        let mut matching_pulls = context.pulls.iter().filter(|(_, pull, _, parent, _)| {
            context.sessions.get(parent.parent()).is_ok_and(|cwd| {
                ReviewProject::containing(&cwd.0, context.projects.iter())
                    .is_some_and(|session_project| session_project.entity == project.entity)
            }) && pull.head_owner == local.owner
                && pull.head_repository == local.repository
                && pull.head == local.head
                && pull.head_sha == local.head_sha
                && pull.base == local.base
                && pull.base_sha == local.base_sha
        });
        let Some((link, pull, revision, _, pull_scope)) = matching_pulls.next() else {
            commands.queue(ReviewFailure::new(
                operation,
                "Linked GitHub pull request is unavailable",
            ));
            continue;
        };
        if matching_pulls.next().is_some() {
            commands.queue(ReviewFailure::new(
                operation,
                "Multiple linked GitHub pull requests match this branch",
            ));
            continue;
        }
        let mut linked_repositories =
            context
                .repositories
                .iter()
                .filter(|(parent, repository, kind, scope)| {
                    parent.parent() == project.entity
                        && **kind == ResourceKind::repository()
                        && repository.owner_login == pull.owner
                        && repository.name == pull.repository
                        && scope.0 == pull_scope.0
                });
        let Some((_, _, _, repository_scope)) = linked_repositories.next() else {
            commands.queue(ReviewFailure::new(
                operation,
                "GitHub repository link is unavailable",
            ));
            continue;
        };
        if linked_repositories.next().is_some() {
            commands.queue(ReviewFailure::new(
                operation,
                "Multiple GitHub repository links match this review",
            ));
            continue;
        }
        let connection =
            match ReviewConnection::resolve(context.connections.iter(), &repository_scope.0) {
                Ok(connection) => connection,
                Err(error) => {
                    commands.queue(ReviewFailure::new(operation, error.message()));
                    continue;
                }
            };
        let notes = submission.snapshot.provider_notes().collect::<Vec<_>>();
        let mut comments = Vec::with_capacity(notes.len());
        let mut receipts = Vec::with_capacity(notes.len());
        for note in notes {
            let vmux_git::state::GitReviewLocation::Line { side, line } = note.anchor.location
            else {
                continue;
            };
            comments.push(GithubReviewComment {
                path: note.anchor.path.clone(),
                body: note.body.trim().to_string(),
                line,
                side: side.into(),
            });
            receipts.push(note.receipt());
        }
        if comments.is_empty() {
            commands.queue(ReviewFailure::new(
                operation,
                "GitHub review has no publishable notes",
            ));
            continue;
        }
        requests.write(PullRequestReviewRequest {
            operation,
            connection,
            owner: pull.owner.clone(),
            repository: pull.repository.clone(),
            number: pull.number,
            base_sha: local.base_sha,
            commit_id: local.head_sha,
            body: format!("Review from vmux · {} notes", comments.len()),
            comments: comments.into(),
            source: submission.source,
            notes: receipts.into(),
            link,
            revision: *revision,
        });
    }
}

#[derive(SystemParam)]
struct ReviewSubmissionContext<'w, 's> {
    operations: Query<'w, 's, &'static OperationState, With<Operation>>,
    connections: ReviewConnections<'w, 's>,
    pulls: ReviewSubmissionLinks<'w, 's>,
    broker: CredentialBroker<'w, 's>,
    runtime: Res<'w, BlockingRuntime>,
    wake: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
}

fn submit(
    mut requests: MessageReader<PullRequestReviewRequest>,
    context: ReviewSubmissionContext,
    mut commands: Commands,
) {
    for request in requests.read() {
        if context.operations.get(request.operation) != Ok(&OperationState::Running) {
            continue;
        }
        let Ok((pull, revision, scope)) = context.pulls.get(request.link) else {
            commands.queue(ReviewFailure::new(
                request.operation,
                "GitHub pull request link is unavailable",
            ));
            continue;
        };
        if *revision != request.revision
            || pull.owner != request.owner
            || pull.repository != request.repository
            || pull.number != request.number
            || pull.base_sha != request.base_sha
            || pull.head_sha != request.commit_id
        {
            commands.queue(ReviewFailure::new(
                request.operation,
                "GitHub pull request changed before review submission",
            ));
            continue;
        }
        let Some((_, provider, _, _)) =
            context
                .connections
                .iter()
                .find(|(held, provider, state, access)| {
                    *held == &request.connection
                        && provider.0 == "github"
                        && **state == ConnectionState::Connected
                        && access.0.contains(&scope.0)
                })
        else {
            commands.queue(ReviewFailure::new(
                request.operation,
                "GitHub connection is unavailable",
            ));
            continue;
        };
        let source = match context.broker.source(provider, &request.connection) {
            Ok(source) => source,
            Err(error) => {
                commands.queue(ReviewFailure::new(request.operation, error));
                continue;
            }
        };
        let wake = context.wake.as_deref().map(|wake| (**wake).clone());
        let Some(task) = GithubReviewTask::spawn(&context.runtime, request.clone(), source, wake)
        else {
            commands.queue(ReviewFailure::new(
                request.operation,
                "GitHub operation queue is full",
            ));
            continue;
        };
        commands.entity(request.operation).insert(task);
    }
}

fn poll(
    mut tasks: Query<(Entity, &mut GithubReviewTask)>,
    mut published: MessageWriter<GitReviewPublished>,
    mut commands: Commands,
) {
    for (operation, mut task) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        commands.entity(operation).remove::<GithubReviewTask>();
        let result = match result {
            Ok(result) => result,
            Err(_) => Err("GitHub review operation panicked".to_string()),
        };
        match result {
            Ok(()) => {
                published.write(task.publication.clone());
                commands.entity(operation).insert(OperationState::Succeeded);
            }
            Err(error) => {
                commands.entity(operation).insert((
                    OperationState::Failed,
                    OperationError::new(error).expect("valid operation metadata"),
                ));
            }
        }
    }
}

type FinishedReviewOperations<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static GithubReviewKey, &'static OperationState),
    (Changed<OperationState>, Without<GithubReviewFinished>),
>;

fn finish(
    operations: FinishedReviewOperations,
    mut finished: MessageWriter<GitReviewSubmissionFinished>,
    mut commands: Commands,
) {
    for (operation, key, state) in &operations {
        if *state == OperationState::Running {
            continue;
        }
        finished.write(GitReviewSubmissionFinished {
            source: key.source,
            fingerprint: key.fingerprint,
        });
        commands.entity(operation).insert(GithubReviewFinished);
    }
}

fn failed(commands: &mut Commands, key: GithubReviewKey, error: impl Into<String>) {
    commands.spawn((
        Operation,
        vmux_ecs::Uuid(uuid::Uuid::new_v4().to_string()),
        OperationKind::new("github.pull_request.review").expect("valid operation metadata"),
        OperationState::Failed,
        OperationError::new(error.into()).expect("valid operation metadata"),
        key,
    ));
}

struct ReviewFailure {
    operation: Entity,
    error: String,
}

impl ReviewFailure {
    fn new(operation: Entity, error: impl Into<String>) -> Self {
        Self {
            operation,
            error: error.into(),
        }
    }
}

impl Command for ReviewFailure {
    type Out = ();
    fn apply(self, world: &mut World) {
        if let Ok(mut operation) = world.get_entity_mut(self.operation) {
            operation.insert((
                OperationState::Failed,
                OperationError::new(self.error).expect("valid operation error"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_project_uses_the_deepest_containing_root() {
        let outer = Entity::from_raw_u32(1).unwrap();
        let inner = Entity::from_raw_u32(2).unwrap();
        let projects = [
            (outer, Cwd(PathBuf::from("/projects/vmux"))),
            (inner, Cwd(PathBuf::from("/projects/vmux/client"))),
        ];

        let project = ReviewProject::containing(
            Path::new("/projects/vmux/client/crates"),
            projects.iter().map(|(entity, cwd)| (*entity, cwd)),
        )
        .unwrap();

        assert_eq!(project.entity, inner);
        assert_eq!(project.root, PathBuf::from("/projects/vmux/client"));
    }

    #[test]
    fn review_connection_rejects_ambiguous_accounts() {
        let first = Id::<Connection>::new("first");
        let second = Id::<Connection>::new("second");
        let provider = ProviderKey("github".into());
        let state = ConnectionState::Connected;
        let scope = Id::<ExternalScope>::new("github.com/accounts/1");
        let access = CanAccess(vec![scope.clone()]);
        let connections = [
            (&first, &provider, &state, &access),
            (&second, &provider, &state, &access),
        ];

        assert!(matches!(
            ReviewConnection::resolve(connections.into_iter(), &scope),
            Err(ReviewConnectionError::Ambiguous)
        ));
    }

    #[test]
    fn remote_identity_and_branch_names_are_normalized() {
        assert_eq!(
            LocalReview::remote("git@github.com:vmux-ai/vmux.git")
                .unwrap()
                .unwrap(),
            ("vmux-ai".into(), "vmux".into())
        );
        assert_eq!(
            LocalReview::branch("upstream/feature/review", "upstream"),
            "feature/review"
        );
        assert_eq!(
            LocalReview::remote("https://example.com/vmux-ai/vmux.git").unwrap(),
            None
        );
    }
}
