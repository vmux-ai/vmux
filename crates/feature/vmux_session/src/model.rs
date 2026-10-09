use std::path::PathBuf;

#[cfg(host)]
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
#[cfg(host)]
use std::collections::HashSet;
use vmux_api::conversation::SessionId;
#[cfg(host)]
use vmux_ecs::persistence::PersistenceAppExt;
#[cfg(host)]
use vmux_ecs::{
    ActivateRequest, CreatedAt, Cwd, Description, EntityTarget, LastActivatedAt, Order,
    PageMetadata,
};

#[cfg(host)]
pub(crate) struct EntityPlugin;

#[cfg(host)]
impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        app.register_persisted::<Session>()
            .register_persisted::<SessionId>()
            .register_persisted::<AgentId>()
            .register_persisted::<LocalTask>()
            .add_message::<CreateRequest>()
            .add_message::<CleanupRequest>()
            .add_observer(activate)
            .add_systems(Update, (create, cleanup).in_set(MutationSet));
    }
}

#[cfg(host)]
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MutationSet;

#[cfg(host)]
pub(crate) struct MetadataPlugin;

#[cfg(host)]
impl Plugin for MetadataPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RenameRequest>()
            .add_message::<DescriptionUpdateRequest>()
            .add_systems(Update, (rename, update_description).in_set(MutationSet))
            .add_systems(Update, project_views.after(MutationSet));
    }
}

#[cfg(host)]
pub(crate) struct StagePlugin;

#[cfg(host)]
impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Configuration::load())
            .register_persisted::<Stage>()
            .register_persisted::<StageChangedAt>()
            .register_type::<StageId>()
            .register_type::<StageDefinition>()
            .add_message::<StageChangeRequest>()
            .add_systems(Startup, spawn_stages)
            .add_systems(Update, change_stage.in_set(MutationSet));
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Session;

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct AgentId(pub String);

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct StageId(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct Stage(pub StageId);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct StageChangedAt(pub i64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_session"]
pub struct LocalTask;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct StageDefinition;

#[derive(Message, Clone, Debug)]
pub struct CreateRequest {
    id: SessionId,
    pub name: String,
    pub description: String,
    pub cwd: PathBuf,
    pub agent: Option<AgentId>,
}

impl CreateRequest {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        cwd: PathBuf,
        agent: Option<AgentId>,
    ) -> Self {
        Self {
            id: SessionId(uuid::Uuid::new_v4().to_string()),
            name: name.into(),
            description: description.into(),
            cwd,
            agent,
        }
    }

    pub fn id(&self) -> &SessionId {
        &self.id
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Created {
    #[event_target]
    pub entity: Entity,
    pub id: SessionId,
}

#[derive(Message, Clone, Debug)]
pub struct RenameRequest {
    pub id: SessionId,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct DescriptionUpdateRequest {
    pub id: SessionId,
    pub description: String,
}

#[derive(Message, Clone, Debug)]
pub struct StageChangeRequest {
    pub id: SessionId,
    pub stage: StageId,
}

#[derive(Message, Clone, Debug)]
pub struct CleanupRequest {
    pub id: SessionId,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Cleanup {
    #[event_target]
    pub entity: Entity,
}

#[cfg(host)]
#[derive(Resource)]
struct Configuration {
    default_stage: StageId,
    stages: Vec<StageSeed>,
}

#[cfg(host)]
impl Configuration {
    fn load() -> Self {
        let file: FeatureFile = ron::from_str(include_str!("feature.ron"))
            .expect("embedded Session configuration must be valid RON");
        file.session.validate();
        Self {
            default_stage: StageId(file.session.default_stage),
            stages: file.session.stages,
        }
    }
}

#[cfg(host)]
#[derive(Deserialize)]
struct FeatureFile {
    session: SessionConfiguration,
}

#[cfg(host)]
#[derive(Deserialize)]
struct SessionConfiguration {
    default_stage: String,
    stages: Vec<StageSeed>,
}

#[cfg(host)]
impl SessionConfiguration {
    fn validate(&self) {
        let mut ids = HashSet::new();
        let mut orders = HashSet::new();
        for stage in &self.stages {
            assert!(
                !stage.id.trim().is_empty(),
                "Session stage ID must not be empty"
            );
            assert!(ids.insert(stage.id.as_str()), "duplicate Session stage ID");
            assert!(orders.insert(stage.order), "duplicate Session stage order");
        }
        assert!(
            ids.contains(self.default_stage.as_str()),
            "default Session stage must reference a configured stage"
        );
    }
}

#[cfg(host)]
#[derive(Clone, Deserialize)]
struct StageSeed {
    id: String,
    name: String,
    terminal: bool,
    order: u32,
}

#[cfg(host)]
fn spawn_stages(mut commands: Commands, configuration: Res<Configuration>) {
    for stage in &configuration.stages {
        let mut entity = commands.spawn((
            StageDefinition,
            StageId(stage.id.clone()),
            Name::new(stage.name.clone()),
            Order(stage.order),
        ));
        if stage.terminal {
            entity.insert(vmux_ecs::Terminal);
        }
    }
}

#[cfg(host)]
fn create(
    mut requests: MessageReader<CreateRequest>,
    configuration: Res<Configuration>,
    existing: Query<&SessionId, With<Session>>,
    mut commands: Commands,
) {
    if requests.is_empty() {
        return;
    }
    let mut ids = existing.iter().cloned().collect::<HashSet<_>>();
    for request in requests.read() {
        if !ids.insert(request.id.clone()) {
            continue;
        }
        let now = vmux_ecs::UnixMillis::now().0;
        let entity = {
            let mut session = commands.spawn((
                Session,
                request.id.clone(),
                Name::new(request.name.clone()),
                Description(request.description.clone()),
                Cwd(request.cwd.clone()),
                CreatedAt(now),
                LastActivatedAt(now),
                Stage(configuration.default_stage.clone()),
                StageChangedAt(now),
                LocalTask,
            ));
            if let Some(agent) = request.agent.clone() {
                session.insert(agent);
            }
            session.id()
        };
        commands.trigger(Created {
            entity,
            id: request.id.clone(),
        });
    }
}

#[cfg(host)]
fn activate(
    trigger: On<ActivateRequest>,
    parents: Query<&ChildOf>,
    targets: Query<&EntityTarget<Session>>,
    mut sessions: Query<&mut LastActivatedAt, With<Session>>,
) {
    let mut current = trigger.event_target();
    loop {
        if let Ok(mut activated_at) = sessions.get_mut(current) {
            *activated_at = LastActivatedAt::now();
            return;
        }
        if let Ok(target) = targets.get(current)
            && let Ok(mut activated_at) = sessions.get_mut(target.entity())
        {
            *activated_at = LastActivatedAt::now();
            return;
        }
        let Ok(parent) = parents.get(current) else {
            return;
        };
        current = parent.parent();
    }
}

#[cfg(host)]
fn project_views(
    sessions: Query<(&SessionId, Ref<Name>), With<Session>>,
    views: Query<(Entity, Ref<EntityTarget<Session>>, Option<&Children>)>,
    mut metadata: Query<&mut PageMetadata>,
) {
    for (view, target, children) in &views {
        let Ok((id, name)) = sessions.get(target.entity()) else {
            continue;
        };
        if !target.is_added() && !target.is_changed() && !name.is_changed() {
            continue;
        }
        let url = crate::Route::Session(id.clone()).url();
        for entity in
            std::iter::once(view).chain(children.into_iter().flat_map(|children| children.iter()))
        {
            let Ok(mut page) = metadata.get_mut(entity) else {
                continue;
            };
            page.title = name.as_str().to_string();
            page.url.clone_from(&url);
        }
    }
}

#[cfg(host)]
fn rename(
    mut requests: MessageReader<RenameRequest>,
    mut sessions: Query<(&SessionId, &mut Name), With<Session>>,
) {
    for request in requests.read() {
        for (id, mut name) in &mut sessions {
            if id == &request.id {
                *name = Name::new(request.name.clone());
            }
        }
    }
}

#[cfg(host)]
fn update_description(
    mut requests: MessageReader<DescriptionUpdateRequest>,
    mut sessions: Query<(&SessionId, &mut Description), With<Session>>,
) {
    for request in requests.read() {
        for (id, mut description) in &mut sessions {
            if id == &request.id {
                description.0.clone_from(&request.description);
            }
        }
    }
}

#[cfg(host)]
fn change_stage(
    mut requests: MessageReader<StageChangeRequest>,
    definitions: Query<&StageId, With<StageDefinition>>,
    mut sessions: Query<(&SessionId, &mut Stage, &mut StageChangedAt), With<Session>>,
) {
    for request in requests.read() {
        if !definitions.iter().any(|id| id == &request.stage) {
            continue;
        }
        for (id, mut stage, mut changed_at) in &mut sessions {
            if id == &request.id && stage.0 != request.stage {
                stage.0.clone_from(&request.stage);
                changed_at.0 = vmux_ecs::UnixMillis::now().0;
            }
        }
    }
}

#[cfg(host)]
fn cleanup(
    mut requests: MessageReader<CleanupRequest>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        for (entity, id) in &sessions {
            if id == &request.id {
                commands.trigger(Cleanup { entity });
            }
        }
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;
    use bevy_ecs::reflect::AppTypeRegistry;

    #[derive(Resource, Default)]
    struct Cleaned(Vec<Entity>);

    impl Cleaned {
        fn record(trigger: On<Cleanup>, mut cleaned: ResMut<Cleaned>) {
            cleaned.0.push(trigger.event_target());
        }
    }

    #[test]
    fn creation_uses_configured_default_stage_and_common_components() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, StagePlugin));
        let request = CreateRequest::new(
            "Task",
            "Description",
            PathBuf::from("/tmp/project"),
            Some(AgentId("codex".into())),
        );
        let expected_id = request.id.clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);

        app.update();

        let mut sessions = app.world_mut().query_filtered::<(
            &SessionId,
            &Name,
            &Description,
            &Cwd,
            &Stage,
            &AgentId,
        ), With<Session>>();
        let (id, name, description, cwd, stage, agent) = sessions.single(app.world()).unwrap();
        assert_eq!(id, &expected_id);
        assert_eq!(name.as_str(), "Task");
        assert_eq!(description.0, "Description");
        assert_eq!(cwd.0, PathBuf::from("/tmp/project"));
        assert_eq!(stage.0.0, "in_progress");
        assert_eq!(agent.0, "codex");
    }

    #[test]
    fn stage_change_rejects_unknown_stage_and_records_transition_time() {
        let mut app = App::new();
        app.add_plugins(StagePlugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("session-1".into()),
                Stage(StageId("in_progress".into())),
                StageChangedAt(1),
            ))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<Messages<StageChangeRequest>>()
            .write(StageChangeRequest {
                id: SessionId("session-1".into()),
                stage: StageId("missing".into()),
            });
        app.update();
        assert_eq!(
            app.world().get::<Stage>(session).unwrap().0.0,
            "in_progress"
        );
        assert_eq!(app.world().get::<StageChangedAt>(session).unwrap().0, 1);

        app.world_mut()
            .resource_mut::<Messages<StageChangeRequest>>()
            .write(StageChangeRequest {
                id: SessionId("session-1".into()),
                stage: StageId("done".into()),
            });
        app.update();
        assert_eq!(app.world().get::<Stage>(session).unwrap().0.0, "done");
        assert!(app.world().get::<StageChangedAt>(session).unwrap().0 > 1);
    }

    #[test]
    fn duplicate_creation_requests_create_one_session() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin));
        let request = CreateRequest::new("Task", "", PathBuf::new(), None);
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write_batch([request.clone(), request]);

        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<Session>>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn cleanup_request_targets_the_canonical_session() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, StagePlugin))
            .init_resource::<Cleaned>()
            .add_observer(Cleaned::record);
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into())))
            .id();
        app.world_mut()
            .resource_mut::<Messages<CleanupRequest>>()
            .write(CleanupRequest {
                id: SessionId("session-1".into()),
            });

        app.update();

        assert_eq!(app.world().resource::<Cleaned>().0, vec![session]);
    }

    #[test]
    fn domain_plugin_registers_its_durable_components() {
        let mut app = App::new();
        app.add_plugins(crate::DomainPlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let filter = vmux_ecs::persistence::WorkspacePersisted::filter(&registry);

        assert!(filter.is_allowed::<Session>());
        assert!(filter.is_allowed::<SessionId>());
        assert!(filter.is_allowed::<AgentId>());
        assert!(filter.is_allowed::<LocalTask>());
        assert!(filter.is_allowed::<Stage>());
        assert!(filter.is_allowed::<StageChangedAt>());
    }

    #[test]
    fn activating_a_session_view_updates_the_session_timestamp() {
        let mut app = App::new();
        app.add_plugins(EntityPlugin);
        let session = app.world_mut().spawn((Session, LastActivatedAt(1))).id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();

        app.world_mut().trigger(ActivateRequest { entity: stack });

        assert!(app.world().get::<LastActivatedAt>(session).unwrap().0 > 1);
    }

    #[test]
    fn renamed_session_updates_view_metadata() {
        let mut app = App::new();
        app.add_plugins((EntityPlugin, MetadataPlugin, StagePlugin));
        let session = app
            .world_mut()
            .spawn((Session, SessionId("session-1".into()), Name::new("Before")))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                EntityTarget::<Session>::new(session),
                PageMetadata {
                    url: String::new(),
                    title: String::new(),
                    icon: Default::default(),
                    bg_color: None,
                },
            ))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<Messages<RenameRequest>>()
            .write(RenameRequest {
                id: SessionId("session-1".into()),
                name: "After".into(),
            });
        app.update();

        let metadata = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(metadata.title, "After");
        assert_eq!(metadata.url, "vmux://sessions/session-1");
    }
}
