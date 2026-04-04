use std::path::PathBuf;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::window::{PrimaryWindow, Window, WindowPlugin};
use bevy_cef::prelude::*;
use chrono::{DateTime, Duration, Utc};
use url::Url;
use vmux_core::{CAMERA_DISTANCE, NavigationHistory, VmuxWorldCamera};
use vmux_history_poc::HistoryPlugin;
use vmux_history_poc::event::{HISTORY_EVENT, HistoryEvent};
use vmux_scene::ScenePlugin;
use vmux_settings::SettingsPlugin;
use vmux_webview_app::{JsEmitUiReadyPlugin, UiReady, WebviewAppEmbedSet};

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(WindowPlugin {
                    primary_window: Some(vmux_primary_window()),
                    ..default()
                }),
            SettingsPlugin,
            NavigationHistoryInit,
            HistoryPlugin,
            BrowserPlugin,
            ScenePlugin,
        ))
        .add_systems(
            Startup,
            (
                spawn_history_webview.after(WebviewAppEmbedSet),
                spawn_sample_history_visits,
            ),
        )
        .add_systems(Update, (fit_window_to_viewport, push_history_via_host_emit))
        .run();
}

#[cfg(target_os = "macos")]
fn vmux_primary_window() -> Window {
    Window {
        transparent: true,
        decorations: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
        ..default()
    }
}

#[cfg(not(target_os = "macos"))]
fn vmux_primary_window() -> Window {
    Window::default()
}

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug)]
struct Sent(pub DateTime<Utc>);

#[derive(Bundle)]
struct VisitBundle {
    visit: Visit,
    metadata: PageMetadata,
    created_at: CreatedAt,
}

#[derive(Component, Clone, Copy)]
struct Visit;

#[allow(dead_code)]
#[derive(Component, Clone, Debug)]
struct PageMetadata {
    url: Url,
    title: String,
    favicon_url: Option<String>,
}

#[derive(Component, Clone, Copy, Debug)]
struct CreatedAt(DateTime<Utc>);

fn push_history_via_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    ready: Query<Entity, (With<WebviewSource>, With<UiReady>, Without<Sent>)>,
    history_q: Query<(&PageMetadata, &CreatedAt), With<Visit>>,
) {
    for wv in ready.iter() {
        if !browsers.has_browser(wv) || !browsers.host_emit_ready(&wv) {
            continue;
        }
        let mut rows: Vec<(&PageMetadata, &CreatedAt)> = history_q.iter().collect();
        rows.sort_by_key(|(_, created)| std::cmp::Reverse(created.0));
        let history: Vec<String> = rows
            .into_iter()
            .map(|(meta, _)| meta.url.as_str().to_owned())
            .collect();
        let url = history.join(", ");
        let payload = HistoryEvent { url, history };
        let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(wv, HISTORY_EVENT, &ron_body));
        commands.entity(wv).insert(Sent(Utc::now()));
    }
}

#[derive(Component)]
struct Browser;

#[derive(Component)]
struct FullViewportHistoryWebview;

#[derive(Bundle)]
struct BrowserBundle {
    browser: Browser,
    webview: WebviewBundle,
}

#[derive(Bundle)]
struct WebviewBundle {
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
}

fn fit_window_to_viewport(
    mut q: Query<&mut Transform, With<FullViewportHistoryWebview>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &Projection), With<VmuxWorldCamera>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let Ok((camera, projection)) = camera_q.single() else {
        return;
    };
    let Projection::Perspective(perspective) = projection else {
        return;
    };

    let mut vw = window.width();
    let mut vh = window.height();
    if !(vw.is_finite() && vh.is_finite()) || vw <= 0.0 || vh <= 0.0 {
        if let Some(s) = camera.logical_viewport_size()
            && s.x > 0.0
            && s.y > 0.0
            && s.x.is_finite()
            && s.y.is_finite()
        {
            vw = s.x;
            vh = s.y;
        } else {
            return;
        }
    }

    let aspect = vw / vh;
    let tan_half_fov = (perspective.fov * 0.5).tan();
    let half_h = CAMERA_DISTANCE * tan_half_fov;
    let half_w = half_h * aspect;

    for mut tf in &mut q {
        tf.translation = Vec3::ZERO;
        tf.scale = Vec3::new(half_w.max(1.0e-4), half_h.max(1.0e-4), 1.0);
    }
}

fn spawn_history_webview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    commands.spawn((
        BrowserBundle {
            browser: Browser,
            webview: WebviewBundle {
                source: WebviewSource::new("vmux://history"),
                mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
                material: MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    },
                    extension: WebviewMaterial::default(),
                })),
            },
        },
        Transform::default(),
        FullViewportHistoryWebview,
    ));
}

struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            CefPlugin {
                root_cache_path: example_cef_root_cache_path(),
                ..default()
            },
        ));
    }
}

#[derive(Default)]
struct NavigationHistoryInit;

impl Plugin for NavigationHistoryInit {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavigationHistory>();
    }
}

fn example_cef_root_cache_path() -> Option<String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            let base = if cfg!(target_os = "macos") {
                home.join("Library/Caches/vmux_examples")
            } else {
                home.join(".cache/vmux_examples")
            };
            base.join("cef").to_string_lossy().into_owned()
        })
        .or_else(|| {
            std::env::temp_dir()
                .to_str()
                .map(|p| format!("{p}/vmux_examples_cef"))
        })
}

fn spawn_sample_history_visits(mut commands: Commands) {
    let now = Utc::now();
    let samples = [
        (
            "https://example.com/",
            "Example",
            Some("https://example.com/favicon.ico"),
        ),
        ("https://bevyengine.org/", "Bevy", None),
        ("https://rust-lang.org/", "Rust", None),
    ];
    for (i, (href, title, favicon_url)) in samples.iter().enumerate() {
        commands.spawn(VisitBundle {
            visit: Visit,
            metadata: PageMetadata {
                url: Url::parse(href).unwrap(),
                title: (*title).to_owned(),
                favicon_url: favicon_url.map(String::from),
            },
            created_at: CreatedAt(now - Duration::minutes(i as i64)),
        });
    }
}
