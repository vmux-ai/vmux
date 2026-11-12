use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::mcp::{
    McpServerOperation, McpServerSelectionRequest, McpServerStatus, McpServersUiState,
};
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{PageOpenRequest, PageOpenTarget, UiState, UiStateWrite};
use vmux_tool::{ConnectorAccess, ConnectorConnectionCompleted};

use super::session::{SessionView, SessionViews};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.register_stored::<Connectors>()
            .add_plugins(UiEventPlugin::<(McpServerSelectionRequest,)>::default())
            .add_message::<PageOpenRequest>()
            .add_observer(select)
            .add_observer(connected)
            .add_systems(Update, project);
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[require(Save)]
struct Connectors(Vec<String>);

impl Connectors {
    fn toggle(&mut self, id: &str) {
        if self.0.iter().any(|selected| selected == id) {
            self.0.retain(|selected| selected != id);
        } else {
            self.0.push(id.to_string());
        }
    }
}

#[derive(Component)]
struct PendingSelection {
    session: Entity,
    id: String,
}

fn select(
    trigger: On<UiInput<McpServerSelectionRequest>>,
    views: SessionViews,
    states: Query<&UiState<McpServersUiState>>,
    selections: Query<&Connectors>,
    parents: Query<&ChildOf>,
    mut pages: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Some(session) = views.session(webview) else {
        return;
    };
    let Ok(state) = states.get(webview) else {
        return;
    };
    let Some(state) = state.current() else {
        return;
    };
    if state.loading || state.pending.is_some() {
        return;
    }
    let id = &trigger.event().payload.id;
    let Some(server) = state.servers.iter().find(|server| &server.id == id) else {
        return;
    };
    let selected = selections
        .get(session)
        .is_ok_and(|selection| selection.0.iter().any(|selected| selected == id));
    if server.status == McpServerStatus::Configured && !selected {
        return;
    }
    if selected || server.status == McpServerStatus::Connected {
        let mut selection = selections.get(session).cloned().unwrap_or_default();
        selection.toggle(id);
        let mut snapshot = state.clone();
        snapshot.selected = selection.0.clone();
        commands
            .entity(webview)
            .insert(ConnectorAccess(selection.0.clone()));
        commands.entity(session).insert(selection);
        commands.trigger(UiStateWrite::<McpServersUiState>::from_event(
            webview, &snapshot,
        ));
        return;
    }
    commands.entity(webview).insert(PendingSelection {
        session,
        id: id.clone(),
    });
    let target = if let Ok(stack) = parents.get(webview)
        && let Ok(pane) = parents.get(stack.parent())
    {
        PageOpenTarget::NewStackInPane(pane.parent())
    } else {
        PageOpenTarget::NewStack
    };
    pages.write(PageOpenRequest {
        target,
        url: vmux_api::mcp::ConnectorSetupRoute(id.clone()).url(),
        request_id: None,
    });
}

fn connected(
    trigger: On<ConnectorConnectionCompleted>,
    views: SessionViews,
    pending: Query<(Entity, &PendingSelection)>,
    states: Query<&UiState<McpServersUiState>>,
    selections: Query<&Connectors>,
    mut commands: Commands,
) {
    let result = &trigger.event().0;
    if result.operation != McpServerOperation::Connect {
        return;
    }
    for (webview, pending) in &pending {
        if result.id != pending.id {
            continue;
        }
        commands.entity(webview).remove::<PendingSelection>();
        if !result.success || views.session(webview) != Some(pending.session) {
            continue;
        }
        let mut selection = selections.get(pending.session).cloned().unwrap_or_default();
        if !selection.0.contains(&pending.id) {
            selection.0.push(pending.id.clone());
        }
        commands
            .entity(webview)
            .insert(ConnectorAccess(selection.0.clone()));
        if let Ok(state) = states.get(webview)
            && let Some(state) = state.current()
        {
            let mut snapshot = state.clone();
            snapshot.selected = selection.0.clone();
            commands.trigger(UiStateWrite::<McpServersUiState>::from_event(
                webview, &snapshot,
            ));
        }
        commands.entity(pending.session).insert(selection);
    }
}

fn project(
    views: SessionViews,
    targets: Query<
        (
            Entity,
            Option<&ConnectorAccess>,
            Option<&UiState<McpServersUiState>>,
        ),
        With<SessionView>,
    >,
    selections: Query<&Connectors>,
    mut commands: Commands,
) {
    for (webview, access, state) in &targets {
        let selected = views
            .session(webview)
            .and_then(|session| selections.get(session).ok())
            .map(|selection| selection.0.clone())
            .unwrap_or_default();
        if access.is_none_or(|access| access.0 != selected) {
            commands
                .entity(webview)
                .insert(ConnectorAccess(selected.clone()));
        }
        if let Some(state) = state.and_then(UiState::current)
            && state.selected != selected
        {
            let mut state = state.clone();
            state.selected = selected;
            commands.trigger(UiStateWrite::<McpServersUiState>::from_event(
                webview, &state,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::mcp::{McpServerEntry, McpServerResult};
    use vmux_ecs::EntityTarget;

    #[derive(Component)]
    struct Published(Vec<String>);

    fn record(trigger: On<UiStateWrite<McpServersUiState>>, mut commands: Commands) {
        commands
            .entity(trigger.event().webview())
            .insert(Published(trigger.event().update().selected.clone()));
    }

    struct Conversation {
        session: Entity,
        view: Entity,
    }

    impl Conversation {
        fn create(app: &mut App) -> Self {
            let session = app.world_mut().spawn(crate::Session).id();
            let stack = app
                .world_mut()
                .spawn(EntityTarget::<crate::Session>::new(session))
                .id();
            let view = app.world_mut().spawn((SessionView, ChildOf(stack))).id();
            Self { session, view }
        }

        fn publish(&self, app: &mut App, status: McpServerStatus, result: Option<McpServerResult>) {
            let snapshot = McpServersUiState {
                loaded: true,
                servers: vec![McpServerEntry {
                    id: "github".into(),
                    status,
                    ..Default::default()
                }],
                result,
                ..Default::default()
            };
            let mut writer = bevy::ecs::system::SystemState::<
                vmux_ecs::UiStateWriter<McpServersUiState>,
            >::new(app.world_mut());
            writer.get_mut(app.world_mut()).unwrap().restore(
                self.view,
                None,
                Some(snapshot.clone()),
            );
            writer.apply(app.world_mut());
            app.world_mut()
                .trigger(UiStateWrite::<McpServersUiState>::from_event(
                    self.view, &snapshot,
                ));
            if let Some(result) = snapshot.result {
                app.world_mut()
                    .trigger(ConnectorConnectionCompleted(result));
            }
            app.world_mut().flush();
        }

        fn select(&self, app: &mut App) {
            app.world_mut().trigger(UiInput {
                webview: self.view,
                payload: McpServerSelectionRequest {
                    id: "github".into(),
                },
            });
            app.update();
        }

        fn selected(&self, app: &App) -> Vec<String> {
            app.world()
                .get::<ConnectorAccess>(self.view)
                .unwrap()
                .0
                .clone()
        }
    }

    #[test]
    fn selecting_a_connected_account_only_changes_its_conversation() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<McpServersUiState>::default(),
            Plugin,
        ))
        .add_observer(record);
        let first = Conversation::create(&mut app);
        let second = Conversation::create(&mut app);
        first.publish(&mut app, McpServerStatus::Connected, None);
        second.publish(&mut app, McpServerStatus::Connected, None);
        app.world_mut().trigger(UiInput {
            webview: first.view,
            payload: McpServerSelectionRequest {
                id: "github".into(),
            },
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<Published>(first.view).unwrap().0,
            ["github"]
        );
        assert!(
            app.world()
                .get::<Published>(second.view)
                .unwrap()
                .0
                .is_empty()
        );
        app.update();
        assert_eq!(first.selected(&app), ["github"]);
        assert!(second.selected(&app).is_empty());
        first.select(&mut app);
        assert!(first.selected(&app).is_empty());
        assert!(second.selected(&app).is_empty());
    }

    #[test]
    fn cancelled_setup_cannot_attach_on_an_unrelated_later_connection() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<McpServersUiState>::default(),
            Plugin,
        ));
        let chat = Conversation::create(&mut app);
        chat.publish(&mut app, McpServerStatus::Available, None);
        chat.select(&mut app);
        for success in [false, true] {
            app.world_mut()
                .trigger(ConnectorConnectionCompleted(McpServerResult {
                    id: "github".into(),
                    operation: McpServerOperation::Connect,
                    success,
                    message: String::new(),
                }));
            app.update();
            assert!(chat.selected(&app).is_empty());
        }
    }

    #[test]
    fn failed_sign_in_does_not_attach_and_successful_retry_does() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<McpServersUiState>::default(),
            Plugin,
        ));
        let chat = Conversation::create(&mut app);
        let other = Conversation::create(&mut app);
        other.publish(&mut app, McpServerStatus::Available, None);
        chat.publish(&mut app, McpServerStatus::Available, None);
        chat.select(&mut app);
        let pages = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, "vmux://tools/connectors/github/setup");
        chat.publish(
            &mut app,
            McpServerStatus::Available,
            Some(McpServerResult {
                id: "github".into(),
                operation: McpServerOperation::Connect,
                success: false,
                message: "denied".into(),
            }),
        );
        app.update();
        assert!(chat.selected(&app).is_empty());
        chat.select(&mut app);
        chat.publish(
            &mut app,
            McpServerStatus::Connected,
            Some(McpServerResult {
                id: "github".into(),
                operation: McpServerOperation::Connect,
                success: true,
                message: String::new(),
            }),
        );
        app.update();
        assert_eq!(chat.selected(&app), ["github"]);
        assert!(other.selected(&app).is_empty());
        let persisted =
            ron::to_string(app.world().get::<Connectors>(chat.session).unwrap()).unwrap();
        let restored: Connectors = ron::from_str(&persisted).unwrap();
        assert_eq!(restored.0, ["github"]);
    }
}
