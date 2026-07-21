use bevy::{picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_command::command::{AppCommand, LayoutCommand, WindowCommand};
use vmux_core::page::PageReady;
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
use vmux_layout::{
    Browser,
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
    warm_page::WarmPage,
};
use vmux_ui::i18n::{register_catalog, requested_locale, translate_for};

use crate::event::{
    CheckForUpdatesEvent, CheckForUpdatesRequest, CurrentUpdateCheckStatus, SETTINGS_LIST_EVENT,
    SETTINGS_PAGE_URL, SETTINGS_SCHEMA_EVENT, SettingsCommandEvent, SettingsListEvent,
    SettingsSchemaEvent, UPDATE_CHECK_STATUS_EVENT, UpdateCheckStatusEvent,
};
use crate::schema::{FieldSpec, SectionSpec, SelectOption, SettingsSchema, WidgetKind};
use crate::{AppSettings, SettingsWriteRequest, apply_settings_update, serialize_settings_to_json};

#[derive(Component)]
pub struct Settings;

impl Settings {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
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
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

impl WarmPage for Settings {
    const HOST: &'static str = "settings";
    const URL: &'static str = SETTINGS_PAGE_URL;
    const TITLE: &'static str = "Settings";

    fn spawn(
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> Entity {
        commands.spawn(Settings::new(meshes, webview_mt)).id()
    }
}

pub(crate) fn reset_sent_markers_on_page_ready(
    trigger: On<BinReceive<PageReady>>,
    views: Query<Entity, With<Settings>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if !views.contains(entity) {
        return;
    }
    commands
        .entity(entity)
        .remove::<SettingsListSent>()
        .remove::<SettingsSchemaSent>()
        .remove::<UpdateCheckStatusSent>();
}

#[derive(Component)]
pub(crate) struct SettingsListSent;

#[derive(Component)]
pub(crate) struct SettingsSchemaSent;

#[derive(Component)]
pub(crate) struct UpdateCheckStatusSent;

pub(crate) fn broadcast_settings_to_views(
    settings: Res<AppSettings>,
    pending: Query<Entity, (With<Settings>, With<PageReady>, Without<SettingsListSent>)>,
    sent: Query<Entity, (With<Settings>, With<PageReady>, With<SettingsListSent>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let payload = SettingsListEvent {
        json: serialize_settings_to_json(&settings),
    };
    for entity in &pending {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SETTINGS_LIST_EVENT,
            &payload,
        ));
        commands.entity(entity).insert(SettingsListSent);
    }
    if settings.is_changed() {
        for entity in &sent {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SETTINGS_LIST_EVENT,
                &payload,
            ));
        }
    }
}

pub(crate) fn broadcast_schema_to_views(
    settings: Res<AppSettings>,
    pending: Query<Entity, (With<Settings>, With<PageReady>, Without<SettingsSchemaSent>)>,
    sent: Query<Entity, (With<Settings>, With<PageReady>, With<SettingsSchemaSent>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if pending.is_empty() && (!settings.is_changed() || sent.is_empty()) {
        return;
    }
    let locale = requested_locale(Some(&settings.appearance.locale));
    let payload = SettingsSchemaEvent {
        json: serde_json::to_string(&build_settings_schema_for(&locale)).unwrap_or_default(),
    };
    for entity in &pending {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SETTINGS_SCHEMA_EVENT,
            &payload,
        ));
        commands.entity(entity).insert(SettingsSchemaSent);
    }
    if settings.is_changed() {
        for entity in &sent {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SETTINGS_SCHEMA_EVENT,
                &payload,
            ));
        }
    }
}

pub(crate) fn broadcast_update_status_to_views(
    status: Res<CurrentUpdateCheckStatus>,
    pending: Query<
        Entity,
        (
            With<Settings>,
            With<PageReady>,
            Without<UpdateCheckStatusSent>,
        ),
    >,
    sent: Query<Entity, (With<Settings>, With<PageReady>, With<UpdateCheckStatusSent>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let payload = UpdateCheckStatusEvent {
        status: status.0.clone(),
    };
    for entity in &pending {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            UPDATE_CHECK_STATUS_EVENT,
            &payload,
        ));
        commands.entity(entity).insert(UpdateCheckStatusSent);
    }
    if status.is_changed() {
        for entity in &sent {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                UPDATE_CHECK_STATUS_EVENT,
                &payload,
            ));
        }
    }
}

pub(crate) fn on_settings_command(
    trigger: On<BinReceive<SettingsCommandEvent>>,
    mut settings: ResMut<AppSettings>,
    mut writes: MessageWriter<SettingsWriteRequest>,
) {
    let evt = &trigger.event().payload;
    let value: serde_json::Value = match serde_json::from_str(&evt.value) {
        Ok(v) => v,
        Err(e) => {
            bevy::log::warn!("settings: invalid JSON for path {}: {e}", evt.path);
            return;
        }
    };
    match apply_settings_update(settings.as_mut(), &evt.path, value) {
        Ok(ron_bytes) => {
            writes.write(SettingsWriteRequest { ron_bytes });
        }
        Err(e) => bevy::log::warn!("settings: update {} rejected: {}", evt.path, e),
    }
}

pub(crate) fn on_check_for_updates(
    _trigger: On<BinReceive<CheckForUpdatesEvent>>,
    mut requests: MessageWriter<CheckForUpdatesRequest>,
) {
    requests.write(CheckForUpdatesRequest);
}

pub(crate) fn handle_open_settings_command(
    mut reader: MessageReader<AppCommand>,
    focus: Option<Res<FocusedStack>>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for cmd in reader.read() {
        if !matches!(
            *cmd,
            AppCommand::Layout(LayoutCommand::Window(WindowCommand::Settings))
        ) {
            continue;
        }
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
fn build_settings_schema() -> SettingsSchema {
    build_settings_schema_for("en-US")
}

fn build_settings_schema_for(locale: &str) -> SettingsSchema {
    let directory = vmux_core::profile::config_dir().join("locales");
    if let Some(source) = [locale, locale.split('-').next().unwrap_or(locale)]
        .into_iter()
        .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
    {
        let _ = register_catalog(locale, &source);
    }
    let t = |id| translate_for(locale, id);
    SettingsSchema {
        sections: vec![
            SectionSpec {
                id: "appearance".to_string(),
                title: t("schema-appearance"),
                description: None,
                synthetic_keys: vec!["mode".to_string(), "locale".to_string()],
                root_path: "appearance".to_string(),
            },
            SectionSpec {
                id: "general".to_string(),
                title: t("schema-general"),
                description: None,
                synthetic_keys: vec!["auto_update".to_string()],
                root_path: String::new(),
            },
            SectionSpec {
                id: "layout".to_string(),
                title: t("schema-layout"),
                description: Some(t("schema-layout-detail")),
                synthetic_keys: vec![],
                root_path: "layout".to_string(),
            },
            SectionSpec {
                id: "agent".to_string(),
                title: t("schema-agent"),
                description: Some(t("schema-agent-detail")),
                synthetic_keys: vec![],
                root_path: "agent".to_string(),
            },
            SectionSpec {
                id: "shortcuts".to_string(),
                title: t("schema-shortcuts"),
                description: Some(t("schema-shortcuts-detail")),
                synthetic_keys: vec![],
                root_path: "shortcuts".to_string(),
            },
            SectionSpec {
                id: "terminal".to_string(),
                title: t("schema-terminal"),
                description: None,
                synthetic_keys: vec![],
                root_path: "terminal".to_string(),
            },
            SectionSpec {
                id: "browser".to_string(),
                title: t("schema-browser"),
                description: None,
                synthetic_keys: vec![],
                root_path: "browser".to_string(),
            },
        ],
        fields: vec![
            field(
                "appearance.mode",
                FieldSpec {
                    label: Some(t("schema-mode")),
                    hint: Some(t("schema-mode-detail")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SelectOption {
                            value: "device".into(),
                            label: t("schema-device"),
                        },
                        SelectOption {
                            value: "light".into(),
                            label: t("schema-light"),
                        },
                        SelectOption {
                            value: "dark".into(),
                            label: t("schema-dark"),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "appearance.locale",
                FieldSpec {
                    label: Some(t("schema-language")),
                    hint: Some(t("schema-language-detail")),
                    ..Default::default()
                },
            ),
            field(
                "auto_update",
                FieldSpec {
                    label: Some(t("schema-auto-update")),
                    hint: Some(t("schema-auto-update-detail")),
                    ..Default::default()
                },
            ),
            field(
                "browser",
                FieldSpec {
                    order: vec!["startup_url".into(), "search_engine".into()],
                    ..Default::default()
                },
            ),
            field(
                "browser.startup_url",
                FieldSpec {
                    label: Some(t("schema-startup-url")),
                    hint: Some(t("schema-startup-url-detail")),
                    placeholder: Some("https://example.com".into()),
                    ..Default::default()
                },
            ),
            field(
                "browser.search_engine",
                FieldSpec {
                    label: Some(t("schema-search-engine")),
                    hint: Some(t("schema-search-engine-detail")),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SelectOption {
                            value: "google".into(),
                            label: "Google".into(),
                        },
                        SelectOption {
                            value: "bing".into(),
                            label: "Bing".into(),
                        },
                        SelectOption {
                            value: "duckduckgo".into(),
                            label: "DuckDuckGo".into(),
                        },
                        SelectOption {
                            value: "brave".into(),
                            label: "Brave Search".into(),
                        },
                        SelectOption {
                            value: "kagi".into(),
                            label: "Kagi".into(),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "layout",
                FieldSpec {
                    order: vec![
                        "window".into(),
                        "pane".into(),
                        "side_sheet".into(),
                        "focus_ring".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "layout.window",
                FieldSpec {
                    label: Some(t("schema-window")),
                    order: vec!["padding".into()],
                    ..Default::default()
                },
            ),
            field(
                "layout.pane",
                FieldSpec {
                    label: Some(t("schema-pane")),
                    order: vec!["gap".into(), "radius".into()],
                    ..Default::default()
                },
            ),
            field(
                "layout.side_sheet",
                FieldSpec {
                    label: Some(t("schema-side-sheet")),
                    ..Default::default()
                },
            ),
            field(
                "layout.focus_ring",
                FieldSpec {
                    label: Some(t("schema-focus-ring")),
                    order: vec!["width".into(), "color".into()],
                    ..Default::default()
                },
            ),
            field(
                "agent",
                FieldSpec {
                    order: vec![
                        "allow_run_placement_override".into(),
                        "follow_files".into(),
                        "tidy_files".into(),
                        "tidy_files_max".into(),
                        "tidy_files_auto".into(),
                        "app_providers".into(),
                        "acp".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "agent.allow_run_placement_override",
                FieldSpec {
                    label: Some(t("schema-run-placement")),
                    hint: Some(t("schema-run-placement-detail")),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts",
                FieldSpec {
                    order: vec![
                        "chord_timeout_ms".into(),
                        "leader".into(),
                        "bindings".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.leader",
                FieldSpec {
                    label: Some(t("schema-leader")),
                    hint: Some(t("schema-leader-detail")),
                    widget: Some(WidgetKind::LeaderKbd),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.chord_timeout_ms",
                FieldSpec {
                    label: Some(t("schema-chord-timeout")),
                    hint: Some(t("schema-chord-timeout-detail")),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.bindings",
                FieldSpec {
                    label: Some(t("schema-bindings")),
                    widget: Some(WidgetKind::BindingsList),
                    ..Default::default()
                },
            ),
            field(
                "terminal",
                FieldSpec {
                    order: vec![
                        "confirm_close".into(),
                        "default_theme".into(),
                        "themes".into(),
                        "custom_themes".into(),
                    ],
                    ..Default::default()
                },
            ),
            field(
                "terminal.confirm_close",
                FieldSpec {
                    label: Some(t("schema-confirm-close")),
                    hint: Some(t("schema-confirm-close-detail")),
                    ..Default::default()
                },
            ),
            field(
                "terminal.default_theme",
                FieldSpec {
                    label: Some(t("schema-default-theme")),
                    hint: Some(t("schema-default-theme-detail")),
                    placeholder: Some("default".into()),
                    ..Default::default()
                },
            ),
        ],
    }
}

fn field(path: &str, spec: FieldSpec) -> (String, FieldSpec) {
    (path.to_string(), spec)
}

#[cfg(test)]
mod appearance_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_appearance_mode_select() {
        let schema = build_settings_schema();
        assert!(schema.sections.iter().any(|s| s.id == "appearance"));
        let mode = schema.field("appearance.mode").expect("mode field");
        assert_eq!(mode.widget, Some(WidgetKind::Select));
        let vals: Vec<_> = mode.options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(vals, vec!["device", "light", "dark"]);
    }

    #[test]
    fn schema_uses_requested_locale() {
        let schema = build_settings_schema_for("ja");
        let appearance = schema
            .sections
            .iter()
            .find(|section| section.id == "appearance")
            .unwrap();
        assert_eq!(appearance.title, "外観");
        assert_eq!(
            schema.field("appearance.locale").unwrap().label.as_deref(),
            Some("言語")
        );
    }
}

#[cfg(test)]
mod browser_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_search_engine_select() {
        let schema = build_settings_schema();
        let field = schema
            .field("browser.search_engine")
            .expect("search engine field");
        assert_eq!(field.widget, Some(WidgetKind::Select));
        let values: Vec<_> = field
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect();
        assert_eq!(
            values,
            vec!["google", "bing", "duckduckgo", "brave", "kagi"]
        );
    }
}

#[cfg(test)]
mod agent_schema_tests {
    use super::*;

    #[test]
    fn schema_exposes_run_placement_override_under_agent() {
        let schema = build_settings_schema();
        assert!(schema.sections.iter().any(|section| section.id == "agent"));
        let field = schema
            .field("agent.allow_run_placement_override")
            .expect("run placement override field");
        assert_eq!(field.label.as_deref(), Some("Allow run placement override"));
    }
}

#[cfg(test)]
mod page_open_tests {
    use super::*;
    use vmux_core::{PageOpenHandled, PageOpenId, PageOpenTask};
    use vmux_layout::warm_page::WarmPagePlugin;

    #[test]
    fn settings_page_open_spawns_marker_and_handles() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_plugins(WarmPagePlugin::<Settings>::default());
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
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_plugins(WarmPagePlugin::<Settings>::default());
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
