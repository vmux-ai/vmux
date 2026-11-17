use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarWorkspaceSelection, StartGoToBranch, StartSelectWorkspace,
};
use vmux_command::CommandBarWorkDirectory;
use vmux_ecs::launcher::{HostsLauncher, WorkspaceSelectionState};
use vmux_ecs::{
    Cwd, Operation, OperationState, OperationTarget, OperationTargetRevision, PageMetadata,
    UiStateWrite,
};
use vmux_layout::tab::{Tab, TabWorkspace};
use vmux_layout::worktree::{
    CancelCheckout, CheckoutFailed, CheckoutPlugin, CheckoutPrepared, CheckoutTargetRevision,
    CommitCheckout, ManagedWorktreeRoot, PreparedCheckout, SelectBranchCheckout,
};
use vmux_ui::i18n::translate;

pub(crate) struct WorkspacePlugin;

impl bevy::app::Plugin for WorkspacePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CheckoutPlugin>() {
            app.add_plugins(CheckoutPlugin);
        }
        app.init_resource::<ManagedWorktreeRoot>()
            .add_plugins(UiEventPlugin::<(StartSelectWorkspace, StartGoToBranch)>::default())
            .add_observer(select)
            .add_observer(pick)
            .add_observer(apply)
            .add_observer(project_selection)
            .add_observer(clear_selection)
            .add_systems(Update, (pin, release).in_set(Binding))
            .add_systems(
                Update,
                (retire, finish, commit, failed, finish_pickers)
                    .chain()
                    .before(Binding),
            );
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Binding;

#[derive(Component)]
struct Pinned;

#[derive(Component)]
struct Picker {
    webview: Entity,
    task: Task<Option<(PathBuf, bool)>>,
}

#[derive(Component)]
struct Selection {
    operation: Entity,
}

#[derive(Component)]
struct Applied;

#[derive(Component)]
struct LauncherCheckout;

#[derive(EntityEvent)]
struct Chosen {
    #[event_target]
    webview: Entity,
    path: PathBuf,
}

fn project_selection(
    event: On<Insert, WorkspaceSelectionState>,
    states: Query<&WorkspaceSelectionState>,
    mut commands: Commands,
) {
    let Ok(state) = states.get(event.entity) else {
        return;
    };
    let projection = match state {
        WorkspaceSelectionState::Ready => CommandBarWorkspaceSelection::default(),
        WorkspaceSelectionState::Pending => CommandBarWorkspaceSelection {
            pending: true,
            error: String::new(),
        },
        WorkspaceSelectionState::Failed(error) => CommandBarWorkspaceSelection {
            pending: false,
            error: error.clone(),
        },
    };
    commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
        event.entity,
        &projection,
    ));
}

fn clear_selection(event: On<Remove, WorkspaceSelectionState>, mut commands: Commands) {
    commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
        event.entity,
        &CommandBarWorkspaceSelection::default(),
    ));
}

fn pin(
    pages: Query<(Entity, &PageMetadata), (With<HostsLauncher>, Without<Pinned>)>,
    parents: Query<&ChildOf>,
    directories: Query<&Cwd>,
    work_directories: Query<&CommandBarWorkDirectory>,
    tabs: Query<(&Tab, Option<&TabWorkspace>)>,
    layout: vmux_layout::stack::LayoutFocus,
    mut commands: Commands,
) {
    for (webview, metadata) in &pages {
        if !metadata.url.starts_with(crate::StartPlugin::URL) {
            continue;
        }
        if layout.tab_of(webview).is_none() {
            continue;
        }
        let Ok(parent) = parents.get(webview) else {
            continue;
        };
        let mut current = parent.parent();
        let mut dir = PathBuf::new();
        for _ in 0..32 {
            if let Ok(cwd) = directories.get(current) {
                dir = cwd.0.clone();
                break;
            }
            if let Ok(cwd) = work_directories.get(current) {
                dir = PathBuf::from(&cwd.0);
                break;
            }
            if let Ok((tab, workspace)) = tabs.get(current) {
                dir = tab
                    .startup_dir
                    .as_deref()
                    .or_else(|| workspace.map(|workspace| workspace.project_dir.as_str()))
                    .map(PathBuf::from)
                    .unwrap_or_default();
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
        commands.entity(webview).insert((
            Pinned,
            CommandBarWorkDirectory(dir.to_string_lossy().into_owned()),
        ));
    }
}

fn release(
    pages: Query<(Entity, &PageMetadata, Option<&Selection>), With<Pinned>>,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    for (webview, metadata, selection) in &pages {
        if !metadata.url.starts_with(crate::StartPlugin::URL) {
            if let Some(selection) = selection {
                cancels.write(CancelCheckout {
                    operation: selection.operation,
                });
            }
            commands.entity(webview).remove::<(
                Pinned,
                CommandBarWorkDirectory,
                Selection,
                WorkspaceSelectionState,
                Applied,
            )>();
        }
    }
}

fn pick(
    trigger: On<UiInput<StartGoToBranch>>,
    pages: Query<(&PageMetadata, Option<&CheckoutTargetRevision>), With<HostsLauncher>>,
    selections: Query<&Selection>,
    root: Res<ManagedWorktreeRoot>,
    mut requests: MessageWriter<SelectBranchCheckout>,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !pages
        .get(webview)
        .is_ok_and(|(page, _)| page.url.starts_with(crate::StartPlugin::URL))
    {
        return;
    }
    let request = &trigger.event().payload;
    if request.checkout.trim().is_empty() && !request.branch.trim().is_empty() {
        if let Ok(selection) = selections.get(webview) {
            cancels.write(CancelCheckout {
                operation: selection.operation,
            });
        }
        let revision = pages
            .get(webview)
            .ok()
            .and_then(|(_, revision)| revision)
            .map_or(1, |revision| revision.0.wrapping_add(1).max(1));
        let operation = commands
            .spawn((
                Name::new("Launcher branch selection"),
                vmux_ecs::Active,
                LauncherCheckout,
                Operation::bundle("launcher.checkout").expect("fixed launcher checkout kind"),
                OperationTarget::new(webview),
                OperationTargetRevision(revision),
            ))
            .id();
        commands
            .entity(webview)
            .insert((
                Selection { operation },
                CheckoutTargetRevision(revision),
                WorkspaceSelectionState::Pending,
            ))
            .remove::<Applied>();
        requests.write(SelectBranchCheckout {
            operation,
            directory: PathBuf::from(request.project.trim()),
            managed_root: root.0.clone(),
            branch: request.branch.trim().to_string(),
        });
        return;
    }
    let path = if request.checkout.trim().is_empty() {
        &request.project
    } else {
        &request.checkout
    };
    commands.trigger(Chosen {
        webview,
        path: PathBuf::from(path),
    });
}

fn apply(
    trigger: On<Chosen>,
    pages: Query<&PageMetadata, With<HostsLauncher>>,
    selections: Query<&Selection>,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !pages
        .get(request.webview)
        .is_ok_and(|page| page.url.starts_with(crate::StartPlugin::URL))
    {
        return;
    }
    if let Ok(selection) = selections.get(request.webview) {
        cancels.write(CancelCheckout {
            operation: selection.operation,
        });
    }
    commands.entity(request.webview).remove::<Applied>();
    match request.path.canonicalize() {
        Ok(path) if path.is_dir() => {
            commands
                .entity(request.webview)
                .insert((
                    Pinned,
                    CommandBarWorkDirectory(path.to_string_lossy().into_owned()),
                ))
                .remove::<Selection>()
                .insert(WorkspaceSelectionState::Ready);
        }
        _ => {
            commands
                .entity(request.webview)
                .remove::<Selection>()
                .insert(WorkspaceSelectionState::Failed(translate(
                    "project-directory-unavailable",
                )));
        }
    }
}

fn retire(
    selections: Query<(Entity, &Selection, Option<&PageMetadata>)>,
    mut removed: RemovedComponents<Selection>,
    operations: Query<(Entity, Option<&OperationTarget>, &OperationState), With<LauncherCheckout>>,
    mut cancels: MessageWriter<CancelCheckout>,
    mut commands: Commands,
) {
    for (page, selection, metadata) in &selections {
        if metadata.is_some_and(|page| page.url.starts_with(crate::StartPlugin::URL)) {
            continue;
        }
        cancels.write(CancelCheckout {
            operation: selection.operation,
        });
        commands
            .entity(page)
            .remove::<(Selection, Applied, WorkspaceSelectionState)>();
    }
    for page in removed.read() {
        for (operation, target, state) in &operations {
            if target.is_some_and(|target| target.entity() == page)
                && !selections
                    .get(page)
                    .is_ok_and(|(_, selection, _)| selection.operation == operation)
                && matches!(state, OperationState::Pending | OperationState::Running)
            {
                cancels.write(CancelCheckout { operation });
            }
        }
    }
}

fn finish(
    mut prepared: MessageReader<CheckoutPrepared>,
    operations: Query<(
        &OperationTarget,
        &OperationTargetRevision,
        &PreparedCheckout,
    )>,
    selections: Query<&Selection>,
    pages: Query<(&PageMetadata, &CheckoutTargetRevision), With<HostsLauncher>>,
    mut commands: Commands,
) {
    for prepared in prepared.read() {
        let Ok((target, revision, checkout)) = operations.get(prepared.operation) else {
            continue;
        };
        let page = target.entity();
        if !selections
            .get(page)
            .is_ok_and(|selection| selection.operation == prepared.operation)
        {
            continue;
        }
        if !pages.get(page).is_ok_and(|(metadata, current)| {
            metadata.url.starts_with(crate::StartPlugin::URL) && current.0 == revision.0
        }) {
            continue;
        }
        commands.entity(page).insert((
            Pinned,
            Applied,
            CommandBarWorkDirectory(
                checkout
                    .activation
                    .execution_dir
                    .to_string_lossy()
                    .into_owned(),
            ),
        ));
    }
}

fn commit(
    selections: Query<(Entity, &Selection, &CheckoutTargetRevision), Added<Applied>>,
    mut commits: MessageWriter<CommitCheckout>,
) {
    for (page, selection, revision) in &selections {
        commits.write(CommitCheckout {
            operation: selection.operation,
            target: page,
            revision: revision.0,
        });
    }
}

fn failed(
    mut failures: MessageReader<CheckoutFailed>,
    states: Query<&OperationState>,
    selections: Query<(Entity, &Selection)>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        for (page, selection) in &selections {
            if selection.operation != failure.operation {
                continue;
            }
            commands
                .entity(page)
                .remove::<(Selection, Applied)>()
                .insert(WorkspaceSelectionState::Failed(failure.error.clone()));
        }
    }
    for (page, selection) in &selections {
        if states
            .get(selection.operation)
            .is_ok_and(|state| *state == OperationState::Succeeded)
        {
            commands
                .entity(page)
                .remove::<(Selection, Applied)>()
                .insert(WorkspaceSelectionState::Ready);
        } else if states
            .get(selection.operation)
            .is_ok_and(|state| *state == OperationState::Canceled)
        {
            commands
                .entity(page)
                .remove::<(Selection, Applied, WorkspaceSelectionState)>();
        }
    }
}

fn select(
    trigger: On<UiInput<StartSelectWorkspace>>,
    pages: Query<&PageMetadata, With<HostsLauncher>>,
    pending: Query<&Picker>,
    projects: vmux_ecs::profile::Projects,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if !pages
        .get(webview)
        .is_ok_and(|page| page.url.starts_with(crate::StartPlugin::URL))
        || pending.iter().any(|picker| picker.webview == webview)
    {
        return;
    }
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let initial_dir = projects
        .path()
        .ok()
        .map(Path::to_path_buf)
        .or_else(|| {
            PathBuf::from(&trigger.event().payload.current_dir)
                .canonicalize()
                .ok()
        })
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"));
    let task = IoTaskPool::get().spawn(async move {
        let selected = rfd::AsyncFileDialog::new()
            .set_title(translate("agent-project-choose"))
            .set_directory(initial_dir)
            .pick_folder()
            .await
            .map(|handle| handle.path().to_path_buf());
        let result = if let Some(path) = selected {
            let initialize_git = !path.join(".git").exists()
                && matches!(
                    rfd::AsyncMessageDialog::new()
                        .set_title(translate("space-initialize-git-question"))
                        .set_description(translate("project-import-not-repository"))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        .await,
                    rfd::MessageDialogResult::Yes
                );
            Some((path, initialize_git))
        } else {
            None
        };
        if let Some(wake) = wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        result
    });
    commands.spawn(Picker { webview, task });
}

fn finish_pickers(mut pending: Query<(Entity, &mut Picker)>, mut commands: Commands) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        if let Some((path, initialize_git)) = selected {
            if initialize_git {
                let _ = vmux_git::worktree::CheckoutInfo::initialize(&path);
            }
            commands.trigger(Chosen {
                webview: picker.webview,
                path,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Published(CommandBarWorkspaceSelection);

    impl Published {
        fn record(trigger: On<UiStateWrite<CommandBarUiState>>, mut published: ResMut<Self>) {
            if let Some(selection) = &trigger.event().update().workspace_selection {
                published.0 = selection.clone();
            }
        }
    }

    struct Composers {
        app: App,
        tab: Entity,
        first: Entity,
        second: Entity,
        original: tempfile::TempDir,
        other: tempfile::TempDir,
    }

    impl Composers {
        fn new() -> Self {
            let original = tempfile::tempdir().unwrap();
            let other = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, WorkspacePlugin))
                .insert_resource(ManagedWorktreeRoot(other.path().join("worktrees")))
                .init_resource::<Published>()
                .add_observer(Published::record);
            let tab = app
                .world_mut()
                .spawn(Tab {
                    startup_dir: Some(original.path().to_string_lossy().into_owned()),
                    ..Default::default()
                })
                .id();
            let first = app
                .world_mut()
                .spawn((
                    HostsLauncher,
                    ChildOf(tab),
                    PageMetadata {
                        url: crate::StartPlugin::URL.into(),
                        ..Default::default()
                    },
                ))
                .id();
            let second = app
                .world_mut()
                .spawn((
                    HostsLauncher,
                    ChildOf(tab),
                    PageMetadata {
                        url: crate::StartPlugin::URL.into(),
                        ..Default::default()
                    },
                ))
                .id();
            app.update();
            Self {
                app,
                tab,
                first,
                second,
                original,
                other,
            }
        }

        fn choose(&mut self, page: Entity, path: &Path) {
            self.app.world_mut().trigger(UiInput {
                webview: page,
                payload: StartGoToBranch {
                    project: path.to_string_lossy().into_owned(),
                    ..Default::default()
                },
            });
            self.app.update();
        }

        fn cwd(&self, page: Entity) -> &Path {
            Path::new(
                &self
                    .app
                    .world()
                    .get::<CommandBarWorkDirectory>(page)
                    .unwrap()
                    .0,
            )
        }

        fn git(&self, args: &[&str]) {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(self.original.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn settle(&mut self, page: Entity) {
            self.app.update();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.app.world().get::<Selection>(page).is_some() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "branch selection did not finish"
                );
                self.app.update();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn failed_workspace_selection_blocks_only_its_page_until_a_valid_choice() {
        let mut composers = Composers::new();
        let original = composers.cwd(composers.first).to_path_buf();
        let missing = composers.other.path().join("missing");
        composers.choose(composers.first, &missing);
        assert_eq!(composers.cwd(composers.first), original);
        assert!(
            composers
                .app
                .world()
                .get::<WorkspaceSelectionState>(composers.first)
                .unwrap()
                .blocks()
        );
        assert!(
            composers
                .app
                .world()
                .get::<WorkspaceSelectionState>(composers.second)
                .is_none()
        );
        assert!(
            !composers
                .app
                .world()
                .resource::<Published>()
                .0
                .error
                .is_empty()
        );
        let other = composers.other.path().to_path_buf();
        composers.choose(composers.first, &other);
        assert!(
            !composers
                .app
                .world()
                .get::<WorkspaceSelectionState>(composers.first)
                .unwrap()
                .blocks()
        );
        assert_eq!(
            composers.cwd(composers.first),
            other.canonicalize().unwrap()
        );
        assert!(
            composers
                .app
                .world()
                .resource::<Published>()
                .0
                .error
                .is_empty()
        );
    }

    #[test]
    fn choosing_a_project_changes_only_that_composer_and_preserves_the_tab_default() {
        let mut composers = Composers::new();
        let other = composers.other.path().to_path_buf();
        composers.choose(composers.first, &other);
        assert_eq!(
            composers.cwd(composers.first),
            other.canonicalize().unwrap()
        );
        assert_eq!(composers.cwd(composers.second), composers.original.path());
        assert_eq!(
            composers
                .app
                .world()
                .get::<Tab>(composers.tab)
                .unwrap()
                .startup_dir,
            Some(composers.original.path().to_string_lossy().into_owned())
        );
        composers
            .app
            .world_mut()
            .get_mut::<Tab>(composers.tab)
            .unwrap()
            .startup_dir = Some(other.to_string_lossy().into_owned());
        composers.app.update();
        assert_eq!(composers.cwd(composers.second), composers.original.path());
    }

    #[test]
    fn leaving_start_releases_the_pin_and_a_new_composer_inherits_its_panes_directory() {
        let mut composers = Composers::new();
        composers
            .app
            .world_mut()
            .get_mut::<PageMetadata>(composers.first)
            .unwrap()
            .url = "vmux://sessions/existing".into();
        composers.app.update();
        assert!(
            composers
                .app
                .world()
                .get::<CommandBarWorkDirectory>(composers.first)
                .is_none()
        );
        let other = composers.other.path().to_path_buf();
        composers
            .app
            .world_mut()
            .entity_mut(composers.tab)
            .insert(CommandBarWorkDirectory(
                other.to_string_lossy().into_owned(),
            ));
        composers
            .app
            .world_mut()
            .get_mut::<PageMetadata>(composers.first)
            .unwrap()
            .url = crate::StartPlugin::URL.into();
        composers.app.update();
        assert_eq!(composers.cwd(composers.first), other);
        assert_eq!(composers.cwd(composers.second), composers.original.path());
    }

    #[test]
    fn a_pending_worktree_lease_suspends_the_launcher_io_task_until_ready() {
        let mut composers = Composers::new();
        composers.git(&["init", "-q", "-b", "main"]);
        composers.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-qm",
            "Initial",
        ]);
        composers.git(&["branch", "held"]);
        let root = composers
            .app
            .world()
            .resource::<ManagedWorktreeRoot>()
            .0
            .clone();
        std::fs::create_dir_all(&root).unwrap();
        let root = root.join("..").join("worktrees");
        composers
            .app
            .insert_resource(ManagedWorktreeRoot(root.clone()));
        let target = composers.app.world_mut().spawn_empty().id();
        let held = composers
            .app
            .world_mut()
            .spawn((
                Name::new("Held branch checkout"),
                Operation::bundle("held.checkout").unwrap(),
                OperationTarget::new(target),
                OperationTargetRevision(1),
            ))
            .id();
        composers
            .app
            .world_mut()
            .write_message(SelectBranchCheckout {
                operation: held,
                directory: composers.original.path().to_path_buf(),
                managed_root: root.clone(),
                branch: "held".into(),
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while composers
            .app
            .world()
            .get::<PreparedCheckout>(held)
            .is_none()
        {
            composers.app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "held checkout was not prepared"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let repository =
            vmux_git::worktree::CheckoutInfo::try_from(composers.original.path()).unwrap();
        let checkout = repository
            .worktree_registrations()
            .unwrap()
            .into_iter()
            .find(|registration| registration.branch.as_deref() == Some("held"))
            .unwrap()
            .path;
        let checkout_path = checkout.to_str().unwrap();
        composers.git(&["worktree", "remove", "--force", checkout_path]);
        composers.app.world_mut().trigger(UiInput {
            webview: composers.first,
            payload: StartGoToBranch {
                project: composers.original.path().to_string_lossy().into_owned(),
                branch: "held".into(),
                checkout: String::new(),
            },
        });
        for _ in 0..30 {
            composers.app.update();
            assert!(
                composers
                    .app
                    .world()
                    .get::<WorkspaceSelectionState>(composers.first)
                    .is_some_and(WorkspaceSelectionState::blocks)
            );
            assert_eq!(composers.cwd(composers.first), composers.original.path());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        composers
            .app
            .world_mut()
            .write_message(CancelCheckout { operation: held });
        composers.settle(composers.first);
        assert_eq!(composers.cwd(composers.first), checkout);
        assert_eq!(composers.cwd(composers.second), composers.original.path());
    }

    #[test]
    fn an_unheld_branch_prepares_a_checkout_without_changing_the_other_composer() {
        let mut composers = Composers::new();
        composers.git(&["init", "-q", "-b", "main"]);
        composers.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-qm",
            "Initial",
        ]);
        composers.git(&["branch", "feature/selected"]);
        composers.app.world_mut().trigger(UiInput {
            webview: composers.first,
            payload: StartGoToBranch {
                project: composers.original.path().to_string_lossy().into_owned(),
                branch: "feature/selected".into(),
                checkout: String::new(),
            },
        });
        composers.settle(composers.first);
        let published = &composers.app.world().resource::<Published>().0;
        assert!(!published.pending);
        assert!(published.error.is_empty(), "{}", published.error);
        let selected =
            vmux_git::worktree::CheckoutInfo::try_from(composers.cwd(composers.first)).unwrap();
        assert_eq!(selected.head_ref().unwrap(), "feature/selected");
        let original =
            vmux_git::worktree::CheckoutInfo::try_from(composers.original.path()).unwrap();
        assert_eq!(selected.common_dir, original.common_dir);
        assert_eq!(original.head_ref().unwrap(), "main");
        assert_eq!(composers.cwd(composers.second), composers.original.path());
    }

    #[test]
    fn composer_directories_never_make_transient_pages_persistent() {
        let mut composers = Composers::new();
        let selected = composers.other.path().to_path_buf();
        composers.choose(composers.first, &selected);
        for page in [composers.first, composers.second] {
            assert!(
                composers
                    .app
                    .world()
                    .get::<CommandBarWorkDirectory>(page)
                    .is_some()
            );
            assert!(
                !composers
                    .app
                    .world()
                    .entity(page)
                    .contains::<moonshine_save::prelude::Save>()
            );
            assert!(composers.app.world().get::<Cwd>(page).is_none());
        }
    }

    #[test]
    fn prewarmed_pages_bind_only_after_they_are_attached_to_a_tab() {
        let mut composers = Composers::new();
        let pool = composers.app.world_mut().spawn_empty().id();
        let page = composers
            .app
            .world_mut()
            .spawn((
                HostsLauncher,
                ChildOf(pool),
                PageMetadata {
                    url: crate::StartPlugin::URL.into(),
                    ..Default::default()
                },
            ))
            .id();
        composers.app.update();
        assert!(
            composers
                .app
                .world()
                .get::<CommandBarWorkDirectory>(page)
                .is_none()
        );
        assert!(
            !composers
                .app
                .world()
                .entity(page)
                .contains::<moonshine_save::prelude::Save>()
        );
        composers
            .app
            .world_mut()
            .entity_mut(page)
            .insert(ChildOf(composers.tab));
        composers.app.update();
        assert_eq!(composers.cwd(page), composers.original.path());
        assert!(
            !composers
                .app
                .world()
                .entity(page)
                .contains::<moonshine_save::prelude::Save>()
        );
    }
}
