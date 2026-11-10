use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::collections::BTreeMap;
use vmux_command::{CommandDispatch, CommandRuntimePlugin};
use vmux_ecs::UiState;
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;
use vmux_layout::{
    hosted_page::HostedUiPlugin,
    stack::{OpenPolicy, OpenRequest},
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
        app.add_message::<OpenRequest>()
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
            .add_observer(check_for_updates);
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

#[vmux_command::command]
struct OpenSettingsBinding;

fn issue_open_settings(
    trigger: On<CommandDispatch>,
    registered: Query<(), With<OpenSettingsBinding>>,
    mut requests: MessageWriter<OpenRequest>,
) {
    if registered.contains(trigger.event().command()) {
        requests.write(OpenRequest {
            url: Some(crate::SettingsPlugin::URL.to_string()),
            policy: OpenPolicy::FocusReusable,
        });
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

#[cfg(test)]
mod page_open_tests {
    use super::*;
    use vmux_ecs::{
        Active, PageMetadata, PageNavigation, PageOpenHandled, PageOpenId, PageOpenRequest,
        PageOpenTarget, PageOpenTask, StackCommandSet,
    };
    use vmux_layout::hosted_page::{HostedPagePlugin, HostedUiPlugin};
    use vmux_layout::pane::Pane;
    use vmux_layout::space::{CurrentSpace, Space};
    use vmux_layout::stack::{Stack, StackPlugin};
    use vmux_layout::tab::{CloseTabRequest, Tab};

    struct CommandFixture {
        app: App,
        existing: Option<Entity>,
    }

    impl CommandFixture {
        fn new(existing_url: Option<&str>) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin, StackPlugin))
                .add_message::<vmux_ecs::LauncherDismissRequest>()
                .add_message::<CloseTabRequest>()
                .add_message::<SettingsSaveRequest>()
                .init_resource::<AppSettings>()
                .init_resource::<vmux_layout::settings::LayoutSettings>()
                .configure_sets(
                    Update,
                    StackCommandSet.in_set(vmux_command::ReadCommandRequests),
                );
            let space = app.world_mut().spawn((Space, CurrentSpace, Active)).id();
            let tab = app
                .world_mut()
                .spawn((Tab::default(), vmux_ecs::LastActivatedAt(2), ChildOf(space)))
                .id();
            let pane = app
                .world_mut()
                .spawn((Pane, vmux_ecs::LastActivatedAt(2), ChildOf(tab)))
                .id();
            app.world_mut().spawn((
                Stack::default(),
                vmux_ecs::LastActivatedAt(2),
                PageMetadata {
                    url: "https://example.com/".to_string(),
                    ..Default::default()
                },
                ChildOf(pane),
            ));
            let mut existing = None;
            if let Some(url) = existing_url {
                existing = Some(
                    app.world_mut()
                        .spawn((
                            Stack::default(),
                            vmux_ecs::LastActivatedAt(1),
                            PageMetadata {
                                url: url.to_string(),
                                ..Default::default()
                            },
                            ChildOf(pane),
                        ))
                        .id(),
                );
            }
            app.update();
            Self { app, existing }
        }

        fn open(&mut self, count: usize) -> Vec<PageOpenRequest> {
            let caller = self.app.world_mut().spawn_empty().id();
            for _ in 0..count {
                self.app
                    .world_mut()
                    .write_message(vmux_command::CommandInvocation::new(
                        caller,
                        "open_settings",
                    ));
            }
            self.app.update();
            self.app
                .world_mut()
                .resource_mut::<Messages<PageOpenRequest>>()
                .drain()
                .collect()
        }

        fn settings(&mut self) -> Vec<(Entity, String)> {
            let mut pages = Vec::new();
            let mut query = self
                .app
                .world_mut()
                .query_filtered::<(Entity, &PageMetadata), With<Stack>>();
            for (entity, metadata) in query.iter(self.app.world()) {
                if metadata.url.split('#').next() == Some(crate::SettingsPlugin::URL) {
                    pages.push((entity, metadata.url.clone()));
                }
            }
            pages
        }
    }

    #[test]
    fn repeated_settings_commands_reuse_the_page_and_preserve_its_section() {
        for existing_url in [
            None,
            Some("vmux://settings/"),
            Some("vmux://settings/#appearance"),
        ] {
            let mut fixture = CommandFixture::new(existing_url);
            let opens = fixture.open(3);
            let pages = fixture.settings();
            assert_eq!(pages.len(), 1);
            let (settings, url) = &pages[0];
            if let Some(existing) = fixture.existing {
                assert!(opens.is_empty());
                assert_eq!(*settings, existing);
                assert_eq!(url, existing_url.unwrap());
            } else {
                assert!(!opens.is_empty());
                for request in opens {
                    assert!(
                        matches!(request.target, PageOpenTarget::Stack(target) if target == *settings)
                    );
                }
            }
            assert!(
                fixture
                    .app
                    .world()
                    .get::<vmux_ecs::LastActivatedAt>(*settings)
                    .unwrap()
                    .0
                    > 2
            );
            assert!(fixture.open(1).is_empty());
            assert_eq!(fixture.settings(), pages);
        }
    }

    #[test]
    fn settings_page_open_spawns_marker_and_handles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(HostedPagePlugin)
            .add_plugins(HostedUiPlugin::<Settings>::new(
                super::super::SettingsPlugin::MANIFEST,
            ));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        let claimed = app
            .world_mut()
            .spawn(PageOpenTask {
                id: navigation,
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
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        for _ in 0..2 {
            app.world_mut().spawn(PageOpenTask {
                id: navigation,
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
