use bevy::{picking::Pickable, prelude::*};
use bevy_cef::prelude::*;
use vmux_command::command::{AppCommand, LayoutCommand, WindowCommand};
use vmux_core::PageMetadata;
use vmux_core::page::PageReady;
use vmux_history::{CreatedAt, LastActivatedAt};
use vmux_layout::{
    Browser,
    pane::{Pane, PaneSplit},
    stack::{FocusedStack, stack_bundle},
};

use crate::event::{
    SETTINGS_LIST_EVENT, SETTINGS_PAGE_URL, SETTINGS_SCHEMA_EVENT, SettingsCommandEvent,
    SettingsListEvent, SettingsSchemaEvent,
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
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
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
        .remove::<SettingsSchemaSent>();
}

#[derive(Component)]
pub(crate) struct SettingsListSent;

#[derive(Component)]
pub(crate) struct SettingsSchemaSent;

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
    pending: Query<Entity, (With<Settings>, With<PageReady>, Without<SettingsSchemaSent>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let payload = SettingsSchemaEvent {
        json: serde_json::to_string(&build_settings_schema()).unwrap_or_default(),
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

pub(crate) fn handle_open_settings_command(
    mut reader: MessageReader<AppCommand>,
    focus: Option<Res<FocusedStack>>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut commands: Commands,
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
        let tab = commands
            .spawn((
                stack_bundle(),
                LastActivatedAt::now(),
                CreatedAt::now(),
                ChildOf(pane),
            ))
            .id();
        commands.entity(tab).insert(PageMetadata {
            url: SETTINGS_PAGE_URL.to_string(),
            title: "Settings".to_string(),
            ..default()
        });
        commands.spawn((Settings::new(&mut meshes, &mut webview_mt), ChildOf(tab)));
    }
}

fn build_settings_schema() -> SettingsSchema {
    SettingsSchema {
        sections: vec![
            SectionSpec {
                id: "appearance".to_string(),
                title: "Appearance".to_string(),
                description: None,
                synthetic_keys: vec!["mode".to_string()],
                root_path: "appearance".to_string(),
            },
            SectionSpec {
                id: "general".to_string(),
                title: "General".to_string(),
                description: None,
                synthetic_keys: vec!["auto_update".to_string()],
                root_path: String::new(),
            },
            SectionSpec {
                id: "layout".to_string(),
                title: "Layout".to_string(),
                description: Some("Window CEF shell, panes, sidebar, and focus ring.".to_string()),
                synthetic_keys: vec![],
                root_path: "layout".to_string(),
            },
            SectionSpec {
                id: "shortcuts".to_string(),
                title: "Shortcuts".to_string(),
                description: Some(
                    "Read-only view. Edit settings.ron directly to change bindings.".to_string(),
                ),
                synthetic_keys: vec![],
                root_path: "shortcuts".to_string(),
            },
            SectionSpec {
                id: "terminal".to_string(),
                title: "Terminal".to_string(),
                description: None,
                synthetic_keys: vec![],
                root_path: "terminal".to_string(),
            },
            SectionSpec {
                id: "browser".to_string(),
                title: "Browser".to_string(),
                description: None,
                synthetic_keys: vec!["startup_url".to_string()],
                root_path: "browser".to_string(),
            },
        ],
        fields: vec![
            field(
                "appearance.mode",
                FieldSpec {
                    label: Some("Mode".into()),
                    hint: Some("Color scheme for web pages. Device follows your system.".into()),
                    widget: Some(WidgetKind::Select),
                    options: vec![
                        SelectOption {
                            value: "device".into(),
                            label: "Device".into(),
                        },
                        SelectOption {
                            value: "light".into(),
                            label: "Light".into(),
                        },
                        SelectOption {
                            value: "dark".into(),
                            label: "Dark".into(),
                        },
                    ],
                    ..Default::default()
                },
            ),
            field(
                "auto_update",
                FieldSpec {
                    label: Some("Auto-update".into()),
                    hint: Some("Check for new releases on launch.".into()),
                    ..Default::default()
                },
            ),
            field(
                "browser",
                FieldSpec {
                    order: vec!["startup_url".into()],
                    ..Default::default()
                },
            ),
            field(
                "browser.startup_url",
                FieldSpec {
                    label: Some("Startup URL".into()),
                    hint: Some("Empty opens the command bar prompt.".into()),
                    placeholder: Some("https://example.com".into()),
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
                    label: Some("Window".into()),
                    order: vec!["padding".into()],
                    ..Default::default()
                },
            ),
            field(
                "layout.pane",
                FieldSpec {
                    label: Some("Pane".into()),
                    order: vec!["gap".into(), "radius".into()],
                    ..Default::default()
                },
            ),
            field(
                "layout.side_sheet",
                FieldSpec {
                    label: Some("Side sheet".into()),
                    ..Default::default()
                },
            ),
            field(
                "layout.focus_ring",
                FieldSpec {
                    label: Some("Focus ring".into()),
                    order: vec!["width".into(), "color".into()],
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
                    label: Some("Leader".into()),
                    hint: Some("Prefix key for chord shortcuts.".into()),
                    widget: Some(WidgetKind::LeaderKbd),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.chord_timeout_ms",
                FieldSpec {
                    label: Some("Chord timeout".into()),
                    hint: Some("Milliseconds before a chord prefix expires.".into()),
                    ..Default::default()
                },
            ),
            field(
                "shortcuts.bindings",
                FieldSpec {
                    label: Some("Bindings".into()),
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
                    label: Some("Confirm close".into()),
                    hint: Some("Prompt before closing a terminal with a running process.".into()),
                    ..Default::default()
                },
            ),
            field(
                "terminal.default_theme",
                FieldSpec {
                    label: Some("Default theme".into()),
                    hint: Some("Name of the active theme from the themes list.".into()),
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
}
