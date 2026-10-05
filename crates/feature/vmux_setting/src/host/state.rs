use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::collections::BTreeMap;
use vmux_command::{
    CommandDispatch, CommandRuntimePlugin, ReadCommandRequests, WriteCommandRequests,
};
use vmux_ecs::UiState;
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_layout::{
    hosted_page::HostedUiPlugin,
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};

use crate::event::{
    CheckForUpdatesEvent, CheckForUpdatesRequest, SettingsEditRequest, SettingsFilterRequest,
    SettingsRequest,
};
use crate::state::SettingsUiState;
use crate::{AppSettings, SettingsSaveRequest};

use super::projection::SettingsRenderProjection;
use crate::schema::SettingsSchema;

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_message::<OpenSettingsRequest>()
            .add_observer(issue_open_settings)
            .add_message::<CheckForUpdatesRequest>()
            .add_plugins((
                HostedUiPlugin::<Settings>::new(super::SettingsPlugin::MANIFEST),
                UiEventPlugin::<(
                    SettingsRequest,
                    SettingsEditRequest,
                    SettingsFilterRequest,
                    CheckForUpdatesEvent,
                )>::default(),
            ))
            .add_observer(settings_request)
            .add_observer(edit)
            .add_observer(filter)
            .add_observer(check_for_updates)
            .add_systems(
                Update,
                handle_open_settings_command
                    .in_set(ReadCommandRequests)
                    .after(WriteCommandRequests),
            );
    }
}

#[derive(Component, Default)]
#[require(
    SettingsUiStateUpdates,
    SettingsSchema,
    SettingsRenderProjection,
    SettingsViewState
)]
pub struct Settings;

type SettingsUiStateUpdates = UiState<SettingsUiState>;

#[derive(Component, Default)]
pub(super) struct SettingsViewState {
    pub(super) query: String,
    pub(super) drafts: BTreeMap<String, String>,
}

#[derive(Message)]
struct OpenSettingsRequest;

#[vmux_command::command]
struct OpenSettingsBinding;

fn issue_open_settings(
    trigger: On<CommandDispatch>,
    registered: Query<(), With<OpenSettingsBinding>>,
    mut requests: MessageWriter<OpenSettingsRequest>,
) {
    if registered.contains(trigger.event().command()) {
        requests.write(OpenSettingsRequest);
    }
}

fn settings_request(
    trigger: On<UiInput<SettingsRequest>>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let evt = &trigger.event().payload;
    let value = match serde_json::Value::try_from(&evt.value) {
        Ok(v) => v,
        Err(e) => {
            bevy::log::warn!("settings: invalid value for path {}: {e}", evt.path);
            return;
        }
    };
    match settings.apply_update(&evt.path, value) {
        Ok(()) => {
            saves.write(SettingsSaveRequest);
        }
        Err(e) => bevy::log::warn!("settings: update {} rejected: {}", evt.path, e),
    }
}

fn edit(
    trigger: On<UiInput<SettingsEditRequest>>,
    mut views: Query<(&mut SettingsViewState, &SettingsRenderProjection), With<Settings>>,
    mut settings: ResMut<AppSettings>,
    mut saves: MessageWriter<SettingsSaveRequest>,
) {
    let event = trigger.event();
    let Ok((mut view, projection)) = views.get_mut(event.webview) else {
        return;
    };
    let path = &event.payload.path;
    let draft = &event.payload.draft;
    view.drafts.insert(path.clone(), draft.clone());
    let Some(field) = projection.field(path) else {
        return;
    };
    let value = match &field.kind {
        crate::state::SettingsRenderFieldKind::Integer { .. } => {
            let Ok(value) = draft.parse::<u64>() else {
                return;
            };
            serde_json::json!(value)
        }
        crate::state::SettingsRenderFieldKind::Number { .. } => {
            let Ok(value) = draft.parse::<f64>() else {
                return;
            };
            serde_json::json!(value)
        }
        crate::state::SettingsRenderFieldKind::Text { .. } => serde_json::json!(draft),
        _ => return,
    };
    match settings.apply_update(path, value) {
        Ok(()) => {
            saves.write(SettingsSaveRequest);
        }
        Err(error) => bevy::log::warn!("settings: update {path} rejected: {error}"),
    }
}

fn filter(
    trigger: On<UiInput<SettingsFilterRequest>>,
    mut views: Query<&mut SettingsViewState, With<Settings>>,
) {
    let Ok(mut view) = views.get_mut(trigger.event().webview) else {
        return;
    };
    view.query.clone_from(&trigger.event().payload.query);
}

fn check_for_updates(
    _trigger: On<UiInput<CheckForUpdatesEvent>>,
    mut requests: MessageWriter<CheckForUpdatesRequest>,
) {
    requests.write(CheckForUpdatesRequest);
}

fn handle_open_settings_command(
    mut reader: MessageReader<OpenSettingsRequest>,
    focus: FocusedStack,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for _ in reader.read() {
        let Some(focus) = focus.as_ref() else {
            continue;
        };
        let Some(pane) = focus.pane.filter(|p| panes.contains(*p)) else {
            continue;
        };
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::NewStackInPane(pane),
            url: crate::SettingsPlugin::URL.to_string(),
            request_id: None,
        });
    }
}

#[cfg(test)]
mod page_open_tests {
    use super::*;
    use vmux_ecs::{PageOpenHandled, PageOpenId, PageOpenTask};
    use vmux_layout::hosted_page::{HostedPagePlugin, HostedUiPlugin};

    #[test]
    fn settings_page_open_spawns_marker_and_handles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(HostedPagePlugin)
            .add_plugins(HostedUiPlugin::<Settings>::new(
                super::super::SettingsPlugin::MANIFEST,
            ));
        let stack = app.world_mut().spawn_empty().id();
        let claimed = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: crate::SettingsPlugin::URL.to_string(),
                request_id: None,
            })
            .id();
        let decoy = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://history/".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(claimed).is_some());
        assert!(app.world().get::<PageOpenHandled>(decoy).is_none());
        let mut q = app.world_mut().query_filtered::<(), With<Settings>>();
        assert_eq!(q.iter(app.world()).count(), 1);
    }

    #[test]
    fn settings_page_open_dedupes_per_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(HostedPagePlugin)
            .add_plugins(HostedUiPlugin::<Settings>::new(
                super::super::SettingsPlugin::MANIFEST,
            ));
        let stack = app.world_mut().spawn_empty().id();
        for _ in 0..2 {
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: crate::SettingsPlugin::URL.to_string(),
                request_id: None,
            });
        }
        app.update();
        let mut q = app.world_mut().query_filtered::<(), With<Settings>>();
        assert_eq!(q.iter(app.world()).count(), 1);
    }
}
