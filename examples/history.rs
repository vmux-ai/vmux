use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::Browsers;
use serde::Deserialize;

use vmux_history_poc::{HistoryEvent, HISTORY_EVENT};

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    App::new()
        .add_plugins((
            DefaultPlugins.set(WebAssetPlugin {
                silence_startup_warning: true,
            }),
            WebviewPlugin,
            HistoryPlugin,
        ))
        .add_systems(
            Startup,
            (spawn_camera, spawn_directional_light, spawn_webview),
        )
        .add_systems(Update, push_history_via_host_emit)
        .run();
}

struct WebviewPlugin;

impl Plugin for WebviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitEventPlugin::<HistoryUiReady>::default(),
            CefPlugin {
                root_cache_path: poc_cef_root_cache_path(),
                ..default()
            },
        ));
    }
}

struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_history_ui_ready);
        let manifest_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/vmux_history_poc/dist");
        let mut reg = app.world_mut().resource_mut::<EmbeddedAssetRegistry>();
        if let Err(e) = embed_dist_recursive(&mut reg, &manifest_dist, &manifest_dist) {
            bevy::log::error!(
                "vmux_history_poc: failed to embed dist/ (run `cargo build -p vmux_history_poc` so build.rs runs `dx`): {e}"
            );
        }
    }
}

fn embed_dist_recursive(
    reg: &mut EmbeddedAssetRegistry,
    manifest_dist: &Path,
    cur: &Path,
) -> std::io::Result<()> {
    let read_dir = match std::fs::read_dir(cur) {
        Ok(rd) => rd,
        Err(e) if cur == manifest_dist => return Err(e),
        Err(_) => return Ok(()),
    };
    for e in read_dir.flatten() {
        let p = e.path();
        if p.is_dir() {
            embed_dist_recursive(reg, manifest_dist, &p)?;
        } else {
            let Ok(rel) = p.strip_prefix(manifest_dist) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let embedded_path: &Path = if rel_str == "index.html" {
                Path::new(VMUX_HISTORY_DEFAULT_DOCUMENT)
            } else {
                Path::new(&rel_str)
            };
            let bytes = std::fs::read(&p)?;
            reg.insert_asset(p, embedded_path, bytes);
        }
    }
    Ok(())
}

fn poc_cef_root_cache_path() -> Option<String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            let base = if cfg!(target_os = "macos") {
                home.join("Library/Caches/vmux_history_poc")
            } else {
                home.join(".cache/vmux_history_poc")
            };
            base.join("cef").to_string_lossy().into_owned()
        })
        .or_else(|| {
            std::env::temp_dir()
                .to_str()
                .map(|p| format!("{p}/vmux_history_poc_cef"))
        })
}

/// Payload from `cef.emit` after the UI registered `cef.listen` and emitted this object (`{}`).
#[derive(Deserialize)]
struct HistoryUiReady {}

/// Marker on the [`WebviewSource`] entity once the Dioxus side has emitted ready.
#[derive(Component)]
struct HistoryPocUiReady;

/// Host emit has been sent at least once for this webview (POC: single snapshot).
#[derive(Component)]
struct HistoryPocHistorySent;

fn on_history_ui_ready(trigger: On<Receive<HistoryUiReady>>, mut commands: Commands) {
    let wv = trigger.event().webview;
    commands.entity(wv).insert(HistoryPocUiReady);
}

fn push_history_via_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    ready: Query<
        Entity,
        (
            With<WebviewSource>,
            With<HistoryPocUiReady>,
            Without<HistoryPocHistorySent>,
        ),
    >,
    history_q: Query<&History>,
) {
    for wv in ready.iter() {
        if !browsers.has_browser(wv) || !browsers.host_emit_ready(&wv) {
            continue;
        }
        let history: Vec<String> = history_q.iter().map(|h| h.url.clone()).collect();
        let url = history.join(", ");
        let payload = HistoryEvent { url, history };
        let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(wv, HISTORY_EVENT, &ron_body));
        commands.entity(wv).insert(HistoryPocHistorySent);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0., 0., 3.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_webview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.spawn((
        WebviewSource::vmux_service_root("history"),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
        MeshMaterial3d(materials.add(WebviewExtendStandardMaterial::default())),
    ));
    commands.spawn(History {
        url: "history1".to_string(),
    });
    commands.spawn(History {
        url: "history2".to_string(),
    });
    commands.spawn(History {
        url: "history3".to_string(),
    });
}

#[derive(Component)]
struct History {
    url: String,
}
