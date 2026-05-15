use std::path::PathBuf;

use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_settings::event::{
    SETTINGS_LIST_EVENT, SETTINGS_WEBVIEW_URL, SettingsCommandEvent, SettingsListEvent,
};
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::{
    browser::Browser,
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
    settings::{AppSettings, SettingsWriteRequest, apply_settings_update, serialize_settings_to_json},
};

#[derive(Component)]
pub(crate) struct SettingsView;

impl SettingsView {
    #[allow(dead_code)]
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SETTINGS_WEBVIEW_URL),
                ResolvedWebviewUri(SETTINGS_WEBVIEW_URL.to_string()),
                PageMetadata {
                    title: "Settings".to_string(),
                    url: SETTINGS_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
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

pub(crate) struct SettingsViewPlugin;

impl Plugin for SettingsViewPlugin {
    fn build(&self, app: &mut App) {
        register_settings_webview_app(
            app.world_mut()
                .resource_mut::<WebviewAppRegistry>()
                .as_mut(),
        );
        app.add_plugins(BinJsEmitEventPlugin::<SettingsCommandEvent>::default())
            .add_observer(on_settings_command)
            .add_systems(Update, broadcast_settings_to_views);
    }
}

fn register_settings_webview_app(registry: &mut WebviewAppRegistry) {
    registry.register(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_settings"),
        &WebviewAppConfig::with_custom_host("settings"),
    );
}

#[derive(Default)]
struct SettingsBroadcastCache {
    body: String,
    sent: std::collections::HashSet<Entity>,
}

fn broadcast_settings_to_views(
    settings: Res<AppSettings>,
    views: Query<Entity, (With<SettingsView>, With<UiReady>)>,
    browsers: NonSend<Browsers>,
    mut cache: Local<SettingsBroadcastCache>,
    mut commands: Commands,
) {
    if views.is_empty() {
        return;
    }
    let payload = SettingsListEvent {
        json: serialize_settings_to_json(&settings),
    };
    let body = payload.json.clone();
    if body != cache.body {
        cache.body = body;
        cache.sent.clear();
    }
    for entity in &views {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        if !cache.sent.insert(entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            SETTINGS_LIST_EVENT,
            &payload,
        ));
    }
}

fn on_settings_command(
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
