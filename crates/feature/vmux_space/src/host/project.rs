use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::UiInput;
use vmux_command::{CommandBarProjectRoots, WriteCommandBarSnapshots};
use vmux_ecs::event::space::{ProjectActivateRequest, ProjectPickerRequest};
use vmux_ecs::event::{ProjectRow, ProjectRowKind, ProjectTreeToggle};
use vmux_ecs::page::PageReady;
use vmux_ecs::persistence::PersistenceAppExt;
use vmux_ecs::{PageMetadata, UiStateWrite};
use vmux_git::worktree::{CheckoutInfo, LinkedRepoRoot};
#[cfg(test)]
use vmux_layout::pane::Pane;
use vmux_layout::space::{
    EffectiveStartupSet, FocusedSpace, Space, SpaceHierarchy, SpaceId, SpaceOfPane,
};
use vmux_layout::tab::{Tab, TabWorkspace, TabWorktree};
#[cfg(test)]
use vmux_setting::SpaceOverrides;
use vmux_setting::{AppSettings, SettingsSaveRequest, SpaceProject};

pub struct SpaceProjectPlugin;

impl Plugin for SpaceProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_repository_roots)
            .init_resource::<vmux_git::GitRuntime>()
            .init_resource::<ActiveProjectRows>()
            .init_resource::<ActiveProjectScan>()
            .register_persisted::<ExpandedProjectDirs>()
            .add_observer(tree_toggle)
            .add_observer(pick)
            .add_systems(
                Update,
                (
                    remember.before(EffectiveStartupSet),
                    start_repo_root_resolutions.after(remember),
                    finish_repo_root_resolutions.after(start_repo_root_resolutions),
                    refresh_active_projects.after(remember),
                    publish_roots
                        .in_set(WriteCommandBarSnapshots)
                        .after(refresh_active_projects),
                    publish_git_projects.after(refresh_active_projects),
                    finish_picks,
                ),
            );
    }
}

fn spawn_repository_roots(mut commands: Commands) {
    commands.spawn((
        Name::new("Repository roots"),
        RepoRoots::default(),
        CommandBarProjectRoots::default(),
    ));
}

fn tree_toggle(
    trigger: On<UiInput<ProjectTreeToggle>>,
    space_of_pane: SpaceOfPane,
    mut expanded: Query<&mut ExpandedProjectDirs>,
    mut commands: Commands,
) {
    let Some(space) = space_of_pane.resolve(&trigger.event().payload.pane_id) else {
        return;
    };
    let path = &trigger.event().payload.path;
    if let Ok(mut dirs) = expanded.get_mut(space) {
        dirs.toggle(path);
        return;
    }
    let mut dirs = ExpandedProjectDirs::default();
    dirs.toggle(path);
    commands.entity(space).insert(dirs);
}

#[derive(Component)]
struct PendingProjectPicker {
    webview: Entity,
    pane_id: Option<u64>,
    task: Task<Option<std::path::PathBuf>>,
}

struct ProjectPicker;

impl ProjectPicker {
    fn initial_directory(path: &str) -> std::path::PathBuf {
        let mut current = std::path::PathBuf::from(path.trim());
        while !current.is_dir() && current.pop() {}
        if current.is_dir() {
            return current;
        }
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .filter(|home| home.is_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
    }
}

fn pick(
    trigger: On<UiInput<ProjectPickerRequest>>,
    pending: Query<&PendingProjectPicker>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    if pending.iter().any(|picker| picker.webview == event.webview) {
        return;
    }
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let current_dir = event.payload.current_dir.clone();
    let task = IoTaskPool::get().spawn(async move {
        let initial = ProjectPicker::initial_directory(&current_dir);
        let selected = rfd::AsyncFileDialog::new()
            .set_title(vmux_ui::i18n::translate("agent-project-choose"))
            .set_directory(initial)
            .pick_folder()
            .await
            .map(|folder| folder.path().to_path_buf())
            .and_then(|path| path.canonicalize().ok())
            .filter(|path| path.is_dir());
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        selected
    });
    commands.spawn(PendingProjectPicker {
        webview: event.webview,
        pane_id: event.payload.pane_id,
        task,
    });
}

fn finish_picks(mut pending: Query<(Entity, &mut PendingProjectPicker)>, mut commands: Commands) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        if let Some(path) = selected {
            commands.trigger(UiInput {
                webview: picker.webview,
                payload: ProjectActivateRequest {
                    path: path.to_string_lossy().into_owned(),
                    branch: String::new(),
                    checkout: String::new(),
                    pane_id: picker.pane_id,
                },
            });
        }
        commands.entity(entity).despawn();
    }
}

#[derive(Resource, Default)]
pub struct ActiveProjectRows {
    space: Option<String>,
    rows: Vec<ProjectRow>,
    repositories: Vec<ProjectRow>,
}

impl ActiveProjectRows {
    fn selector_rows(&self, space: Option<&str>, source: &[ProjectRow]) -> Vec<ProjectRow> {
        if self.space.as_deref() == space {
            return self.rows.clone();
        }
        source
            .iter()
            .find(|row| row.depth == 0 && row.is_active)
            .cloned()
            .into_iter()
            .collect()
    }

    pub fn layout_rows(&self, space: Option<&str>, source: &[ProjectRow]) -> Vec<ProjectRow> {
        if self.space.as_deref() != space || self.rows.is_empty() {
            return Self::active_source_rows(source);
        }
        let mut rows = Vec::new();
        for root in &self.rows {
            rows.push(root.clone());
            if !root.is_active {
                continue;
            }
            let root_path = vmux_path::PathIdentity::resolve(&root.path);
            let Some(index) = source.iter().position(|row| {
                row.depth == 0 && vmux_path::PathIdentity::resolve(&row.path) == root_path
            }) else {
                continue;
            };
            rows.extend(
                source
                    .iter()
                    .skip(index + 1)
                    .take_while(|row| row.depth > 0)
                    .cloned(),
            );
        }
        rows
    }

    fn active_source_rows(source: &[ProjectRow]) -> Vec<ProjectRow> {
        let Some(index) = source
            .iter()
            .position(|row| row.depth == 0 && row.is_active)
        else {
            return Vec::new();
        };
        source
            .iter()
            .skip(index)
            .take_while(|row| row.depth > 0 || row.is_active)
            .cloned()
            .collect()
    }
}

type ActiveProjectScanResult = (Option<String>, ProjectCatalog);

struct ProjectCatalog {
    rows: Vec<ProjectRow>,
    repositories: Vec<ProjectRow>,
}

impl ProjectCatalog {
    fn from_rows(rows: &[ProjectRow]) -> Self {
        let mut candidates = Vec::new();
        for row in rows {
            if row.depth != 0
                || row.kind != ProjectRowKind::Project
                || row.missing
                || std::path::Path::new(&row.path)
                    .components()
                    .any(|component| component.as_os_str() == std::ffi::OsStr::new(".git"))
            {
                continue;
            }
            let Ok(checkout) = CheckoutInfo::try_from(std::path::Path::new(&row.path)) else {
                candidates.push((None, row.clone()));
                continue;
            };
            let identity =
                LinkedRepoRoot::find(&checkout.root).unwrap_or_else(|| checkout.root.clone());
            let identity = identity.canonicalize().unwrap_or(identity);
            let mut projected = row.clone();
            projected.path = checkout.root.to_string_lossy().into_owned();
            projected.display_path = vmux_path::DisplayPath::project(&identity);
            projected.label = identity
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| projected.label.clone());
            projected.is_worktree = checkout.root != identity;
            candidates.push((Some(identity), projected));
        }
        let mut projected = Vec::<(Option<std::path::PathBuf>, ProjectRow)>::new();
        for (identity, row) in &candidates {
            if identity.as_ref().is_some_and(|identity| {
                candidates.iter().any(|(other, _)| {
                    other.as_ref().is_some_and(|other| {
                        identity != other && std::path::Path::new(&row.path).starts_with(other)
                    })
                })
            }) {
                continue;
            }
            if let Some((_, held)) =
                projected
                    .iter_mut()
                    .find(|(held, held_row)| match (held, identity) {
                        (Some(held), Some(identity)) => held == identity,
                        (None, None) => held_row.path == row.path,
                        _ => false,
                    })
            {
                if row.is_active && !held.is_active {
                    held.clone_from(row);
                }
                continue;
            }
            projected.push((identity.clone(), row.clone()));
        }
        let mut rows = Vec::new();
        let mut repositories = Vec::new();
        for (identity, row) in projected {
            if identity.is_some() {
                repositories.push(row.clone());
            }
            rows.push(row);
        }
        Self { rows, repositories }
    }
}

#[derive(Resource, Default)]
struct ActiveProjectScan {
    task: Option<vmux_git::GitTask<ActiveProjectScanResult>>,
    requested: bool,
}

fn refresh_active_projects(
    projects: SpaceProjects,
    mut active: ResMut<ActiveProjectRows>,
    mut scan: ResMut<ActiveProjectScan>,
    runtime: Res<vmux_git::GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
) {
    let space = projects.active_space.id();
    if let Some(task) = scan.task.as_mut()
        && let Some(output) = task.poll()
    {
        scan.task = None;
        let Ok((scanned_space, catalog)) = output else {
            scan.requested = true;
            return;
        };
        if scanned_space.as_deref() == space
            && (active.space != scanned_space
                || active.rows != catalog.rows
                || active.repositories != catalog.repositories)
        {
            active.space = scanned_space;
            active.rows = catalog.rows;
            active.repositories = catalog.repositories;
        }
    }
    scan.requested |= active.space.as_deref() != space || projects.settings_changed();
    if scan.task.is_some() || !scan.requested {
        return;
    }
    scan.requested = false;
    let space = space.map(str::to_string);
    let settings = projects.settings.as_deref().cloned();
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    let Some(task) = runtime.spawn(wake, move || {
        let catalog = match (space.as_deref(), settings.as_ref()) {
            (Some(space), Some(settings)) => {
                ProjectCatalog::from_rows(&settings.project_rows(space))
            }
            _ => ProjectCatalog {
                rows: Vec::new(),
                repositories: Vec::new(),
            },
        };
        (space, catalog)
    }) else {
        scan.requested = true;
        return;
    };
    scan.task = Some(task);
}

fn publish_roots(projects: Res<ActiveProjectRows>, mut roots: Single<&mut CommandBarProjectRoots>) {
    if !projects.is_changed() {
        return;
    }
    let mut next = Vec::new();
    let mut active = None;
    for project in &projects.rows {
        if project.missing {
            continue;
        }
        if project.is_active {
            active = Some(project.path.clone());
        }
        next.push(project.path.clone());
    }
    let next = CommandBarProjectRoots {
        roots: next,
        active,
    };
    if **roots != next {
        **roots = next;
    }
}

#[derive(Component, Default)]
struct GitProjectsProjection(Vec<ProjectRow>);

impl GitProjectsProjection {
    fn from_rows(rows: &[ProjectRow]) -> Self {
        Self(
            rows.iter()
                .filter(|row| {
                    row.depth == 0
                        && row.kind == ProjectRowKind::Project
                        && !row.missing
                        && !std::path::Path::new(&row.path)
                            .components()
                            .any(|component| component.as_os_str() == std::ffi::OsStr::new(".git"))
                })
                .cloned()
                .collect(),
        )
    }
}

fn publish_git_projects(
    projects: Res<ActiveProjectRows>,
    views: Query<(Entity, &PageMetadata, Option<&GitProjectsProjection>), With<PageReady>>,
    mut commands: Commands,
) {
    let changed = projects.is_changed();
    let mut projection = None;
    for (entity, page, current) in &views {
        if !page.url.starts_with(vmux_git::GIT_PAGE_URL)
            && page.url != vmux_git::GitPlugin::MANIFEST.url
        {
            continue;
        }
        if !changed && current.is_some() {
            continue;
        }
        let rows = projection
            .get_or_insert_with(|| GitProjectsProjection::from_rows(&projects.repositories));
        if current.is_some_and(|current| current.0.as_slice() == rows.0.as_slice()) {
            continue;
        }
        commands
            .entity(entity)
            .insert(GitProjectsProjection(rows.0.clone()));
        commands.trigger(UiStateWrite::<vmux_git::state::GitUiState>::from_event(
            entity,
            &vmux_git::state::GitProjects {
                rows: rows.0.clone(),
            },
        ));
    }
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::space::project"]
#[require(moonshine_save::prelude::Save)]
pub struct ExpandedProjectDirs(Vec<String>);

impl ExpandedProjectDirs {
    fn toggle(&mut self, path: &str) {
        if let Some(index) = self.0.iter().position(|held| held == path) {
            self.0.remove(index);
            return;
        }
        self.0.push(path.to_string());
    }

    fn holds(&self, path: &str) -> bool {
        self.0.iter().any(|held| held == path)
    }

    fn children_of(&self, dir: &std::path::Path, depth: u32) -> Vec<ProjectRow> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if UNLISTED_DIRS.contains(&name.as_str()) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path().to_string_lossy().into_owned();
            rows.push(ProjectRow {
                label: name,
                display_path: path.clone(),
                depth,
                kind: match kind.is_dir() {
                    true => ProjectRowKind::Directory,
                    false => ProjectRowKind::File,
                },
                expanded: kind.is_dir() && self.holds(&path),
                path,
                is_active: false,
                is_worktree: false,
                missing: false,
                branch: String::new(),
            });
        }
        rows.sort_by(|a, b| {
            let b_opens = matches!(b.kind, ProjectRowKind::Project | ProjectRowKind::Directory);
            let a_opens = matches!(a.kind, ProjectRowKind::Project | ProjectRowKind::Directory);
            let folder_first = b_opens.cmp(&a_opens);
            folder_first.then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });
        let mut out = Vec::new();
        for row in rows {
            let descend = row.expanded;
            let path = row.path.clone();
            out.push(row);
            if descend {
                out.extend(self.children_of(std::path::Path::new(&path), depth + 1));
            }
        }
        out
    }
}

const UNLISTED_DIRS: &[&str] = &[
    ".git",
    "DerivedData",
    "Pods",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "target",
    "vendor",
    "venv",
];

#[derive(bevy::ecs::system::SystemParam)]
pub struct SpaceProjects<'w, 's> {
    settings: Option<Res<'w, AppSettings>>,
    active_space: FocusedSpace<'w, 's>,
    hierarchy: SpaceHierarchy<'w, 's>,
    space_ids: Query<'w, 's, &'static SpaceId>,
    expanded: Query<'w, 's, &'static ExpandedProjectDirs, With<Space>>,
}

impl SpaceProjects<'_, '_> {
    fn settings_changed(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.is_changed())
    }

    pub fn rows(&self, entity: Entity) -> Vec<ProjectRow> {
        let Some(space) = self.hierarchy.get(entity) else {
            return self.active_rows();
        };
        let Ok(space_id) = self.space_ids.get(space) else {
            return self.active_rows();
        };
        self.rows_of(&space_id.0, Some(space))
    }

    pub fn active_rows(&self) -> Vec<ProjectRow> {
        let Some(space) = self.active_space.entity() else {
            return Vec::new();
        };
        let Some(space_id) = self.active_space.id() else {
            return Vec::new();
        };
        self.rows_of(space_id, Some(space))
    }

    pub fn active_space_id(&self) -> Option<&str> {
        self.active_space.id()
    }

    pub fn active_projects(&self) -> Vec<ProjectRow> {
        let Some(space_id) = self.active_space.id() else {
            return Vec::new();
        };
        self.projects_of(space_id)
    }

    pub fn selector_rows(&self, entity: Entity, catalog: &ActiveProjectRows) -> Vec<ProjectRow> {
        let Some(space) = self.hierarchy.get(entity) else {
            let source = self.active_projects();
            return catalog.selector_rows(self.active_space.id(), &source);
        };
        let Ok(space_id) = self.space_ids.get(space) else {
            let source = self.active_projects();
            return catalog.selector_rows(self.active_space.id(), &source);
        };
        let source = self.projects_of(&space_id.0);
        catalog.selector_rows(Some(&space_id.0), &source)
    }

    fn projects_of(&self, space_id: &str) -> Vec<ProjectRow> {
        let Some(settings) = self.settings.as_deref() else {
            return Vec::new();
        };
        settings.project_rows(space_id)
    }

    fn rows_of(&self, space_id: &str, space: Option<Entity>) -> Vec<ProjectRow> {
        let listed = self.projects_of(space_id);
        let fallback = ExpandedProjectDirs::default();
        let expanded = space
            .and_then(|space| self.expanded.get(space).ok())
            .unwrap_or(&fallback);
        let mut rows = Vec::new();
        for mut project in listed {
            let open = !project.missing && (project.is_active || expanded.holds(&project.path));
            project.expanded = open;
            let path = project.path.clone();
            let depth = project.depth;
            rows.push(project);
            if open {
                rows.extend(expanded.children_of(std::path::Path::new(&path), depth + 1));
            }
        }
        rows
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpaceOfTab<'w, 's> {
    hierarchy: SpaceHierarchy<'w, 's>,
}

impl SpaceOfTab<'_, '_> {
    fn find(&self, tab: Entity) -> Option<String> {
        self.hierarchy.id(tab)
    }
}

#[derive(Component, Default)]
struct RepoRoots(std::collections::HashMap<String, Option<String>>);

impl RepoRoots {
    fn cached(&self, dir: &str) -> Option<Option<String>> {
        self.0.get(dir).cloned()
    }

    fn remember(&mut self, dir: String, root: Option<String>) {
        self.0.insert(dir, root);
    }

    fn read(dir: &str) -> Option<String> {
        let root = LinkedRepoRoot::find(std::path::Path::new(dir))?;
        let root = root.to_string_lossy().into_owned();
        (root != dir && !root.is_empty()).then_some(root)
    }
}

#[derive(Component, Clone)]
struct PendingRepoRoot {
    space_id: String,
    dir: String,
}

#[derive(Component)]
struct RepoRootTask(vmux_git::GitTask<Option<String>>);

fn remember(
    bound: Query<(
        Entity,
        Ref<TabWorkspace>,
        Option<Ref<TabWorktree>>,
        Option<&Tab>,
    )>,
    space_of_tab: SpaceOfTab,
    roots: Single<&RepoRoots>,
    settings: Option<ResMut<AppSettings>>,
    mut saves: MessageWriter<SettingsSaveRequest>,
    mut commands: Commands,
) {
    if bound.is_empty() {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    for (tab_entity, workspace, worktree, tab) in &bound {
        if !workspace.is_changed()
            && !worktree
                .as_ref()
                .is_some_and(|worktree| worktree.is_changed())
        {
            continue;
        }
        let dir = workspace.project_dir.trim();
        if dir.is_empty() {
            continue;
        }
        let Some(space_id) = space_of_tab.find(tab_entity) else {
            continue;
        };
        let project = match worktree.as_ref() {
            Some(worktree) if !worktree.repo_root.trim().is_empty() => {
                commands
                    .entity(tab_entity)
                    .remove::<(PendingRepoRoot, RepoRootTask)>();
                let checkout = tab
                    .and_then(|tab| tab.startup_dir.as_deref())
                    .filter(|path| !path.is_empty())
                    .unwrap_or_else(|| {
                        if worktree.checkout_dir.is_empty() {
                            dir
                        } else {
                            &worktree.checkout_dir
                        }
                    });
                SpaceProject::checked_out(&worktree.repo_root, checkout)
            }
            _ => {
                let Some(root) = roots.cached(dir) else {
                    commands
                        .entity(tab_entity)
                        .remove::<RepoRootTask>()
                        .insert(PendingRepoRoot {
                            space_id,
                            dir: dir.to_string(),
                        });
                    continue;
                };
                match root {
                    Some(root) => SpaceProject::checked_out(&root, dir),
                    None => SpaceProject::at(dir),
                }
            }
        };
        if settings
            .bypass_change_detection()
            .remember_space_project(&space_id, project)
        {
            saves.write(SettingsSaveRequest);
            settings.set_changed();
        }
    }
}

fn start_repo_root_resolutions(
    pending: Query<(Entity, &PendingRepoRoot), Without<RepoRootTask>>,
    runtime: Res<vmux_git::GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, pending) in &pending {
        let dir = pending.dir.clone();
        let Some(task) = runtime.spawn(wake.clone(), move || RepoRoots::read(&dir)) else {
            break;
        };
        commands.entity(entity).insert(RepoRootTask(task));
    }
}

fn finish_repo_root_resolutions(
    mut tasks: Query<(Entity, &PendingRepoRoot, &mut RepoRootTask)>,
    workspaces: Query<&TabWorkspace>,
    worktrees: Query<(), With<TabWorktree>>,
    space_of_tab: SpaceOfTab,
    mut roots: Single<&mut RepoRoots>,
    settings: Option<ResMut<AppSettings>>,
    mut saves: MessageWriter<SettingsSaveRequest>,
    mut commands: Commands,
) {
    let mut settings = settings;
    for (entity, pending, mut task) in &mut tasks {
        let Some(output) = task.0.poll() else {
            continue;
        };
        let Ok(root) = output else {
            commands.entity(entity).remove::<RepoRootTask>();
            continue;
        };
        roots.remember(pending.dir.clone(), root.clone());
        let current = workspaces
            .get(entity)
            .is_ok_and(|workspace| workspace.project_dir.trim() == pending.dir)
            && !worktrees.contains(entity)
            && space_of_tab.find(entity).as_deref() == Some(pending.space_id.as_str());
        if current && let Some(settings) = settings.as_mut() {
            let project = match root {
                Some(root) => SpaceProject::checked_out(&root, &pending.dir),
                None => SpaceProject::at(&pending.dir),
            };
            if settings
                .bypass_change_detection()
                .remember_space_project(&pending.space_id, project)
            {
                saves.write(SettingsSaveRequest);
                settings.set_changed();
            }
        }
        commands
            .entity(entity)
            .remove::<(PendingRepoRoot, RepoRootTask)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Resource, Default)]
    struct SawSettingsChange(bool);

    fn record_settings_change(settings: Res<AppSettings>, mut saw: ResMut<SawSettingsChange>) {
        saw.0 = settings.is_changed();
    }

    struct Fixture {
        app: App,
        tab: Entity,
        space: Entity,
        pane: Entity,
    }

    impl Fixture {
        fn start(space_id: &str) -> Self {
            let mut app = App::new();
            app.add_plugins(SpaceProjectPlugin)
                .add_message::<SettingsSaveRequest>()
                .init_resource::<SawSettingsChange>()
                .add_systems(
                    Update,
                    record_settings_change
                        .after(remember)
                        .after(finish_repo_root_resolutions),
                )
                .insert_resource(AppSettings::default());
            let space = app
                .world_mut()
                .spawn((Space, SpaceId(space_id.to_string())))
                .id();
            let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
            let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
            app.update();
            Self {
                app,
                tab,
                space,
                pane,
            }
        }

        fn toggle(&mut self, path: &str, pane_id: String) {
            self.app.world_mut().trigger(UiInput::<ProjectTreeToggle> {
                webview: Entity::PLACEHOLDER,
                payload: ProjectTreeToggle {
                    path: path.to_string(),
                    pane_id,
                },
            });
            self.app.update();
        }

        fn open_dirs(&self) -> Vec<String> {
            self.app
                .world()
                .get::<ExpandedProjectDirs>(self.space)
                .map(|dirs| dirs.0.clone())
                .unwrap_or_default()
        }

        fn select(&mut self, project_dir: &str) {
            self.app
                .world_mut()
                .entity_mut(self.tab)
                .insert(TabWorkspace {
                    project_dir: project_dir.to_string(),
                });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                self.app.update();
                if self.app.world().get::<PendingRepoRoot>(self.tab).is_none()
                    && self.app.world().get::<RepoRootTask>(self.tab).is_none()
                {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }

        fn select_worktree(&mut self, project_dir: &str, repo_root: &str) {
            self.app
                .world_mut()
                .get_mut::<Tab>(self.tab)
                .unwrap()
                .startup_dir = Some(project_dir.to_string());
            self.app.world_mut().entity_mut(self.tab).insert((
                TabWorkspace {
                    project_dir: project_dir.to_string(),
                },
                TabWorktree {
                    repo_root: repo_root.to_string(),
                    checkout_dir: project_dir.to_string(),
                    branch: "vmux/test".to_string(),
                    base_ref: "main".to_string(),
                },
            ));
            self.app.update();
        }

        fn select_managed_worktree(
            &mut self,
            project_dir: &str,
            checkout_dir: &str,
            repo_root: &str,
        ) {
            self.app
                .world_mut()
                .get_mut::<Tab>(self.tab)
                .unwrap()
                .startup_dir = Some(checkout_dir.to_string());
            self.app.world_mut().entity_mut(self.tab).insert((
                TabWorkspace {
                    project_dir: project_dir.to_string(),
                },
                TabWorktree {
                    repo_root: repo_root.to_string(),
                    checkout_dir: checkout_dir.to_string(),
                    branch: "vmux/test".to_string(),
                    base_ref: "main".to_string(),
                },
            ));
            self.app.update();
        }

        fn checkouts(&self, space_id: &str) -> Vec<Option<String>> {
            self.app
                .world()
                .resource::<AppSettings>()
                .spaces
                .get(space_id)
                .map(|space| space.projects.iter().map(|p| p.checkout.clone()).collect())
                .unwrap_or_default()
        }

        fn remembered(&self, space_id: &str) -> Option<String> {
            self.app
                .world()
                .resource::<AppSettings>()
                .spaces
                .get(space_id)?
                .active_dir()
                .map(str::to_string)
        }

        fn listed(&self, space_id: &str) -> Vec<String> {
            self.app
                .world()
                .resource::<AppSettings>()
                .spaces
                .get(space_id)
                .map(|space| space.projects.iter().map(|p| p.path.clone()).collect())
                .unwrap_or_default()
        }

        fn known(&self) -> Vec<String> {
            self.app
                .world()
                .resource::<AppSettings>()
                .projects
                .iter()
                .map(|p| p.path.clone())
                .collect()
        }

        fn drain_saves(&mut self) -> usize {
            self.app
                .world_mut()
                .resource_mut::<bevy::ecs::message::Messages<SettingsSaveRequest>>()
                .drain()
                .count()
        }
    }

    struct GitProjectRowsFixture {
        _root: tempfile::TempDir,
        cloud: std::path::PathBuf,
        cloud_worktree: std::path::PathBuf,
        dashboard: std::path::PathBuf,
    }

    impl GitProjectRowsFixture {
        fn start() -> Self {
            let root = tempfile::tempdir().unwrap();
            let cloud = root.path().join("vmux-cloud");
            let dashboard = root.path().join("dashboard");
            std::fs::create_dir_all(&cloud).unwrap();
            std::fs::create_dir_all(&dashboard).unwrap();
            let cloud_checkout = CheckoutInfo::initialize(&cloud).unwrap();
            cloud_checkout.ensure_initial_snapshot("Initial").unwrap();
            let branch = cloud_checkout.head_ref().unwrap();
            let cloud_worktree = root.path().join("vmx-206");
            cloud_checkout
                .add_worktree(&cloud_worktree, "vmux/test", &branch)
                .unwrap();
            CheckoutInfo::initialize(&dashboard).unwrap();
            let client = cloud.join("client");
            std::fs::create_dir_all(&client).unwrap();
            CheckoutInfo::initialize(&client).unwrap();
            std::fs::create_dir_all(cloud.join(".git/modules")).unwrap();
            Self {
                _root: root,
                cloud,
                cloud_worktree,
                dashboard,
            }
        }

        fn row(&self, path: &std::path::Path, active: bool) -> ProjectRow {
            let path = path.to_string_lossy().into_owned();
            ProjectRow {
                label: std::path::Path::new(&path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                display_path: path.clone(),
                path,
                depth: 0,
                is_active: active,
                is_worktree: false,
                missing: false,
                branch: String::new(),
                kind: ProjectRowKind::Project,
                expanded: false,
            }
        }
    }

    #[test]
    fn git_projects_show_top_level_repositories_once() {
        let fixture = GitProjectRowsFixture::start();
        let non_repository = fixture._root.path().join("notes");
        std::fs::create_dir_all(&non_repository).unwrap();
        let rows = vec![
            fixture.row(&fixture.cloud_worktree, true),
            fixture.row(&fixture.cloud, false),
            fixture.row(&fixture.cloud.join("client"), false),
            fixture.row(&fixture.cloud.join(".git/modules"), false),
            fixture.row(&fixture.dashboard, false),
            fixture.row(&non_repository, false),
        ];

        let projection = ProjectCatalog::from_rows(&rows).repositories;

        assert_eq!(
            projection
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            ["vmux-cloud", "dashboard"]
        );
        assert_eq!(
            projection[0].path,
            fixture
                .cloud_worktree
                .canonicalize()
                .unwrap()
                .to_string_lossy()
        );
        assert!(projection[0].is_active);
        assert!(projection[0].is_worktree);
        assert_eq!(
            projection[0].display_path,
            vmux_path::DisplayPath::project(fixture.cloud.canonicalize().unwrap())
        );
    }

    #[test]
    fn project_catalog_collapses_internal_and_linked_repository_paths() {
        let fixture = GitProjectRowsFixture::start();
        let notes = fixture._root.path().join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        let rows = vec![
            fixture.row(&fixture.cloud_worktree, true),
            fixture.row(&fixture.cloud, false),
            fixture.row(&fixture.cloud.join("client"), false),
            fixture.row(&fixture.cloud.join(".git/modules"), false),
            fixture.row(&fixture.dashboard, false),
            fixture.row(&notes, false),
        ];

        let catalog = ProjectCatalog::from_rows(&rows);

        assert_eq!(
            catalog
                .rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            ["vmux-cloud", "dashboard", "notes"]
        );
        assert_eq!(
            catalog.rows[0].path,
            fixture
                .cloud_worktree
                .canonicalize()
                .unwrap()
                .to_string_lossy()
        );
        assert!(catalog.rows[0].is_active);
    }

    #[test]
    fn layout_rows_keep_only_catalog_roots_and_the_active_tree() {
        let fixture = GitProjectRowsFixture::start();
        let mut source = vec![
            fixture.row(&fixture.cloud_worktree, true),
            fixture.row(&fixture.cloud, false),
            fixture.row(&fixture.cloud.join("client"), false),
            fixture.row(&fixture.dashboard, false),
        ];
        let mut child = fixture.row(&fixture.cloud_worktree.join("src"), false);
        child.depth = 1;
        child.kind = ProjectRowKind::Directory;
        source.insert(1, child.clone());
        let catalog = ProjectCatalog::from_rows(&source);
        let active = ActiveProjectRows {
            space: Some("work".to_string()),
            rows: catalog.rows,
            repositories: catalog.repositories,
        };

        let rows = active.layout_rows(Some("work"), &source);

        assert_eq!(
            rows.iter()
                .filter(|row| row.depth == 0)
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            ["vmux-cloud", "dashboard"]
        );
        assert!(rows.iter().any(|row| row.path == child.path));
    }

    #[test]
    fn project_picker_starts_at_the_nearest_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("projects");
        std::fs::create_dir(&existing).unwrap();
        let missing = existing.join("github.com/vmux-ai/vmux");

        assert_eq!(
            ProjectPicker::initial_directory(&missing.to_string_lossy()),
            existing
        );
    }

    #[test]
    fn a_toggle_opens_the_directory_on_the_space_that_owns_the_pane() {
        let mut fixture = Fixture::start("work");
        let pane = fixture.pane.to_bits().to_string();

        fixture.toggle("/tmp/alpha/src", pane.clone());
        assert_eq!(fixture.open_dirs(), vec!["/tmp/alpha/src".to_string()]);

        fixture.toggle("/tmp/alpha/src", pane);
        assert!(
            fixture.open_dirs().is_empty(),
            "toggling the same directory again closes it"
        );
    }

    #[test]
    fn a_toggle_that_names_no_pane_opens_nothing() {
        let mut fixture = Fixture::start("work");

        fixture.toggle("/tmp/alpha/src", String::new());

        assert!(
            fixture.open_dirs().is_empty(),
            "the side sheet names its pane, and the space is reached through it"
        );
    }

    #[test]
    fn active_project_tree_is_visible_without_a_persisted_toggle() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        let path = project.to_string_lossy().into_owned();
        let mut settings = AppSettings::default();
        settings.spaces.insert(
            "work".to_string(),
            SpaceOverrides {
                projects: vec![SpaceProject::at(path.clone())],
                active_project: Some(path.clone()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.insert_resource(settings);
        let expanded_space = app
            .world_mut()
            .spawn((
                Space,
                SpaceId("work".to_string()),
                ExpandedProjectDirs(vec![path]),
            ))
            .id();
        let fresh_space = app
            .world_mut()
            .spawn((Space, SpaceId("work".to_string())))
            .id();
        let expanded_pane = app.world_mut().spawn(ChildOf(expanded_space)).id();
        let fresh_pane = app.world_mut().spawn(ChildOf(fresh_space)).id();

        let (expanded, fresh) = app
            .world_mut()
            .run_system_once(move |projects: SpaceProjects| {
                (projects.rows(expanded_pane), projects.rows(fresh_pane))
            })
            .unwrap();

        assert!(expanded[0].expanded);
        assert!(expanded.len() > 1);
        assert!(fresh[0].expanded);
        assert!(fresh.len() > 1);
    }

    #[test]
    fn recording_a_project_marks_the_settings_changed() {
        let mut fixture = Fixture::start("work");

        fixture.select("/tmp/alpha");
        assert!(
            fixture.app.world().resource::<SawSettingsChange>().0,
            "the effective startup dir is recomputed off this flag, so a bypassed write strands it"
        );

        fixture.select("/tmp/alpha");
        assert!(
            !fixture.app.world().resource::<SawSettingsChange>().0,
            "reselecting the same project changes nothing and must not churn"
        );
    }

    #[test]
    fn selecting_a_project_records_it_against_the_space() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.remembered("work").as_deref(), Some("/tmp/alpha"));
        assert_eq!(fixture.drain_saves(), 1);
    }

    #[test]
    fn reselecting_the_same_project_does_not_ask_for_another_save() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.drain_saves(), 1);
        fixture.select("/tmp/alpha");
        assert_eq!(fixture.drain_saves(), 0);
    }

    #[test]
    fn switching_project_keeps_the_previous_one_in_the_list() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        fixture.select("/tmp/beta");

        assert_eq!(fixture.listed("work"), ["/tmp/alpha", "/tmp/beta"]);
        assert_eq!(fixture.remembered("work").as_deref(), Some("/tmp/beta"));
    }

    #[test]
    fn reselecting_an_earlier_project_promotes_it_without_duplicating() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        fixture.select("/tmp/beta");
        fixture.select("/tmp/alpha");

        assert_eq!(
            fixture.listed("work"),
            ["/tmp/alpha", "/tmp/beta"],
            "the list keeps the order projects were first seen in"
        );
        assert_eq!(fixture.remembered("work").as_deref(), Some("/tmp/alpha"));
    }

    #[test]
    fn a_selected_project_is_also_remembered_across_spaces() {
        let mut fixture = Fixture::start("work");
        fixture.select("/tmp/alpha");
        fixture.select("/tmp/beta");

        assert_eq!(
            fixture.known(),
            ["/tmp/beta", "/tmp/alpha"],
            "the global list is most-recent-first so a new space can offer them"
        );
    }

    #[test]
    fn a_worktree_becomes_the_checkout_of_the_repository_it_came_from() {
        let mut fixture = Fixture::start("work");
        fixture.select_worktree("/worktrees/a1b2", "/repo/dashboard");

        assert_eq!(
            fixture.listed("work"),
            ["/repo/dashboard"],
            "a worktree is a checkout of its repository, not a project of its own"
        );
        assert_eq!(
            fixture.checkouts("work"),
            [Some("/worktrees/a1b2".to_string())]
        );
        assert_eq!(
            fixture.remembered("work").as_deref(),
            Some("/worktrees/a1b2"),
            "the tab is working in the worktree, so that is what the space is on"
        );
    }

    #[test]
    fn a_managed_worktree_records_the_tab_cwd_as_the_checkout() {
        let mut fixture = Fixture::start("work");
        fixture.select_managed_worktree("/repo/dashboard", "/worktrees/a1b2", "/repo/dashboard");

        assert_eq!(
            fixture.checkouts("work"),
            [Some("/worktrees/a1b2".to_string())]
        );
        assert_eq!(
            fixture.remembered("work"),
            Some("/worktrees/a1b2".to_string())
        );
    }

    #[test]
    fn a_worktree_listed_plainly_folds_into_its_repository_once_it_is_known() {
        let mut fixture = Fixture::start("work");
        fixture.select("/worktrees/a1b2");
        assert_eq!(fixture.listed("work"), ["/worktrees/a1b2"]);

        fixture.select_worktree("/worktrees/a1b2", "/repo/dashboard");

        assert_eq!(
            fixture.listed("work"),
            ["/repo/dashboard"],
            "the plain entry is replaced, never left beside the repository"
        );
        assert_eq!(
            fixture.checkouts("work"),
            [Some("/worktrees/a1b2".to_string())]
        );
    }

    #[test]
    fn a_project_belongs_to_the_space_that_selected_it() {
        let mut work = Fixture::start("work");
        work.select("/tmp/alpha");
        let mut play = Fixture::start("play");
        play.select("/tmp/beta");

        assert_eq!(work.listed("work"), ["/tmp/alpha"]);
        assert_eq!(play.listed("play"), ["/tmp/beta"]);
        assert!(play.listed("work").is_empty());
    }
}
