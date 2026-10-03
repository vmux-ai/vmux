use std::collections::HashMap;

use bevy_app::{App, Plugin, PostUpdate};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::host::{UiState, UiStateWrite};
use vmux_ecs::page::PageReady;
use vmux_ecs::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_session::{
    AgentId, CatalogSnapshot, CleanupRequest, CreateRequest, Created, DescriptionUpdateRequest,
    RenameRequest, Route, SessionId, StageChangeRequest, StageId,
};

use crate::event::{
    SessionsCleanup, SessionsCreate, SessionsDescriptionUpdate, SessionsRename, SessionsStageChange,
};
use crate::state::ChatUiState;

pub(super) struct CatalogPlugin;

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingOpens>()
            .init_resource::<CatalogSnapshot>()
            .add_plugins(UiEventPlugin::<(
                SessionsCreate,
                SessionsRename,
                SessionsDescriptionUpdate,
                SessionsStageChange,
                SessionsCleanup,
            )>::default())
            .add_observer(create)
            .add_observer(open_created)
            .add_observer(rename)
            .add_observer(update_description)
            .add_observer(change_stage)
            .add_observer(cleanup)
            .add_observer(ready)
            .add_systems(PostUpdate, publish);
    }
}

#[derive(Component)]
#[require(SessionManagerUiState)]
pub struct SessionManagerView;

type SessionManagerUiState = UiState<ChatUiState>;

#[derive(Resource, Default)]
struct PendingOpens(HashMap<SessionId, Entity>);

#[derive(SystemParam)]
struct ManagerViews<'w, 's> {
    views: Query<'w, 's, Option<&'static PageMetadata>, With<SessionManagerView>>,
}

impl ManagerViews<'_, '_> {
    fn contains(&self, entity: Entity) -> bool {
        self.views.contains(entity)
    }

    fn agent(&self, entity: Entity) -> Option<AgentId> {
        self.views
            .get(entity)
            .ok()
            .flatten()
            .and_then(|metadata| Route::requested_agent(&metadata.url))
    }
}

fn create(
    trigger: On<UiInput<SessionsCreate>>,
    parents: Query<&ChildOf>,
    managers: ManagerViews,
    mut pending: ResMut<PendingOpens>,
    mut requests: MessageWriter<CreateRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    let Ok(parent) = parents.get(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    let request = CreateRequest::new(
        request.name.clone(),
        request.description.clone(),
        std::path::PathBuf::new(),
        managers.agent(trigger.event().webview),
    );
    pending.0.insert(request.id().clone(), parent.parent());
    requests.write(request);
}

fn open_created(
    trigger: On<Created>,
    mut pending: ResMut<PendingOpens>,
    mut opens: MessageWriter<PageOpenRequest>,
) {
    let event = trigger.event();
    let Some(stack) = pending.0.remove(&event.id) else {
        return;
    };
    opens.write(PageOpenRequest {
        target: PageOpenTarget::Stack(stack),
        url: vmux_session::Route::Session(event.id.clone()).url(),
        request_id: None,
    });
}

fn rename(
    trigger: On<UiInput<SessionsRename>>,
    managers: ManagerViews,
    mut requests: MessageWriter<RenameRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(RenameRequest {
        id: SessionId(request.id.clone()),
        name: request.name.clone(),
    });
}

fn update_description(
    trigger: On<UiInput<SessionsDescriptionUpdate>>,
    managers: ManagerViews,
    mut requests: MessageWriter<DescriptionUpdateRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(DescriptionUpdateRequest {
        id: SessionId(request.id.clone()),
        description: request.description.clone(),
    });
}

fn change_stage(
    trigger: On<UiInput<SessionsStageChange>>,
    managers: ManagerViews,
    mut requests: MessageWriter<StageChangeRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    let request = &trigger.event().payload;
    requests.write(StageChangeRequest {
        id: SessionId(request.id.clone()),
        stage: StageId(request.stage.clone()),
    });
}

fn cleanup(
    trigger: On<UiInput<SessionsCleanup>>,
    managers: ManagerViews,
    mut requests: MessageWriter<CleanupRequest>,
) {
    if !managers.contains(trigger.event().webview) {
        return;
    }
    requests.write(CleanupRequest {
        id: SessionId(trigger.event().payload.id.clone()),
    });
}

fn publish(
    catalog: Res<CatalogSnapshot>,
    targets: Query<Entity, (With<SessionManagerView>, With<PageReady>)>,
    mut commands: Commands,
) {
    if !catalog.is_changed() {
        return;
    }
    for target in &targets {
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, &*catalog));
    }
}

fn ready(
    trigger: On<UiInput<PageReady>>,
    managers: ManagerViews,
    catalog: Res<CatalogSnapshot>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    if managers.contains(target) {
        commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, &*catalog));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Published(Vec<Entity>);

    impl Published {
        fn record(trigger: On<UiStateWrite<ChatUiState>>, mut published: ResMut<Published>) {
            if trigger.event().patch().sessions.is_some() {
                published.0.push(trigger.event().webview());
            }
        }
    }

    #[test]
    fn create_requests_a_session_and_opens_its_canonical_route() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_message::<PageOpenRequest>()
            .add_observer(create)
            .add_observer(open_created);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app
            .world_mut()
            .spawn((ChildOf(stack), SessionManagerView))
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: "Description".into(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 1);
        let id = created[0].id().clone();
        assert!(!id.0.is_empty());
        let session = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Created {
            entity: session,
            id: id.clone(),
        });
        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].url, vmux_session::Route::Session(id).url());
        assert!(matches!(
            opened[0].target,
            PageOpenTarget::Stack(entity) if entity == stack
        ));
    }

    #[test]
    fn session_view_cannot_create_sessions() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_observer(create);
        let webview = app.world_mut().spawn(super::super::session::ChatView).id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: String::new(),
            },
        });

        assert!(
            app.world_mut()
                .resource_mut::<Messages<CreateRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn manager_creation_uses_the_requested_agent() {
        let mut app = App::new();
        app.init_resource::<PendingOpens>()
            .add_message::<CreateRequest>()
            .add_observer(create);
        let stack = app.world_mut().spawn_empty().id();
        let webview = app
            .world_mut()
            .spawn((
                SessionManagerView,
                ChildOf(stack),
                PageMetadata {
                    url: Route::manager_for_agent(&AgentId("codex".into())),
                    title: String::new(),
                    icon: Default::default(),
                    bg_color: None,
                },
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: SessionsCreate {
                name: "Task".into(),
                description: String::new(),
            },
        });

        let created = app
            .world_mut()
            .resource_mut::<Messages<CreateRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(created[0].agent, Some(AgentId("codex".into())));
    }

    #[test]
    fn page_ready_republishes_catalog_only_to_manager_views() {
        let mut app = App::new();
        app.init_resource::<CatalogSnapshot>()
            .init_resource::<Published>()
            .add_observer(Published::record)
            .add_observer(ready);
        let manager = app.world_mut().spawn(SessionManagerView).id();
        let session = app.world_mut().spawn(super::super::session::ChatView).id();

        app.world_mut().trigger(UiInput {
            webview: session,
            payload: PageReady {},
        });
        app.world_mut().trigger(UiInput {
            webview: manager,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert_eq!(app.world().resource::<Published>().0, vec![manager]);
    }
}
