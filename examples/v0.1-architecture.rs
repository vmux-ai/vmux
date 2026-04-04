use std::path::PathBuf;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::window::{CompositeAlphaMode, PrimaryWindow, Window as NativeWindow, WindowPlugin};
use bevy_cef::prelude::*;
use chrono::{DateTime, Duration, Utc};
use url::Url;
use vmux_core::{CAMERA_DISTANCE, VmuxWorldCamera};
use vmux_history_poc::HistoryPlugin;
use vmux_history_poc::event::{HISTORY_EVENT, HistoryEvent};
use vmux_scene::ScenePlugin;
use vmux_webview_app::{JsEmitUiReadyPlugin, UiReady, WebviewAppEmbedSet};

#[derive(Message)]
enum AppCommand {
    LayoutCommand(LayoutCommand),
}

#[derive(Message)]
enum LayoutCommand {
    NewSpace { name: String },
}

struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>();
    }
}

struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, new_space_on_startup).add_systems(
            Update,
            (
                handle_new_space,
                spawn_window_on_new_space,
                spawn_pane_on_new_window,
                spawn_tab_on_new_pane,
                spawn_browser_on_new_tab,
            ),
        );
    }
}

fn handle_new_space(mut commands: Commands) {
    commands.spawn(SpaceBundle {
        space: Space,
        name: Name::new("Default Space"),
    });
}

fn spawn_window_on_new_space(mut commands: Commands, query: Query<Entity, Added<Space>>) {
    for space in query.iter() {
        commands.entity(space).with_children(|parent| {
            parent.spawn(WindowBundle {
                window: Window,
                name: Name::new("Default Window"),
            });
        });
    }
}

fn spawn_pane_on_new_window(mut commands: Commands, query: Query<Entity, Added<Window>>) {
    for window in query.iter() {
        commands.entity(window).with_children(|parent| {
            parent.spawn(PaneBundle {
                pane: Pane::Horizontal,
                weight: Weight(1.0),
            });
        });
    }
}

fn spawn_tab_on_new_pane(mut commands: Commands, query: Query<Entity, Added<Pane>>) {
    for window in query.iter() {
        commands.entity(window).with_children(|parent| {
            parent.spawn(TabBundle {
                tab: Tab,
                weight: Weight(0.5),
                name: Name::new("New Tab"),
                created_at: CreatedAt(Utc::now()),
            });
        });
    }
}

fn spawn_browser_on_new_tab(
    mut commands: Commands,
    query: Query<Entity, Added<Tab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for tab in query.iter() {
        commands.entity(tab).with_children(|parent| {
            parent.spawn(BrowserBundle {
                browser: Browser,
                source: WebviewSource::new("https://example.com/"),
                mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::ONE))),
                material: MeshMaterial3d(materials.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    },
                    extension: WebviewMaterial::default(),
                })),
            });
        });
    }
}

#[derive(Bundle)]
pub struct TabBundle {
    pub tab: Tab,
    pub weight: Weight,
    pub name: Name,
    pub created_at: CreatedAt,
}

#[derive(Bundle)]
struct BrowserBundle {
    browser: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
}

#[derive(Bundle)]
struct PaneBundle {
    pane: Pane,
    weight: Weight,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Horizontal,
    Vertical,
}

#[derive(Bundle)]
struct WindowBundle {
    window: Window,
    name: Name,
}

#[derive(Component)]
struct Window;

#[derive(Bundle)]
struct SpaceBundle {
    space: Space,
    name: Name,
}

#[derive(Component)]
struct Space;

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    let primary_window = NativeWindow {
        transparent: true,
        composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
        decorations: true,
        titlebar_shown: false,
        movable_by_window_background: true,
        fullsize_content_view: true,
        ..default()
    };
    let window_plugin = WindowPlugin {
        primary_window: Some(primary_window),
        ..default()
    };

    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin),
            ScenePlugin,
            CommandPlugin,
            // LayoutPlugin,
            // HistoryPlugin,
            // BrowserPlugin,
        ))
        // .add_systems(Startup, spawn_sample_history_visits)
        // .add_systems(Update, push_history_via_host_emit)
        .run();
}

fn new_space_on_startup(mut msg: MessageWriter<AppCommand>) {
    msg.write(AppCommand::LayoutCommand(LayoutCommand::NewSpace {
        name: "New Space".to_string(),
    }));
}

#[derive(Component, Clone, Copy, Debug)]
struct Tab;

#[derive(Component, Clone, Copy, Debug)]
struct Weight(f32);

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
