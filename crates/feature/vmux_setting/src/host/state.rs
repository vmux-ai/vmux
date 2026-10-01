use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_command::{
    BindCommands, CommandDispatch, CommandRegistry, CommandRuntimePlugin, ReadCommandRequests,
    WriteCommandRequests,
};
use vmux_ecs::host::UiState;
#[cfg(test)]
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
use vmux_layout::{
    native_open::HostedUiPlugin,
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};

use crate::event::{CheckForUpdatesEvent, CheckForUpdatesRequest, SettingsRequest};
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
            .add_systems(Startup, bind_command.in_set(BindCommands))
            .add_observer(issue_open_settings)
            .add_message::<CheckForUpdatesRequest>()
            .add_plugins((
                HostedUiPlugin::<Settings>::new(super::SettingsPlugin::MANIFEST),
                UiEventPlugin::<(SettingsRequest, CheckForUpdatesEvent)>::default(),
            ))
            .add_observer(settings_request)
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
#[require(SettingsUiStateUpdates, SettingsSchema, SettingsRenderProjection)]
pub struct Settings;

type SettingsUiStateUpdates = UiState<SettingsUiState>;

#[derive(Message)]
struct OpenSettingsRequest;

#[vmux_command::command(id = "open_settings")]
struct OpenSettingsBinding;

fn bind_command(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<OpenSettingsBinding>(&mut commands);
}

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
    use vmux_layout::native_open::{HostedUiPlugin, NativeOpenPlugin};

    #[test]
    fn settings_page_open_spawns_marker_and_handles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeOpenPlugin)
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
            .add_plugins(NativeOpenPlugin)
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
