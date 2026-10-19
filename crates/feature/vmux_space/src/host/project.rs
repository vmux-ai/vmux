use std::time::{Duration, Instant};

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
use vmux_git::worktree::LinkedRepoRoot;
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
            .init_resource::<ActiveProjectRows>()
            .init_resource::<ActiveProjectScan>()
            .register_persisted::<ExpandedProjectDirs>()
            .add_observer(tree_toggle)
            .add_observer(pick)
            .add_systems(
                Update,
                (
                    remember.before(EffectiveStartupSet),
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
struct ActiveProjectRows {
    space: Option<String>,
    rows: Vec<ProjectRow>,
}

type ActiveProjectScanResult = (Option<String>, Vec<ProjectRow>);

#[derive(Resource, Default)]
struct ActiveProjectScan {
    task: Option<Task<ActiveProjectScanResult>>,
    requested: bool,
    scanned_at: Option<Instant>,
}

const PROJECT_ROWS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

fn refresh_active_projects(
    projects: SpaceProjects,
    mut active: ResMut<ActiveProjectRows>,
    mut scan: ResMut<ActiveProjectScan>,
) {
    let space = projects.active_space.id();
    if let Some(task) = scan.task.as_mut()
        && let Some((scanned_space, rows)) = future::block_on(future::poll_once(task))
    {
        scan.task = None;
        scan.scanned_at = Some(Instant::now());
        if scanned_space.as_deref() == space
            && (active.space != scanned_space || active.rows != rows)
        {
            active.space = scanned_space;
            active.rows = rows;
        }
    }
    let now = Instant::now();
    let scan_due = scan
        .scanned_at
        .as_ref()
        .is_none_or(|last| now.duration_since(*last) >= PROJECT_ROWS_REFRESH_INTERVAL);
    scan.requested |= active.space.as_deref() != space
        || projects.settings_changed()
        || (scan.task.is_none() && scan_due);
    if scan.task.is_some() || !scan.requested {
        return;
    }
    scan.requested = false;
    let space = space.map(str::to_string);
    let settings = projects.settings.as_deref().cloned();
    scan.task = Some(IoTaskPool::get().spawn(async move {
        let rows = match (space.as_deref(), settings.as_ref()) {
            (Some(space), Some(settings)) => settings.project_rows(space),
            _ => Vec::new(),
        };
        (space, rows)
    }));
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

fn publish_git_projects(
    projects: Res<ActiveProjectRows>,
    views: Query<(Entity, &PageMetadata, Option<&GitProjectsProjection>), With<PageReady>>,
    mut commands: Commands,
) {
    let rows = &projects.rows;
    let changed = projects.is_changed();
    for (entity, page, current) in &views {
        if !page.url.starts_with(vmux_git::GIT_PAGE_URL)
            && page.url != vmux_git::GitPlugin::MANIFEST.url
        {
            continue;
        }
        if !changed && current.is_some() {
            continue;
        }
        if current.is_some_and(|current| current.0.as_slice() == rows.as_slice()) {
            continue;
        }
        commands
            .entity(entity)
            .insert(GitProjectsProjection(rows.clone()));
        commands.trigger(UiStateWrite::<vmux_git::state::GitUiState>::from_event(
            entity,
            &vmux_git::state::GitProjects { rows: rows.clone() },
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

    pub fn active_projects(&self) -> Vec<ProjectRow> {
        let Some(space_id) = self.active_space.id() else {
            return Vec::new();
        };
        self.projects_of(space_id)
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
    fn resolve(&mut self, dir: &str) -> Option<String> {
        if let Some(held) = self.0.get(dir) {
            return held.clone();
        }
        let found = Self::read(dir);
        self.0.insert(dir.to_string(), found.clone());
        found
    }

    fn read(dir: &str) -> Option<String> {
        let root = LinkedRepoRoot::find(std::path::Path::new(dir))?;
        let root = root.to_string_lossy().into_owned();
        (root != dir && !root.is_empty()).then_some(root)
    }
}

fn remember(
    bound: Query<(
        Entity,
        Ref<TabWorkspace>,
        Option<Ref<TabWorktree>>,
        Option<&Tab>,
    )>,
    space_of_tab: SpaceOfTab,
    mut roots: Single<&mut RepoRoots>,
    settings: Option<ResMut<AppSettings>>,
    mut saves: MessageWriter<SettingsSaveRequest>,
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
            _ => match roots.resolve(dir) {
                Some(root) => SpaceProject::checked_out(&root, dir),
                None => SpaceProject::at(dir),
            },
        };
        let changed = settings
            .bypass_change_detection()
            .remember_space_project(&space_id, project);
        if changed {
            settings.set_changed();
            saves.write(SettingsSaveRequest);
        }
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
                .add_systems(Update, record_settings_change.after(remember))
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
            self.app.update();
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
