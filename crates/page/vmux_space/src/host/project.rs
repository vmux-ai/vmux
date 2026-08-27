use bevy::prelude::*;

pub struct SpaceProjectPlugin;

impl Plugin for SpaceProjectPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ExpandedProjectDirs>()
            .init_resource::<vmux_command::snapshot::CommandBarProjectRoots>()
            .add_observer(on_project_tree_toggle)
            .add_systems(
                Update,
                (
                    remember_space_project.before(vmux_layout::settings::EffectiveStartupDirSet),
                    publish_project_roots
                        .in_set(vmux_command::snapshot::WriteCommandBarSnapshots)
                        .after(remember_space_project),
                ),
            );
    }
}

fn on_project_tree_toggle(
    trigger: On<bevy_cef::prelude::BinReceive<vmux_core::event::ProjectTreeToggle>>,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<vmux_layout::space::Space>>,
    mut expanded: Query<&mut ExpandedProjectDirs>,
    mut commands: Commands,
) {
    let Some(space) = vmux_layout::space::space_of(trigger.event().webview, &child_of, &spaces)
    else {
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

fn publish_project_roots(
    projects: SpaceProjects,
    mut roots: ResMut<vmux_command::snapshot::CommandBarProjectRoots>,
) {
    let mut next = Vec::new();
    for project in projects.active_rows() {
        if project.missing {
            continue;
        }
        next.push(project.path);
    }
    if roots.roots != next {
        roots.roots = next;
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

    fn children_of(&self, dir: &std::path::Path, depth: u32) -> Vec<vmux_core::event::ProjectRow> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || UNLISTED_DIRS.contains(&name.as_str()) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path().to_string_lossy().into_owned();
            rows.push(vmux_core::event::ProjectRow {
                label: name,
                display_path: path.clone(),
                depth,
                kind: match kind.is_dir() {
                    true => vmux_core::event::ProjectRowKind::Directory,
                    false => vmux_core::event::ProjectRowKind::File,
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
            let folder_first = b.kind.opens_a_tree().cmp(&a.kind.opens_a_tree());
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
    settings: Option<Res<'w, vmux_setting::AppSettings>>,
    active_space: Option<Res<'w, super::spaces::ActiveSpace>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    spaces: Query<'w, 's, (), With<vmux_layout::space::Space>>,
    space_ids: Query<'w, 's, &'static vmux_layout::space::SpaceId>,
    expanded: Query<
        'w,
        's,
        (
            &'static vmux_layout::space::SpaceId,
            &'static ExpandedProjectDirs,
        ),
        With<vmux_layout::space::Space>,
    >,
}

impl SpaceProjects<'_, '_> {
    pub fn rows(&self, entity: Entity) -> Vec<vmux_core::event::ProjectRow> {
        let space_id =
            vmux_layout::space::space_id_of(entity, &self.child_of, &self.spaces, &self.space_ids);
        let Some(space_id) = space_id else {
            return self.active_rows();
        };
        self.rows_of(&space_id)
    }

    pub fn active_rows(&self) -> Vec<vmux_core::event::ProjectRow> {
        let Some(active) = self.active_space.as_deref() else {
            return Vec::new();
        };
        self.rows_of(&active.record.id)
    }

    fn expanded_of(&self, space_id: &str) -> Option<&ExpandedProjectDirs> {
        for (id, dirs) in &self.expanded {
            if id.0 == space_id {
                return Some(dirs);
            }
        }
        None
    }

    fn rows_of(&self, space_id: &str) -> Vec<vmux_core::event::ProjectRow> {
        let Some(settings) = self.settings.as_deref() else {
            return Vec::new();
        };
        let Some(overrides) = settings.space(space_id) else {
            return Vec::new();
        };
        let listed = overrides.project_rows();
        let Some(expanded) = self.expanded_of(space_id) else {
            return listed;
        };
        let mut rows = Vec::new();
        for mut project in listed {
            let open = !project.missing && expanded.holds(&project.path);
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

fn remember_space_project(
    bound: Query<
        (Entity, &vmux_layout::tab::TabWorkspace),
        Changed<vmux_layout::tab::TabWorkspace>,
    >,
    worktrees: Query<&vmux_layout::tab::TabWorktree>,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<vmux_layout::space::Space>>,
    ids: Query<&vmux_layout::space::SpaceId>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    mut saves: MessageWriter<vmux_setting::SettingsSaveRequest>,
) {
    if bound.is_empty() {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    for (tab, workspace) in &bound {
        let dir = workspace.project_dir.trim();
        if dir.is_empty() {
            continue;
        }
        let Some(space_id) = vmux_layout::space::space_id_of(tab, &child_of, &spaces, &ids) else {
            continue;
        };
        let repo_root = worktrees
            .get(tab)
            .ok()
            .map(|worktree| worktree.repo_root.trim())
            .filter(|root| !root.is_empty() && *root != dir);
        let settings = settings.bypass_change_detection();
        let mut changed = false;
        if let Some(root) = repo_root {
            changed |=
                settings.remember_space_project(&space_id, vmux_setting::SpaceProject::at(root));
        }
        let project = match repo_root {
            Some(root) => vmux_setting::SpaceProject::under(dir, root),
            None => vmux_setting::SpaceProject::at(dir),
        };
        changed |= settings.remember_space_project(&space_id, project);
        if changed {
            saves.write(vmux_setting::SettingsSaveRequest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        app: App,
        tab: Entity,
    }

    impl Fixture {
        fn start(space_id: &str) -> Self {
            let mut app = App::new();
            app.add_plugins(SpaceProjectPlugin)
                .add_message::<vmux_setting::SettingsSaveRequest>()
                .insert_resource(vmux_setting::AppSettings::embedded());
            let space = app
                .world_mut()
                .spawn((
                    vmux_layout::space::Space,
                    vmux_layout::space::SpaceId(space_id.to_string()),
                ))
                .id();
            let tab = app.world_mut().spawn(ChildOf(space)).id();
            Self { app, tab }
        }

        fn select(&mut self, project_dir: &str) {
            self.app
                .world_mut()
                .entity_mut(self.tab)
                .insert(vmux_layout::tab::TabWorkspace {
                    project_dir: project_dir.to_string(),
                });
            self.app.update();
        }

        fn select_worktree(&mut self, project_dir: &str, repo_root: &str) {
            self.app.world_mut().entity_mut(self.tab).insert((
                vmux_layout::tab::TabWorkspace {
                    project_dir: project_dir.to_string(),
                },
                vmux_layout::tab::TabWorktree {
                    repo_root: repo_root.to_string(),
                    checkout_dir: project_dir.to_string(),
                    branch: "vmux/test".to_string(),
                    base_ref: "main".to_string(),
                },
            ));
            self.app.update();
        }

        fn parents(&self, space_id: &str) -> Vec<Option<String>> {
            self.app
                .world()
                .resource::<vmux_setting::AppSettings>()
                .spaces
                .get(space_id)
                .map(|space| space.projects.iter().map(|p| p.parent.clone()).collect())
                .unwrap_or_default()
        }

        fn remembered(&self, space_id: &str) -> Option<String> {
            self.app
                .world()
                .resource::<vmux_setting::AppSettings>()
                .spaces
                .get(space_id)?
                .active_dir()
                .map(str::to_string)
        }

        fn listed(&self, space_id: &str) -> Vec<String> {
            self.app
                .world()
                .resource::<vmux_setting::AppSettings>()
                .spaces
                .get(space_id)
                .map(|space| space.projects.iter().map(|p| p.path.clone()).collect())
                .unwrap_or_default()
        }

        fn known(&self) -> Vec<String> {
            self.app
                .world()
                .resource::<vmux_setting::AppSettings>()
                .projects
                .iter()
                .map(|p| p.path.clone())
                .collect()
        }

        fn drain_saves(&mut self) -> usize {
            self.app
                .world_mut()
                .resource_mut::<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>()
                .drain()
                .count()
        }
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
    fn a_worktree_registers_under_the_repository_it_came_from() {
        let mut fixture = Fixture::start("work");
        fixture.select_worktree("/worktrees/a1b2", "/repo/dashboard");

        assert_eq!(
            fixture.listed("work"),
            ["/repo/dashboard", "/worktrees/a1b2"],
            "the repository is listed too, or the worktree has nothing to nest under"
        );
        assert_eq!(
            fixture.parents("work"),
            [None, Some("/repo/dashboard".to_string())]
        );
        assert_eq!(
            fixture.remembered("work").as_deref(),
            Some("/worktrees/a1b2"),
            "the tab is working in the worktree, so that is what the space is on"
        );
    }

    #[test]
    fn a_worktree_that_arrives_after_its_repository_gains_the_link() {
        let mut fixture = Fixture::start("work");
        fixture.select("/worktrees/a1b2");
        assert_eq!(fixture.parents("work"), [None]);

        fixture.select_worktree("/worktrees/a1b2", "/repo/dashboard");

        assert_eq!(
            fixture.listed("work"),
            ["/worktrees/a1b2", "/repo/dashboard"],
            "the directory keeps the place it was first seen in"
        );
        assert_eq!(
            fixture.parents("work"),
            [Some("/repo/dashboard".to_string()), None],
            "a directory already listed plainly gains its parent in place"
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
