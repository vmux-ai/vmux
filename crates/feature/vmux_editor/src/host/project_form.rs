use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::{Operation, OperationCompleted, OperationPlugin, OperationTarget};
use vmux_git::{
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
};

use crate::project::{
    ProjectFolderPickRequest, ProjectFormUiState, ProjectId, ProjectRoute, ProjectSaveRequest,
    ProjectSource,
};

use super::project::{GitHubRepository, ProjectImport, ProjectName};
use super::project_persistence::{
    Mutation, PersistenceSet, ProjectSaved, Registrations, WriteCompleted, WriteRequest,
};

pub(super) struct ProjectFormPlugin;

impl Plugin for ProjectFormPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<ProjectFormUiState>::default(),
            UiEventPlugin::<(ProjectSaveRequest, ProjectFolderPickRequest)>::default(),
        ))
        .add_message::<RepositoryProvisionRequest>()
        .add_message::<RepositoryProvisionCompleted>()
        .add_observer(pick)
        .add_observer(save)
        .add_systems(
            Update,
            (
                prepare,
                finish_picks,
                finish_preparations,
                finish_provisions,
                finish_validations,
            )
                .chain()
                .after(PersistenceSet::Load)
                .before(PersistenceSet::Write),
        )
        .add_systems(Update, finish_commits.after(PersistenceSet::Write));
    }
}

#[derive(Component)]
struct SelectedFolder {
    request_id: String,
    directory: PathBuf,
}

#[derive(Component)]
struct FolderTask {
    webview: Entity,
    request_id: String,
    task: Task<Result<Option<PathBuf>, String>>,
}

#[derive(Component)]
struct SaveDraft {
    root: PathBuf,
    webview: Entity,
    request: ProjectSaveRequest,
    selected: Option<PathBuf>,
}

#[derive(Component)]
struct PreparationTask {
    webview: Entity,
    request_id: String,
    task: Task<Result<SavePlan, String>>,
}

#[derive(Component)]
struct PreparedSave {
    plan: SavePlan,
    webview: Entity,
}

#[derive(Component)]
struct ValidationTask {
    webview: Entity,
    request_id: String,
    task: Task<Result<SavePlan, String>>,
}

#[derive(Component)]
struct PendingCommit {
    webview: Entity,
    request_id: String,
}

#[derive(Clone)]
struct SavePlan {
    root: PathBuf,
    request: ProjectSaveRequest,
    directory: PathBuf,
    id: ProjectId,
    repository: Option<String>,
    provision: Option<RepositoryProvisionOperation>,
}

impl SavePlan {
    fn prepare(
        root: PathBuf,
        request: ProjectSaveRequest,
        selected: Option<PathBuf>,
        current: Option<PathBuf>,
    ) -> Result<Self, String> {
        let name = request.name.trim();
        if name.is_empty() || name.len() > 256 || name.contains(['\0', '\n', '\r']) {
            return Err(vmux_ui::i18n::translate("project-name-invalid"));
        }
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let (directory, repository, provision) = match &request.source {
            ProjectSource::New => {
                if current.is_some() {
                    return Err(vmux_ui::i18n::translate("project-name-invalid"));
                }
                let operation = ProjectName::parse(name)?.provision(&root)?;
                let RepositoryProvisionOperation::Initialize { destination, .. } = &operation
                else {
                    unreachable!()
                };
                (destination.clone(), None, Some(operation))
            }
            ProjectSource::Local(path) => {
                let directory = PathBuf::from(path);
                if selected.as_ref() != Some(&directory) && current.as_ref() != Some(&directory) {
                    return Err(vmux_ui::i18n::translate("agent-project-choose"));
                }
                let alias = if current.is_none() {
                    ProjectImport::register(&root, &directory)?
                } else {
                    directory
                };
                (alias, None, None)
            }
            ProjectSource::Github(value) => {
                let repository = GitHubRepository::parse(value)?;
                let directory = repository.destination(&root);
                let remote = repository.remote().to_string();
                if current.as_ref() == Some(&directory) {
                    (directory, Some(remote), None)
                } else {
                    let operation = repository.provision(&root)?;
                    (directory, Some(remote), Some(operation))
                }
            }
        };
        let id = match &request.project {
            Some(id) => id.clone(),
            None => ProjectId(format!("{}{}", ProjectRoute::ROOT, uuid::Uuid::new_v4())),
        };
        Ok(Self {
            root,
            request,
            directory,
            id,
            repository,
            provision,
        })
    }

    fn validate(mut self) -> Result<Self, String> {
        self.directory = self
            .directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !self.directory.join(".git").exists() {
            return Err(vmux_ui::i18n::translate("project-import-not-repository"));
        }
        Ok(self)
    }
}

#[derive(SystemParam)]
struct Feedback<'w, 's> {
    states: Query<'w, 's, &'static vmux_ecs::UiState<ProjectFormUiState>>,
    commands: Commands<'w, 's>,
}

impl Feedback<'_, '_> {
    fn matches(&self, webview: Entity, request_id: &str) -> bool {
        self.states
            .get(webview)
            .ok()
            .and_then(vmux_ecs::UiState::current)
            .is_some_and(|state| state.request_id == request_id)
    }

    fn start(&mut self, webview: Entity, request_id: &str) -> bool {
        let current = self
            .states
            .get(webview)
            .ok()
            .and_then(vmux_ecs::UiState::current);
        if current.is_some_and(|state| state.request_id == request_id && state.pending) {
            return false;
        }
        let folder = current
            .filter(|state| state.request_id == request_id)
            .and_then(|state| state.folder.clone());
        self.commands
            .trigger(vmux_ecs::UiStateWrite::<ProjectFormUiState>::from_event(
                webview,
                &ProjectFormUiState {
                    request_id: request_id.to_string(),
                    folder,
                    pending: true,
                    error: None,
                    saved: None,
                },
            ));
        true
    }

    fn finish(
        &mut self,
        webview: Entity,
        request_id: &str,
        result: Result<Option<PathBuf>, String>,
        saved: Option<ProjectId>,
    ) {
        let mut state = self
            .states
            .get(webview)
            .ok()
            .and_then(vmux_ecs::UiState::current)
            .cloned()
            .unwrap_or_else(|| ProjectFormUiState {
                request_id: request_id.to_string(),
                ..Default::default()
            });
        if state.request_id != request_id {
            return;
        }
        state.pending = false;
        match result {
            Ok(Some(path)) => state.folder = Some(path.to_string_lossy().into_owned()),
            Ok(None) => {}
            Err(error) => state.error = Some(error),
        }
        state.saved = saved;
        self.commands
            .trigger(vmux_ecs::UiStateWrite::<ProjectFormUiState>::from_event(
                webview, &state,
            ));
    }
}

fn pick(
    trigger: On<UiInput<ProjectFolderPickRequest>>,
    pages: Query<&vmux_ecs::PageMetadata>,
    mut feedback: Feedback,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !pages
        .get(event.webview)
        .is_ok_and(|page| ProjectRoute::is_page(&page.url))
        || !feedback.start(event.webview, &event.payload.request_id)
    {
        return;
    }
    let directory = event.payload.directory.clone();
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let mut dialog =
            rfd::AsyncFileDialog::new().set_title(vmux_ui::i18n::translate("project-import"));
        if !directory.is_empty() {
            dialog = dialog.set_directory(directory);
        }
        let result = match dialog.pick_folder().await {
            Some(folder) => folder
                .path()
                .canonicalize()
                .map(Some)
                .map_err(|error| error.to_string()),
            None => Ok(None),
        };
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn(FolderTask {
        webview: event.webview,
        request_id: event.payload.request_id.clone(),
        task,
    });
}

#[derive(SystemParam)]
struct SourceSelection<'w, 's> {
    pages: Query<'w, 's, &'static vmux_ecs::PageMetadata>,
    selected: Query<'w, 's, &'static SelectedFolder>,
}

fn save(
    trigger: On<UiInput<ProjectSaveRequest>>,
    projects: vmux_ecs::profile::Projects,
    selection: SourceSelection,
    mut feedback: Feedback,
    mut commands: Commands,
) {
    let event = trigger.event();
    if !selection
        .pages
        .get(event.webview)
        .is_ok_and(|page| ProjectRoute::is_page(&page.url))
        || !feedback.start(event.webview, &event.payload.request_id)
    {
        return;
    }
    let root = match projects.path() {
        Ok(root) => root.to_path_buf(),
        Err(error) => {
            feedback.finish(event.webview, &event.payload.request_id, Err(error), None);
            return;
        }
    };
    let chosen = selection
        .selected
        .get(event.webview)
        .ok()
        .filter(|selected| selected.request_id == event.payload.request_id)
        .map(|selected| selected.directory.clone());
    commands
        .spawn(Operation::bundle("project.save").unwrap())
        .insert((
            OperationTarget::new(event.webview),
            SaveDraft {
                root,
                webview: event.webview,
                request: event.payload.clone(),
                selected: chosen,
            },
        ));
}

fn prepare(
    drafts: Query<(Entity, &SaveDraft)>,
    registrations: Registrations,
    mut feedback: Feedback,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, draft) in &drafts {
        match registrations.ready(&draft.root) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(error) => {
                feedback.finish(
                    draft.webview,
                    &draft.request.request_id,
                    Err(error.clone()),
                    None,
                );
                commands.entity(entity).remove::<SaveDraft>();
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error),
                });
                continue;
            }
        }
        let current = if let Some(id) = &draft.request.project {
            let revision = registrations
                .get(&draft.root, &id.0)
                .map_or(0, |(_, _, _, revision)| revision.0);
            if revision != draft.request.revision {
                let error = vmux_ui::i18n::translate("project-edit-conflict");
                feedback.finish(
                    draft.webview,
                    &draft.request.request_id,
                    Err(error.clone()),
                    None,
                );
                commands.entity(entity).remove::<SaveDraft>();
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error),
                });
                continue;
            }
            match registrations.resolve(&id.0, &draft.root) {
                Some(Ok(directory)) => Some(directory),
                result => {
                    let error = result
                        .and_then(Result::err)
                        .unwrap_or_else(|| vmux_ui::i18n::translate("error-page-not-found"));
                    feedback.finish(
                        draft.webview,
                        &draft.request.request_id,
                        Err(error.clone()),
                        None,
                    );
                    commands.entity(entity).remove::<SaveDraft>();
                    commands.trigger(OperationCompleted {
                        entity,
                        result: Err(error),
                    });
                    continue;
                }
            }
        } else {
            None
        };
        let root = draft.root.clone();
        let request = draft.request.clone();
        let chosen = draft.selected.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = SavePlan::prepare(root, request, chosen, current);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).remove::<SaveDraft>().insert((
            vmux_ecs::OperationState::Running,
            PreparationTask {
                webview: draft.webview,
                request_id: draft.request.request_id.clone(),
                task,
            },
        ));
    }
}

fn finish_picks(
    mut tasks: Query<(Entity, &mut FolderTask)>,
    mut feedback: Feedback,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        if !feedback.matches(task.webview, &task.request_id) {
            commands.entity(entity).despawn();
            continue;
        }
        if let Ok(Some(directory)) = &result {
            commands.entity(task.webview).insert(SelectedFolder {
                request_id: task.request_id.clone(),
                directory: directory.clone(),
            });
        }
        feedback.finish(task.webview, &task.request_id, result, None);
        commands.entity(entity).despawn();
    }
}

fn finish_preparations(
    mut tasks: Query<(Entity, &mut PreparationTask)>,
    mut requests: MessageWriter<RepositoryProvisionRequest>,
    mut feedback: Feedback,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<PreparationTask>();
        match result {
            Ok(mut plan) => {
                if let Some(operation) = plan.provision.take() {
                    requests.write(RepositoryProvisionRequest {
                        requester: entity,
                        operation,
                    });
                    commands.entity(entity).insert(PreparedSave {
                        plan,
                        webview: task.webview,
                    });
                } else {
                    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
                    let work = IoTaskPool::get().spawn(async move {
                        let result = plan.validate();
                        if let Some(wake) = wake {
                            let _ = wake.send_event(WinitUserEvent::WakeUp);
                        }
                        result
                    });
                    commands.entity(entity).insert(ValidationTask {
                        webview: task.webview,
                        request_id: task.request_id.clone(),
                        task: work,
                    });
                }
            }
            Err(error) => {
                feedback.finish(task.webview, &task.request_id, Err(error.clone()), None);
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error),
                });
            }
        }
    }
}

fn finish_provisions(
    mut completed: MessageReader<RepositoryProvisionCompleted>,
    prepared: Query<&PreparedSave>,
    mut feedback: Feedback,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for completed in completed.read() {
        let Ok(prepared) = prepared.get(completed.requester) else {
            continue;
        };
        let webview = prepared.webview;
        let request_id = prepared.plan.request.request_id.clone();
        if let Err(error) = &completed.result {
            feedback.finish(webview, &request_id, Err(error.clone()), None);
            commands
                .entity(completed.requester)
                .remove::<PreparedSave>();
            commands.trigger(OperationCompleted {
                entity: completed.requester,
                result: Err(error.clone()),
            });
            continue;
        }
        let plan = prepared.plan.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = plan.validate();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(completed.requester)
            .remove::<PreparedSave>()
            .insert(ValidationTask {
                webview,
                request_id,
                task,
            });
    }
}

fn finish_validations(
    mut tasks: Query<(Entity, &mut ValidationTask)>,
    mut feedback: Feedback,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).remove::<ValidationTask>();
        match result {
            Ok(plan) => {
                commands.entity(entity).insert((
                    WriteRequest {
                        root: plan.root.clone(),
                        mutation: Mutation::Save {
                            expected: plan.request.revision,
                            project: ProjectSaved {
                                root: plan.root,
                                id: plan.id,
                                directory: plan.directory,
                                name: plan.request.name.trim().to_string(),
                                repository: plan.repository,
                                revision: 0,
                            },
                        },
                    },
                    PendingCommit {
                        webview: task.webview,
                        request_id: task.request_id.clone(),
                    },
                ));
            }
            Err(error) => {
                feedback.finish(task.webview, &task.request_id, Err(error.clone()), None);
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err(error),
                });
            }
        }
    }
}

fn finish_commits(
    mut completed: MessageReader<WriteCompleted>,
    pending: Query<&PendingCommit>,
    mut feedback: Feedback,
    mut commands: Commands,
) {
    for completed in completed.read() {
        let Ok(task) = pending.get(completed.requester) else {
            continue;
        };
        let saved = completed.result.as_ref().ok().cloned().flatten();
        feedback.finish(
            task.webview,
            &task.request_id,
            completed.result.clone().map(|_| None),
            saved,
        );
        commands
            .entity(completed.requester)
            .remove::<PendingCommit>();
        commands.trigger(OperationCompleted {
            entity: completed.requester,
            result: completed.result.clone().map(|_| ()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ProjectUiState, ProjectView};

    struct Form {
        app: App,
        view: Entity,
    }

    impl Form {
        fn at(root: &std::path::Path, url: &str) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, super::super::project::ProjectPagePlugin))
                .init_resource::<vmux_setting::AppSettings>()
                .init_resource::<vmux_session::CatalogSnapshot>()
                .insert_non_send(bevy_cef::prelude::Browsers::default())
                .add_observer(Self::deliver);
            app.world_mut().spawn(vmux_ecs::profile::ProjectsRoot(
                vmux_ecs::profile::ProjectsDirectory::ensure_at(root.to_path_buf()).unwrap(),
            ));
            let view = app
                .world_mut()
                .spawn((
                    vmux_ecs::PageMetadata {
                        url: url.to_string(),
                        ..Default::default()
                    },
                    bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                    bevy_cef::prelude::BinIpcNavigationRevision(1),
                    bevy_cef::prelude::BinIpcEpoch(1),
                ))
                .id();
            app.world_mut()
                .non_send_mut::<bevy_cef::prelude::Browsers>()
                .set_externally_hosted(view);
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: ProjectRoute::ROOT,
                owns_subtree: true,
                permissions: &["ProjectUiState", "ProjectFormUiState"],
            });
            Self { app, view }
        }

        fn deliver(event: On<bevy_cef::prelude::BinHostEmitEvent>) {
            event.mark_delivered();
        }

        fn snapshot(
            &mut self,
            root: &std::path::Path,
        ) -> super::super::project_persistence::Snapshot {
            let mut state =
                bevy::ecs::system::SystemState::<Registrations>::new(self.app.world_mut());
            state.get(self.app.world()).unwrap().snapshot(root)
        }

        fn resolve(&mut self, root: &std::path::Path, url: &str) -> std::path::PathBuf {
            let mut state =
                bevy::ecs::system::SystemState::<Registrations>::new(self.app.world_mut());
            state
                .get(self.app.world())
                .unwrap()
                .resolve(url, root)
                .unwrap()
                .unwrap()
        }

        fn submit(&mut self, project: Option<ProjectId>, source: ProjectSource) {
            self.app.world_mut().trigger(UiInput {
                webview: self.view,
                payload: ProjectSaveRequest {
                    request_id: "form".into(),
                    project,
                    revision: 0,
                    name: "Chosen name".into(),
                    source,
                },
            });
        }

        fn next_provision(&mut self) -> RepositoryProvisionRequest {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                let mut requests = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<RepositoryProvisionRequest>>();
                if let Some(request) = requests.drain().next() {
                    return request;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "provision request was not dispatched"
                );
                std::thread::yield_now();
            }
        }

        fn complete(&mut self, request: RepositoryProvisionRequest, result: Result<(), String>) {
            self.app
                .world_mut()
                .write_message(RepositoryProvisionCompleted {
                    requester: request.requester,
                    result,
                });
        }

        fn outcome(&mut self) -> ProjectFormUiState {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                if let Some(state) = self
                    .app
                    .world()
                    .get::<vmux_ecs::UiState<ProjectFormUiState>>(self.view)
                    .and_then(vmux_ecs::UiState::current)
                    && state.request_id == "form"
                    && !state.pending
                {
                    return state.clone();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "form result was not published"
                );
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn accepted_save_completes_after_its_webview_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let mut form = Form::at(root.path(), ProjectRoute::ROOT);
        form.submit(None, ProjectSource::Github("example/repository".into()));
        let request = form.next_provision();
        let operation = request.requester;
        let RepositoryProvisionOperation::Clone { destination, .. } = &request.operation else {
            panic!("expected clone");
        };
        std::fs::create_dir_all(destination.join(".git")).unwrap();
        form.app.world_mut().despawn(form.view);
        form.complete(request, Ok(()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            form.app.update();
            if form.app.world().get::<vmux_ecs::OperationState>(operation)
                == Some(&vmux_ecs::OperationState::Succeeded)
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "accepted save did not finish"
            );
            std::thread::yield_now();
        }
        let snapshot = form.snapshot(root.path());
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(
            snapshot.projects.values().next().unwrap().name,
            "Chosen name"
        );
        assert!(root.path().join(".projects.ron").is_file());
    }

    #[test]
    fn github_clone_registers_only_after_provisioning_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let mut form = Form::at(root.path(), ProjectRoute::ROOT);
        form.submit(None, ProjectSource::Github("example/repository".into()));
        let request = form.next_provision();
        let RepositoryProvisionOperation::Clone {
            remote,
            destination,
        } = &request.operation
        else {
            panic!("expected clone");
        };
        assert_eq!(remote, "https://github.com/example/repository.git");
        assert!(form.snapshot(root.path()).projects.is_empty());
        std::fs::create_dir_all(destination.join(".git")).unwrap();
        form.complete(request, Ok(()));
        let outcome = form.outcome();
        let id = outcome.saved.unwrap();
        let registry = form.snapshot(root.path());
        let record = registry.projects.get(&id.0).unwrap();
        assert_eq!(record.name.as_str(), "Chosen name");
        assert_eq!(
            record.repository.as_deref(),
            Some("https://github.com/example/repository.git")
        );
        assert_eq!(
            form.resolve(root.path(), &id.0),
            root.path()
                .join("github.com/example/repository")
                .canonicalize()
                .unwrap()
        );
    }

    #[test]
    fn failed_clone_keeps_the_previous_project_registration_and_form_error() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        std::fs::create_dir_all(old.join(".git")).unwrap();
        std::fs::write(old.join("work.txt"), "keep").unwrap();
        let id = ProjectId("vmux://projects/old".into());
        let mut form = Form::at(root.path(), ProjectRoute::ROOT);
        form.submit(
            Some(id.clone()),
            ProjectSource::Github("example/repository".into()),
        );
        let request = form.next_provision();
        form.complete(request, Err("clone refused".into()));
        let outcome = form.outcome();
        assert_eq!(outcome.error.as_deref(), Some("clone refused"));
        assert!(outcome.saved.is_none());
        assert!(!form.snapshot(root.path()).projects.contains_key(&id.0));
        assert_eq!(
            form.resolve(root.path(), &id.0),
            old.canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(old.join("work.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn editing_a_local_source_keeps_the_project_url_and_previous_files() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let next = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(old.join(".git")).unwrap();
        std::fs::create_dir_all(next.path().join(".git")).unwrap();
        std::fs::write(old.join("work.txt"), "keep").unwrap();
        let id = ProjectId("vmux://projects/old".into());
        let mut form = Form::at(root.path(), &id.0);
        form.app
            .world_mut()
            .entity_mut(form.view)
            .insert(SelectedFolder {
                request_id: "form".into(),
                directory: next.path().to_path_buf(),
            });
        form.submit(
            Some(id.clone()),
            ProjectSource::Local(next.path().to_string_lossy().into_owned()),
        );
        let outcome = form.outcome();
        assert_eq!(outcome.saved, Some(id.clone()));
        assert_eq!(
            form.resolve(root.path(), &id.0),
            next.path().canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(old.join("work.txt")).unwrap(),
            "keep"
        );
        form.app.update();
        let state = form
            .app
            .world()
            .get::<vmux_ecs::UiState<ProjectUiState>>(form.view)
            .unwrap()
            .current()
            .unwrap();
        let ProjectView::Detail(detail) = &state.view else {
            panic!("project detail must stay open");
        };
        assert_eq!(detail.id, id);
        assert_eq!(detail.name, "Chosen name");
        assert_eq!(
            PathBuf::from(&detail.path),
            next.path().canonicalize().unwrap()
        );
    }
}
