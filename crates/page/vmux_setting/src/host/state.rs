use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_layout::{
    Browser,
    native_open::HostedPage,
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};

use crate::event::{
    CheckForUpdatesEvent, CheckForUpdatesRequest, SETTINGS_PAGE_URL, SettingsRequest,
};
use crate::state::SettingsUiState;
use crate::{AppSettings, SettingsWriteRequest};
use vmux_flex::prelude::*;

use super::projection::{SettingsRenderProjection, SettingsSchemaProjection};

pub(super) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_command::CommandRuntimePlugin>() {
            app.add_plugins(vmux_command::CommandRuntimePlugin);
        }
        app.add_message::<OpenSettingsRequest>()
            .add_systems(
                Startup,
                spawn_open_settings_command.in_set(vmux_command::RegisterCommandDefinitions),
            )
            .add_observer(issue_open_settings)
            .add_message::<CheckForUpdatesRequest>()
            .add_plugins((
                vmux_layout::native_open::HostedPagePlugin::<Settings>::default(),
                UiEventPlugin::<(SettingsRequest, CheckForUpdatesEvent)>::default(),
            ))
            .add_observer(on_settings_request)
            .add_observer(on_check_for_updates)
            .add_systems(
                Update,
                handle_open_settings_command
                    .in_set(vmux_command::ReadCommandRequests)
                    .after(vmux_command::WriteCommandRequests),
            );
    }
}

#[derive(Component, Default)]
#[require(
    SettingsUiStateUpdates,
    SettingsSchemaProjection,
    SettingsRenderProjection
)]
pub struct Settings;

type SettingsUiStateUpdates = vmux_core::host::UiState<SettingsUiState>;

#[derive(Message)]
struct OpenSettingsRequest;

#[derive(Component)]
struct OpenSettingsBinding;

fn spawn_open_settings_command(mut commands: Commands) {
    let mut definitions = vmux_command::CommandDefinitions::from_ron(include_str!("state.ron"));
    commands.spawn((definitions.take("open_settings"), OpenSettingsBinding));
    definitions.assert_all_registered();
}

fn issue_open_settings(
    trigger: On<vmux_command::CommandDispatch>,
    registered: Query<(), With<OpenSettingsBinding>>,
    mut requests: MessageWriter<OpenSettingsRequest>,
) {
    if registered.contains(trigger.event().command()) {
        requests.write(OpenSettingsRequest);
    }
}

impl Settings {
    pub fn new() -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SETTINGS_PAGE_URL),
                ResolvedWebviewUri(SETTINGS_PAGE_URL.to_string()),
                PageMetadata {
                    title: "Settings".to_string(),
                    url: SETTINGS_PAGE_URL.to_string(),
                    icon: vmux_core::PageIcon::None,
                    bg_color: None,
                },
            ),
            (
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Visible,
            ),
        )
    }
}

impl HostedPage for Settings {
    const HOST: &'static str = "settings";
    const URL: &'static str = SETTINGS_PAGE_URL;
    const TITLE: &'static str = "Settings";
}

fn on_settings_request(
    trigger: On<UiInput<SettingsRequest>>,
    mut settings: ResMut<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
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
        Ok(ron_bytes) => {
            writes.write(SettingsWriteRequest { ron_bytes });
        }
        Err(e) => bevy::log::warn!("settings: update {} rejected: {}", evt.path, e),
    }
}

fn on_check_for_updates(
    _trigger: On<UiInput<CheckForUpdatesEvent>>,
    mut requests: MessageWriter<CheckForUpdatesRequest>,
) {
    requests.write(CheckForUpdatesRequest);
}

fn handle_open_settings_command(
    mut reader: MessageReader<OpenSettingsRequest>,
    focus: Option<Res<FocusedStack>>,
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
            url: SETTINGS_PAGE_URL.to_string(),
            request_id: None,
        });
    }
}

#[cfg(test)]
mod page_open_tests {
    use super::*;
    use vmux_core::{PageOpenHandled, PageOpenId, PageOpenTask};
    use vmux_layout::native_open::{HostedPagePlugin, NativeOpenPlugin};

    #[test]
    fn settings_page_open_spawns_marker_and_handles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(NativeOpenPlugin)
            .add_plugins(HostedPagePlugin::<Settings>::default());
        let stack = app.world_mut().spawn_empty().id();
        let claimed = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: SETTINGS_PAGE_URL.to_string(),
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
            .add_plugins(HostedPagePlugin::<Settings>::default());
        let stack = app.world_mut().spawn_empty().id();
        for _ in 0..2 {
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: SETTINGS_PAGE_URL.to_string(),
                request_id: None,
            });
        }
        app.update();
        let mut q = app.world_mut().query_filtered::<(), With<Settings>>();
        assert_eq!(q.iter(app.world()).count(), 1);
    }
}
