use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};
use vmux_api::mcp::{
    McpServerOperation, McpServerRequest, McpServerSelectionRequest, McpServerStatus,
    McpServersUiState,
};
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{UiState, UiStateWrite};
use vmux_tool::ConnectorAccess;

use super::session::{SessionView, SessionViews};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.register_stored::<Connectors>()
            .add_plugins(UiEventPlugin::<(McpServerSelectionRequest,)>::default())
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
    if server.status == McpServerStatus::Configured {
        return;
    }
    if server.status == McpServerStatus::Connected {
        let mut selection = selections.get(session).cloned().unwrap_or_default();
        selection.toggle(id);
        commands.entity(session).insert(selection);
        return;
    }
    commands.entity(webview).insert(PendingSelection {
        session,
        id: id.clone(),
    });
    commands.trigger(UiInput {
        webview,
        payload: McpServerRequest { id: id.clone() },
    });
}

fn connected(
    trigger: On<UiStateWrite<McpServersUiState>>,
    views: SessionViews,
    pending: Query<&PendingSelection>,
    selections: Query<&Connectors>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview();
    let Ok(pending) = pending.get(webview) else {
        return;
    };
    let state = trigger.event().update();
    let Some(result) = &state.result else {
        if state.loaded
            && !state.loading
            && state.pending.is_none()
            && state.authorization.is_none()
        {
            commands.entity(webview).remove::<PendingSelection>();
        }
        return;
    };
    if result.id != pending.id || result.operation != McpServerOperation::Connect {
        return;
    }
    commands.entity(webview).remove::<PendingSelection>();
    if result.success && views.session(webview) == Some(pending.session) {
        let mut selection = selections.get(pending.session).cloned().unwrap_or_default();
        if !selection.0.contains(&pending.id) {
            selection.0.push(pending.id.clone());
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
        ));
        let first = Conversation::create(&mut app);
        let second = Conversation::create(&mut app);
        first.publish(&mut app, McpServerStatus::Connected, None);
        second.publish(&mut app, McpServerStatus::Connected, None);
        first.select(&mut app);
        assert_eq!(first.selected(&app), ["github"]);
        assert!(second.selected(&app).is_empty());
        first.select(&mut app);
        assert!(first.selected(&app).is_empty());
        assert!(second.selected(&app).is_empty());
    }

    #[test]
    fn failed_sign_in_does_not_attach_and_successful_retry_does() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<McpServersUiState>::default(),
            Plugin,
        ));
        let chat = Conversation::create(&mut app);
        chat.publish(&mut app, McpServerStatus::Available, None);
        chat.select(&mut app);
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
        let persisted =
            ron::to_string(app.world().get::<Connectors>(chat.session).unwrap()).unwrap();
        let restored: Connectors = ron::from_str(&persisted).unwrap();
        assert_eq!(restored.0, ["github"]);
    }
}
