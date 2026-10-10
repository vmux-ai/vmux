#[cfg(host)]
use bevy_app::{App, Plugin, PostUpdate};
#[cfg(not(host))]
use bevy_ecs::prelude::Resource;
#[cfg(host)]
use bevy_ecs::prelude::*;
#[cfg(host)]
use bevy_ecs::system::SystemParam;
#[cfg(host)]
use vmux_ecs::{CreatedAt, Cwd, Description, LastActivatedAt, Order, Terminal};

#[cfg(host)]
use crate::{
    AgentId, RunState, Session, SessionId, Stage, StageChangedAt, StageDefinition, StageId,
};

#[vmux_api::contract(Default, Eq)]
#[derive(Resource)]
pub struct CatalogSnapshot {
    pub stages: Vec<StageSummary>,
    pub sessions: Vec<SessionSummary>,
}

#[vmux_api::contract(Default, Eq)]
pub struct StageSummary {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub terminal: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cwd: String,
    pub stage: String,
    pub created_at: i64,
    pub last_activated_at: i64,
    pub stage_changed_at: i64,
    pub agent: String,
    pub runtime: String,
}

#[cfg(host)]
pub(crate) struct CatalogPlugin;

#[cfg(host)]
impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CatalogSnapshot>()
            .add_systems(PostUpdate, project);
    }
}

#[cfg(host)]
type SessionCatalog<'w, 's> = Query<
    'w,
    's,
    (
        &'static SessionId,
        &'static Name,
        &'static Description,
        &'static Cwd,
        &'static Stage,
        &'static CreatedAt,
        &'static LastActivatedAt,
        &'static StageChangedAt,
        Option<&'static AgentId>,
        Option<&'static RunState>,
    ),
    With<Session>,
>;

#[cfg(host)]
type ChangedSessions<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<Session>,
        Or<(
            Changed<SessionId>,
            Changed<Name>,
            Changed<Description>,
            Changed<Cwd>,
            Changed<Stage>,
            Changed<CreatedAt>,
            Changed<LastActivatedAt>,
            Changed<StageChangedAt>,
            Changed<AgentId>,
            Changed<RunState>,
        )>,
    ),
>;

#[cfg(host)]
type ChangedStages<'w, 's> = Query<
    'w,
    's,
    (),
    (
        With<StageDefinition>,
        Or<(
            Changed<StageId>,
            Changed<Name>,
            Changed<Order>,
            Changed<Terminal>,
        )>,
    ),
>;

#[cfg(host)]
#[derive(SystemParam)]
struct CatalogChanges<'w, 's> {
    sessions: ChangedSessions<'w, 's>,
    stages: ChangedStages<'w, 's>,
    removed_sessions: RemovedComponents<'w, 's, Session>,
    removed_ids: RemovedComponents<'w, 's, SessionId>,
    removed_names: RemovedComponents<'w, 's, Name>,
    removed_descriptions: RemovedComponents<'w, 's, Description>,
    removed_cwds: RemovedComponents<'w, 's, Cwd>,
    removed_stages: RemovedComponents<'w, 's, Stage>,
    removed_created_at: RemovedComponents<'w, 's, CreatedAt>,
    removed_activated_at: RemovedComponents<'w, 's, LastActivatedAt>,
    removed_stage_changed_at: RemovedComponents<'w, 's, StageChangedAt>,
    removed_stage_definitions: RemovedComponents<'w, 's, StageDefinition>,
    removed_stage_ids: RemovedComponents<'w, 's, StageId>,
    removed_orders: RemovedComponents<'w, 's, Order>,
    removed_terminal: RemovedComponents<'w, 's, Terminal>,
    removed_agents: RemovedComponents<'w, 's, AgentId>,
    removed_states: RemovedComponents<'w, 's, RunState>,
}

#[cfg(host)]
impl CatalogChanges<'_, '_> {
    fn any(&mut self) -> bool {
        let mut any = !self.sessions.is_empty() || !self.stages.is_empty();
        any |= self.removed_sessions.read().count() > 0;
        any |= self.removed_ids.read().count() > 0;
        any |= self.removed_names.read().count() > 0;
        any |= self.removed_descriptions.read().count() > 0;
        any |= self.removed_cwds.read().count() > 0;
        any |= self.removed_stages.read().count() > 0;
        any |= self.removed_created_at.read().count() > 0;
        any |= self.removed_activated_at.read().count() > 0;
        any |= self.removed_stage_changed_at.read().count() > 0;
        any |= self.removed_stage_definitions.read().count() > 0;
        any |= self.removed_stage_ids.read().count() > 0;
        any |= self.removed_orders.read().count() > 0;
        any |= self.removed_terminal.read().count() > 0;
        any |= self.removed_agents.read().count() > 0;
        any |= self.removed_states.read().count() > 0;
        any
    }
}

#[cfg(host)]
fn project(
    sessions: SessionCatalog,
    stages: Query<(&StageId, &Name, &Order, Has<Terminal>), With<StageDefinition>>,
    mut changes: CatalogChanges,
    mut catalog: ResMut<CatalogSnapshot>,
) {
    if !catalog.is_added() && !changes.any() {
        return;
    }
    let mut next = CatalogSnapshot {
        stages: stages
            .iter()
            .map(|(id, name, order, terminal)| StageSummary {
                id: id.0.clone(),
                name: name.as_str().to_string(),
                order: order.0,
                terminal,
            })
            .collect(),
        sessions: sessions
            .iter()
            .map(
                |(
                    id,
                    name,
                    description,
                    cwd,
                    stage,
                    created_at,
                    last_activated_at,
                    stage_changed_at,
                    agent,
                    runtime,
                )| SessionSummary {
                    id: id.0.clone(),
                    name: name.as_str().to_string(),
                    description: description.0.clone(),
                    cwd: cwd.0.to_string_lossy().into_owned(),
                    stage: stage.0.0.clone(),
                    created_at: created_at.0,
                    last_activated_at: last_activated_at.0,
                    stage_changed_at: stage_changed_at.0,
                    agent: agent.map(|agent| agent.0.clone()).unwrap_or_default(),
                    runtime: runtime
                        .map(RunState::status)
                        .unwrap_or("inactive")
                        .to_string(),
                },
            )
            .collect(),
    };
    next.stages.sort_by_key(|stage| stage.order);
    next.sessions
        .sort_by_key(|session| std::cmp::Reverse(session.last_activated_at));
    if *catalog != next {
        *catalog = next;
    }
}

#[cfg(all(test, host))]
mod tests {
    use super::*;
    use crate::{CreateRequest, DomainPlugin, RenameRequest};

    #[test]
    fn catalog_tracks_session_metadata() {
        let mut app = App::new();
        app.add_plugins(DomainPlugin);
        let request = CreateRequest::new("Task", "Description", "/tmp/project".into(), None);
        let id = request.id().clone();
        app.world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .write(request);
        app.update();

        let catalog = app.world().resource::<CatalogSnapshot>();
        assert_eq!(catalog.sessions.len(), 1);
        assert_eq!(catalog.sessions[0].id, id.0);
        assert_eq!(catalog.sessions[0].name, "Task");
        assert_eq!(catalog.sessions[0].description, "Description");
        assert_eq!(catalog.sessions[0].cwd, "/tmp/project");

        app.world_mut()
            .resource_mut::<Messages<RenameRequest>>()
            .write(RenameRequest {
                id,
                name: "Renamed".into(),
            });
        app.update();

        assert_eq!(
            app.world().resource::<CatalogSnapshot>().sessions[0].name,
            "Renamed"
        );
    }
}
