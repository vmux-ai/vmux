use std::collections::{BTreeMap, HashMap};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::event::{ProjectRow, ProjectRowKind};

use super::runtime::{AppSettings, SettingsSaveRequest, SpaceProject};

pub(super) struct SettingsPolicyPlugin;

impl Plugin for SettingsPolicyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RememberSpaceProject>()
            .add_message::<ActivateSpaceProject>()
            .add_message::<ForgetSpaceProject>()
            .add_message::<RenameKnownProject>()
            .add_message::<AssociateKnownProject>()
            .add_systems(
                Update,
                (
                    apply_remember_space_project,
                    apply_activate_space_project,
                    apply_forget_space_project,
                    apply_rename_known_project,
                    apply_associate_known_project,
                    sync_space_and_project_entities,
                    verify_space_settings_index,
                )
                    .chain()
                    .in_set(SettingsPolicySet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SettingsPolicySet;

#[derive(Component)]
pub struct SpaceSetting;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SpaceSettingKey(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SpaceStartupUrl(pub Option<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SpaceActiveProject(pub Option<String>);

#[derive(Component)]
pub struct KnownProject;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct KnownProjectId(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct KnownProjectPath(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct KnownProjectCheckout(pub Option<String>);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct KnownProjectLabel(pub Option<String>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownProjectOrder(pub usize);

#[derive(Component)]
pub struct SpaceProjectBinding;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceBindingTarget(pub Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceBindingProject(pub Entity);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SpaceBindingCheckout(pub Option<String>);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceBindingOrder(pub usize);

#[derive(Message, Clone, Debug)]
pub struct RememberSpaceProject {
    pub space_id: String,
    pub project: SpaceProject,
}

#[derive(Message, Clone, Debug)]
pub struct ActivateSpaceProject {
    pub space_id: String,
    pub path: String,
}

#[derive(Message, Clone, Debug)]
pub struct ForgetSpaceProject {
    pub space_id: String,
    pub path: String,
}

#[derive(Message, Clone, Debug)]
pub struct RenameKnownProject {
    pub id: String,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct AssociateKnownProject {
    pub path: String,
    pub id: String,
}

#[derive(SystemParam)]
pub struct SpaceSettings<'w, 's> {
    spaces: Query<
        'w,
        's,
        (
            Entity,
            &'static SpaceSettingKey,
            &'static SpaceStartupUrl,
            &'static SpaceActiveProject,
        ),
        With<SpaceSetting>,
    >,
    projects: Query<
        'w,
        's,
        (
            Entity,
            &'static KnownProjectId,
            &'static KnownProjectPath,
            &'static KnownProjectCheckout,
            &'static KnownProjectLabel,
            &'static KnownProjectOrder,
        ),
        With<KnownProject>,
    >,
    bindings: Query<
        'w,
        's,
        (
            &'static SpaceBindingTarget,
            &'static SpaceBindingProject,
            &'static SpaceBindingCheckout,
            &'static SpaceBindingOrder,
        ),
        With<SpaceProjectBinding>,
    >,
}

impl SpaceSettings<'_, '_> {
    pub fn space_count(&self) -> usize {
        self.spaces.iter().count()
    }

    pub fn space_entity(&self, space_id: &str) -> Option<Entity> {
        let target = AppSettings::normalize_space_key(space_id);
        for (entity, key, _, _) in &self.spaces {
            if key.0 == space_id || AppSettings::normalize_space_key(&key.0) == target {
                return Some(entity);
            }
        }
        None
    }

    pub fn startup_url(&self, space_id: &str) -> Option<&str> {
        let entity = self.space_entity(space_id)?;
        let (_, _, url, _) = self.spaces.get(entity).ok()?;
        url.0.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn active_dir(&self, space_id: &str) -> Option<String> {
        let space = self.space_entity(space_id)?;
        let (_, _, _, active) = self.spaces.get(space).ok()?;
        let rows = self.bound_projects(space);
        if let Some(active) = active.0.as_deref()
            && rows.iter().any(|(_, in_use, _, _)| in_use == active)
        {
            return Some(active.to_string());
        }
        rows.first().map(|(_, in_use, _, _)| in_use.clone())
    }

    pub fn project_rows(&self, space_id: &str) -> Vec<ProjectRow> {
        let mut rows = Vec::new();
        if let Some(space) = self.space_entity(space_id) {
            let active = self.active_dir(space_id);
            for (_, in_use, project, checkout) in self.bound_projects(space) {
                if rows.iter().any(|row: &ProjectRow| row.path == in_use) {
                    continue;
                }
                rows.push(ProjectRow {
                    is_active: active.as_deref() == Some(in_use.as_str()),
                    is_worktree: checkout.is_some(),
                    missing: !std::path::Path::new(&in_use).is_dir(),
                    path: in_use,
                    identity: project.identity().to_string(),
                    label: project.display_label().to_string(),
                    display_path: project.path,
                    depth: 0,
                    branch: String::new(),
                    kind: ProjectRowKind::Project,
                    expanded: false,
                });
            }
        }
        let mut known: Vec<_> = self.projects.iter().collect();
        known.sort_by_key(|(_, _, _, _, _, order)| order.0);
        for (_, id, path, checkout, label, _) in known {
            let project = SpaceProject::from_parts(
                id.0.clone(),
                path.0.clone(),
                checkout.0.clone(),
                label.0.clone(),
            );
            let in_use = project.in_use().to_string();
            if rows.iter().any(|row| row.path == in_use) {
                continue;
            }
            rows.push(ProjectRow {
                is_active: false,
                is_worktree: project.checkout.is_some(),
                missing: !std::path::Path::new(&in_use).is_dir(),
                path: in_use,
                identity: project.identity().to_string(),
                label: project.display_label().to_string(),
                display_path: project.path,
                depth: 0,
                branch: String::new(),
                kind: ProjectRowKind::Project,
                expanded: false,
            });
        }
        rows
    }

    fn bound_projects(&self, space: Entity) -> Vec<(usize, String, SpaceProject, Option<String>)> {
        let mut items = Vec::new();
        for (target, project_ref, checkout, order) in &self.bindings {
            if target.0 != space {
                continue;
            }
            let Ok((_, id, path, _, label, _)) = self.projects.get(project_ref.0) else {
                continue;
            };
            let project = SpaceProject::from_parts(
                id.0.clone(),
                path.0.clone(),
                checkout.0.clone(),
                label.0.clone(),
            );
            let in_use = project.in_use().to_string();
            items.push((order.0, in_use, project, checkout.0.clone()));
        }
        items.sort_by_key(|(order, _, _, _)| *order);
        items
    }
}

fn apply_remember_space_project(
    mut requests: MessageReader<RememberSpaceProject>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let mut changed = false;
    for request in requests.read() {
        if settings.remember_space_project(&request.space_id, request.project.clone()) {
            changed = true;
        }
    }
    if changed {
        saves.write(SettingsSaveRequest);
    }
}

fn apply_activate_space_project(
    mut requests: MessageReader<ActivateSpaceProject>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let mut changed = false;
    for request in requests.read() {
        if settings.activate_space_project(&request.space_id, &request.path) {
            changed = true;
        }
    }
    if changed {
        saves.write(SettingsSaveRequest);
    }
}

fn apply_forget_space_project(
    mut requests: MessageReader<ForgetSpaceProject>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let mut changed = false;
    for request in requests.read() {
        if settings.forget_space_project(&request.space_id, &request.path) {
            changed = true;
        }
    }
    if changed {
        saves.write(SettingsSaveRequest);
    }
}

fn apply_rename_known_project(
    mut requests: MessageReader<RenameKnownProject>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let mut changed = false;
    for request in requests.read() {
        if settings.rename_project(&request.id, &request.name) {
            changed = true;
        }
    }
    if changed {
        saves.write(SettingsSaveRequest);
    }
}

fn apply_associate_known_project(
    mut requests: MessageReader<AssociateKnownProject>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let mut changed = false;
    for request in requests.read() {
        if settings.associate_project(&request.path, &request.id) {
            changed = true;
        }
    }
    if changed {
        saves.write(SettingsSaveRequest);
    }
}

fn sync_space_and_project_entities(
    settings: Option<Res<AppSettings>>,
    existing_spaces: Query<Entity, With<SpaceSetting>>,
    existing_projects: Query<Entity, With<KnownProject>>,
    existing_bindings: Query<Entity, With<SpaceProjectBinding>>,
    mut commands: Commands,
) {
    let Some(settings) = settings else {
        return;
    };
    if !settings.is_added()
        && !settings.is_changed()
        && (!existing_spaces.is_empty() || settings.spaces.is_empty())
        && (!existing_projects.is_empty() || settings.projects.is_empty())
    {
        return;
    }
    for entity in &existing_bindings {
        commands.entity(entity).despawn();
    }
    for entity in &existing_spaces {
        commands.entity(entity).despawn();
    }
    for entity in &existing_projects {
        commands.entity(entity).despawn();
    }

    let mut projects_by_path: HashMap<String, Entity> = HashMap::new();
    let mut next_order = 0usize;
    for project in &settings.projects {
        let entity = commands
            .spawn((
                KnownProject,
                KnownProjectId(project.id.clone()),
                KnownProjectPath(project.path.clone()),
                KnownProjectCheckout(project.checkout.clone()),
                KnownProjectLabel(project.label.clone()),
                KnownProjectOrder(next_order),
            ))
            .id();
        projects_by_path.insert(project.path.clone(), entity);
        next_order += 1;
    }

    let sorted_spaces: BTreeMap<_, _> = settings.spaces.iter().collect();
    for (key, overrides) in sorted_spaces {
        let space_entity = commands
            .spawn((
                SpaceSetting,
                SpaceSettingKey(key.clone()),
                SpaceStartupUrl(overrides.startup_url.clone()),
                SpaceActiveProject(overrides.active_project.clone()),
            ))
            .id();
        for (index, project) in overrides.projects.iter().enumerate() {
            let project_entity = match projects_by_path.get(&project.path) {
                Some(entity) => *entity,
                None => {
                    let entity = commands
                        .spawn((
                            KnownProject,
                            KnownProjectId(project.id.clone()),
                            KnownProjectPath(project.path.clone()),
                            KnownProjectCheckout(project.checkout.clone()),
                            KnownProjectLabel(project.label.clone()),
                            KnownProjectOrder(next_order),
                        ))
                        .id();
                    projects_by_path.insert(project.path.clone(), entity);
                    next_order += 1;
                    entity
                }
            };
            commands.spawn((
                SpaceProjectBinding,
                SpaceBindingTarget(space_entity),
                SpaceBindingProject(project_entity),
                SpaceBindingCheckout(project.checkout.clone()),
                SpaceBindingOrder(index),
            ));
        }
    }
}

fn verify_space_settings_index(spaces: SpaceSettings) {
    let _ = spaces.space_count();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;

    struct PolicyFixture {
        app: App,
    }

    impl PolicyFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, SettingsPolicyPlugin))
                .add_message::<SettingsSaveRequest>()
                .init_resource::<AppSettings>();
            Self { app }
        }

        fn save_count(&mut self) -> usize {
            self.app
                .world_mut()
                .resource_mut::<Messages<SettingsSaveRequest>>()
                .drain()
                .count()
        }

        fn rows(&mut self, space_id: &str) -> Vec<ProjectRow> {
            let space_id = space_id.to_string();
            self.app
                .world_mut()
                .run_system_once(move |spaces: SpaceSettings| spaces.project_rows(&space_id))
                .unwrap()
        }

        fn active_dir(&mut self, space_id: &str) -> Option<String> {
            let space_id = space_id.to_string();
            self.app
                .world_mut()
                .run_system_once(move |spaces: SpaceSettings| spaces.active_dir(&space_id))
                .unwrap()
        }
    }

    #[test]
    fn typed_space_project_messages_update_canonical_entities_and_request_save() {
        let mut fixture = PolicyFixture::new();
        fixture.app.world_mut().write_message(RememberSpaceProject {
            space_id: "Work/Main".to_string(),
            project: SpaceProject::checked_out("/repo/app", "/worktrees/a"),
        });
        fixture.app.update();

        assert_eq!(fixture.save_count(), 1);
        assert_eq!(
            fixture.active_dir("work-main").as_deref(),
            Some("/worktrees/a")
        );
        let rows = fixture.rows("work-main");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_active);
        assert!(rows[0].is_worktree);
        let id = rows[0].identity.clone();
        assert!(!id.is_empty());

        fixture.app.world_mut().write_message(RenameKnownProject {
            id: id.clone(),
            name: "Primary App".to_string(),
        });
        fixture.app.update();

        assert_eq!(fixture.save_count(), 1);
        assert_eq!(fixture.rows("work-main")[0].label, "Primary App");

        fixture.app.world_mut().write_message(ForgetSpaceProject {
            space_id: "work-main".to_string(),
            path: "/repo/app".to_string(),
        });
        fixture.app.update();

        assert_eq!(fixture.save_count(), 1);
        assert_eq!(fixture.active_dir("work-main"), None);
    }

    #[test]
    fn associate_and_activate_messages_sync_canonical_space_bindings() {
        let mut fixture = PolicyFixture::new();
        fixture.app.world_mut().write_message(RememberSpaceProject {
            space_id: "dev".to_string(),
            project: SpaceProject::at("/repo/alpha"),
        });
        fixture.app.world_mut().write_message(RememberSpaceProject {
            space_id: "dev".to_string(),
            project: SpaceProject::at("/repo/beta"),
        });
        fixture.app.update();
        let _ = fixture.save_count();

        fixture
            .app
            .world_mut()
            .write_message(AssociateKnownProject {
                path: "/repo/alpha".to_string(),
                id: "canonical-alpha-id".to_string(),
            });
        fixture.app.world_mut().write_message(ActivateSpaceProject {
            space_id: "dev".to_string(),
            path: "/repo/alpha".to_string(),
        });
        fixture.app.update();

        assert_eq!(fixture.save_count(), 1);
        assert_eq!(fixture.active_dir("dev").as_deref(), Some("/repo/alpha"));
        let rows = fixture.rows("dev");
        let alpha = rows.iter().find(|row| row.path == "/repo/alpha").unwrap();
        assert_eq!(alpha.identity, "canonical-alpha-id");
        assert!(alpha.is_active);
    }
}
