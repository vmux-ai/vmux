use bevy_app::{App, Plugin, PostUpdate};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use vmux_ecs::host::UiStateWrite;
use vmux_ecs::{
    CreatedAt, Cwd, Description, LastActivatedAt, Order, PageOpenRequest, PageOpenTarget,
};
use vmux_session::{
    AgentId, CatalogSnapshot, RunState, Session, SessionCleanupRequest, SessionCreateRequest,
    SessionDescriptionUpdateRequest, SessionId, SessionRenameRequest, SessionStageChangeRequest,
    SessionSummary, Stage, StageChangedAt, StageDefinition, StageId, StageSummary,
};

use crate::event::{
    SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate, SessionsRename, SessionsStageChange,
};
use crate::state::ChatUiState;

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            SessionsCreate,
            SessionsRename,
            SessionsDescriptionUpdate,
            SessionsStageChange,
            SessionsCleanup,
        )>::default())
            .add_observer(create)
            .add_observer(rename)
            .add_observer(update_description)
            .add_observer(change_stage)
            .add_observer(cleanup)
            .add_systems(PostUpdate, project);
    }
}

fn create(
    trigger: On<UiInput<SessionsCreate>>,
    parents: Query<&ChildOf>,
    mut requests: MessageWriter<SessionCreateRequest>,
    mut opens: MessageWriter<PageOpenRequest>,
) {
    let request = &trigger.event().payload;
    let id = SessionId(request.id.clone());
    requests.write(SessionCreateRequest {
        id: id.clone(),
        name: request.name.clone(),
        description: request.description.clone(),
        cwd: std::path::PathBuf::new(),
        agent: None,
    });
    if let Ok(parent) = parents.get(trigger.event().webview) {
        opens.write(PageOpenRequest {
            target: PageOpenTarget::Stack(parent.parent()),
            url: vmux_session::Route::Session(id).url(),
            request_id: None,
        });
    }
}

fn rename(trigger: On<UiInput<SessionsRename>>, mut requests: MessageWriter<SessionRenameRequest>) {
    let request = &trigger.event().payload;
    requests.write(SessionRenameRequest {
        id: SessionId(request.id.clone()),
        name: request.name.clone(),
    });
}

fn update_description(
    trigger: On<UiInput<SessionsDescriptionUpdate>>,
    mut requests: MessageWriter<SessionDescriptionUpdateRequest>,
) {
    let request = &trigger.event().payload;
    requests.write(SessionDescriptionUpdateRequest {
        id: SessionId(request.id.clone()),
        description: request.description.clone(),
    });
}

fn change_stage(
    trigger: On<UiInput<SessionsStageChange>>,
    mut requests: MessageWriter<SessionStageChangeRequest>,
) {
    let request = &trigger.event().payload;
    requests.write(SessionStageChangeRequest {
        id: SessionId(request.id.clone()),
        stage: StageId(request.stage.clone()),
    });
}

fn cleanup(
    trigger: On<UiInput<SessionsCleanup>>,
    mut requests: MessageWriter<SessionCleanupRequest>,
) {
    requests.write(SessionCleanupRequest {
        id: SessionId(trigger.event().payload.id.clone()),
    });
}

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

fn project(
    sessions: SessionCatalog,
    stages: Query<
        (&StageId, &Name, &Order, Has<vmux_ecs::component::Terminal>),
        With<StageDefinition>,
    >,
    targets: Query<(Entity, Ref<super::session::ChatView>)>,
    mut previous: Local<Option<CatalogSnapshot>>,
    mut commands: Commands,
) {
    let mut snapshot = CatalogSnapshot {
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
    snapshot.stages.sort_by_key(|stage| stage.order);
    snapshot
        .sessions
        .sort_by_key(|session| std::cmp::Reverse(session.last_activated_at));
    let target_added = targets.iter().any(|(_, view)| view.is_added());
    if !target_added && previous.as_ref() == Some(&snapshot) {
        return;
    }
    *previous = Some(snapshot.clone());
    for (target, _) in &targets {
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, &snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_requests_a_session_and_opens_its_canonical_route() {
        let mut app = App::new();
        app.add_message::<SessionCreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(create);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                id: "session-1".into(),
                name: "Task".into(),
                description: "Description".into(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<SessionCreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].id.0, "session-1");
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, "vmux://sessions/session-1");
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(entity) if entity == stack
        ));
    }
}
