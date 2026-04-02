//! Minimal Bevy app with CEF loading embedded `assets/index.html` via `WebviewSource::vmux_service_root("history")`.
//!
//! Embedded asset: <https://bevy.org/examples/assets/embedded-asset/>. After changing `bevy_cef_core`
//! custom schemes, run `make install-debug-render-process` so `bevy_cef_debug_render_process` is in the
//! CEF framework `Libraries/` folder (macOS debug: helper next to `libGLESv2.dylib`, not only `target/debug/`).

use std::path::{Path, PathBuf};

use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use serde::{Deserialize, Serialize};

const HISTORY_HOST_EVENT: &str = "history";

/// Registers `assets/index.html` at `VMUX_HISTORY_DEFAULT_DOCUMENT` for `vmux://history`.
struct HistoryPocPlugin;

impl Plugin for HistoryPocPlugin {
    fn build(&self, app: &mut App) {
        let disk = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/index.html");
        let bytes: &'static [u8] =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/index.html"));
        app.world_mut()
            .resource_mut::<EmbeddedAssetRegistry>()
            .insert_asset(disk, Path::new(VMUX_HISTORY_DEFAULT_DOCUMENT), bytes);
    }
}

/// CEF disk profile root, separate from vmux (`~/Library/Caches/vmux/cef`) so concurrent runs
/// do not hit Chromium’s process singleton lock on the same profile.
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

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    App::new()
        .add_plugins((
            DefaultPlugins.set(WebAssetPlugin {
                silence_startup_warning: true,
            }),
            HistoryPocPlugin,
            CefPlugin {
                root_cache_path: poc_cef_root_cache_path(),
                ..default()
            },
            JsEmitEventPlugin::<History>::default(),
        ))
        .add_systems(
            Startup,
            (spawn_camera, spawn_directional_light, spawn_webview),
        )
        .add_systems(Update, emit_history_to_webview_on_change)
        .add_observer(apply_history_from_js)
        .run();
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
        History {
            url: "initial".to_string(),
        },
    ));
}

fn emit_history_to_webview_on_change(
    mut commands: Commands,
    webviews: Query<(Entity, &History), Changed<History>>,
) {
    for (entity, history) in &webviews {
        commands.trigger(HostEmitEvent::new(
            entity,
            HISTORY_HOST_EVENT,
            history,
        ));
    }
}

fn apply_history_from_js(
    trigger: On<Receive<History>>,
    mut histories: Query<&mut History>,
) {
    let Ok(mut history) = histories.get_mut(trigger.webview) else {
        return;
    };
    if history.url != trigger.url {
        history.url = trigger.url.clone();
    }
}

/// Shared snapshot for host emit (`window.cef.listen`) and JS emit (`window.cef.emit`).
#[derive(Component, Clone, PartialEq, Serialize, Deserialize, Debug)]
struct History {
    url: String,
}
