use crate::{
    rounded::{RoundedCorners, RoundedMaterial},
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    ui::{UiGlobalTransform, UiSystems, UiTargetCamera, ZIndex},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use std::path::PathBuf;
use vmux_status_bar::{STATUS_BAR_WEBVIEW_URL, StatusBar, StatusBarBundle};
use vmux_webview_app::JsEmitUiReadyPlugin;

pub struct Layout3Plugin;

impl Plugin for Layout3Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            CefPlugin {
                command_line_config: cef_command_line_config(),
                root_cache_path: cef_root_cache_path(),
                ..default()
            },
        ))
        .add_systems(
            Startup,
            (setup, fit_display_glass_to_window)
                .chain()
                .after(load_settings)
                .after(crate::scene::setup),
        )
        .add_systems(
            PostUpdate,
            (
                fit_display_glass_to_window,
                sync_children_to_ui,
                sync_webview_pane_corner_clip,
                sync_osr_webview_focus,
                kick_main_startup_navigation,
            )
                .chain()
                .after(UiSystems::Layout)
                .before(render_standard_materials),
        );
    }
}

#[derive(Bundle)]
struct DisplayGlassBundle<M>
where
    M: Material,
{
    marker: DisplayGlass,
    mesh: Mesh3d,
    material: MeshMaterial3d<M>,
    transform: Transform,
    node: Node,
    ui_target: UiTargetCamera,
}

#[derive(Component)]
pub struct DisplayGlass;

#[derive(Bundle)]
struct MainBundle {
    marker: Main,
    child_of: ChildOf,
    node: Node,
    browser: BrowserBundle,
}

#[derive(Component)]
struct Main;

#[derive(Bundle)]
struct BrowserBundle {
    marker: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
    webview_size: WebviewSize,
}

#[derive(Component)]
struct Browser;

fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;

    let display = commands
        .spawn(DisplayGlassBundle {
            marker: DisplayGlass,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(RoundedMaterial {
                base: StandardMaterial {
                    base_color: Color::srgba(0.08, 0.08, 0.08, 0.4),
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    perceptual_roughness: 0.12,
                    metallic: 0.0,
                    specular_transmission: 0.9,
                    diffuse_transmission: 1.0,
                    thickness: 0.1,
                    ior: 1.5,
                    ..default()
                },
                extension: RoundedCorners {
                    clip: Vec4::new(settings.layout.pane.radius, m.x, m.y, PIXELS_PER_METER),
                    ..default()
                },
            })),
            transform: Transform {
                translation: Vec3::new(0.0, m.y * 0.5, 0.0),
                scale: Vec3::new(m.x, m.y, 1.0),
                ..default()
            },
            node: Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Relative,
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        })
        .id();

    commands
        .spawn((
            ZIndex(0),
            HostWindow(pw),
            MainBundle {
                marker: Main,
                child_of: ChildOf(display),
                node: Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(STATUS_BAR_HEIGHT_PX),
                    ..default()
                },
                browser: BrowserBundle {
                    marker: Browser,
                    source: WebviewSource::new(settings.browser.startup_url.as_str()),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                    material: MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                        base: StandardMaterial {
                            unlit: true,
                            alpha_mode: AlphaMode::Blend,
                            depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                            ..default()
                        },
                        ..default()
                    })),
                    webview_size: WebviewSize(Vec2::new(1280.0, 720.0)),
                },
            },
        ))
        .insert(CefKeyboardTarget);

    commands.spawn((
        ChildOf(display),
        ZIndex(1),
        HostWindow(pw),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            height: Val::Px(STATUS_BAR_HEIGHT_PX),
            ..default()
        },
        StatusBarBundle {
            marker: StatusBar,
            source: WebviewSource::new(STATUS_BAR_WEBVIEW_URL),
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                    ..default()
                },
                ..default()
            })),
            webview_size: WebviewSize(Vec2::new(1280.0, STATUS_BAR_HEIGHT_PX)),
        },
    ));
}

pub fn fit_display_glass_to_window(
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<RoundedMaterial>), With<DisplayGlass>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    let r = settings.layout.pane.radius;

    for (mut tf, handle) in &mut q {
        tf.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        tf.scale = Vec3::new(m.x, m.y, 1.0);

        if let Some(mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
        }
    }
}

fn sync_children_to_ui(
    mut child_q: Query<(
        &mut Transform,
        &ComputedNode,
        &ChildOf,
        &UiGlobalTransform,
        Option<&mut WebviewSize>,
        Option<&Main>,
        Option<&StatusBar>,
    )>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<DisplayGlass>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;

    for (mut tf, computed, child_of, child_ui_gt, webview_size, main, status) in child_q.iter_mut()
    {
        if child_of.get() != glass_entity {
            continue;
        }

        let glass_size_px = glass_node.size;
        if glass_size_px.x <= 0.0 || glass_size_px.y <= 0.0 {
            continue;
        }

        let size_px = computed.size;
        if size_px.x <= 0.0 || size_px.y <= 0.0 {
            continue;
        }

        let sx = size_px.x / glass_size_px.x;
        let sy = size_px.y / glass_size_px.y;
        tf.scale = Vec3::new(sx, sy, 1.0);

        let child_center_ui = child_ui_gt.transform_point2(Vec2::ZERO);
        let glass_center_ui = glass_ui_gt.transform_point2(Vec2::ZERO);
        let delta_px = child_center_ui - glass_center_ui;

        let tx = delta_px.x / glass_size_px.x;
        let ty = -delta_px.y / glass_size_px.y;
        let z = if status.is_some() {
            WEBVIEW_Z_STATUS
        } else if main.is_some() {
            WEBVIEW_Z_MAIN
        } else {
            0.01 + computed.stack_index as f32 * 0.001
        };
        tf.translation = Vec3::new(tx, ty, z);

        if let Some(mut size) = webview_size {
            let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
            if size.0 != dip {
                size.0 = dip;
            }
        }
    }
}

fn sync_webview_pane_corner_clip(
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    main: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<Main>>,
    status: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<StatusBar>>,
) {
    let r = settings.layout.pane.radius;
    for (size, mat_h) in &main {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(0.0, w, h, 0.0);
        }
    }
    for (size, mat_h) in &status {
        let w = size.0.x.max(1.0e-6);
        let h = size.0.y.max(1.0e-6);
        if let Some(mat) = materials.get_mut(mat_h.id()) {
            mat.extension.pane_corner_clip = Vec4::new(r, w, h, 1.0);
        }
    }
}

const STATUS_BAR_HEIGHT_PX: f32 = 40.0;
const WEBVIEW_Z_MAIN: f32 = 0.12;
const WEBVIEW_Z_STATUS: f32 = 0.125;
const WEBVIEW_MESH_DEPTH_BIAS: f32 = -4.0;

fn kick_main_startup_navigation(
    browsers: NonSend<Browsers>,
    q: Query<(Entity, &WebviewSource), With<Main>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok((entity, source)) = q.single() else {
        return;
    };
    let WebviewSource::Url(url) = source else {
        return;
    };
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    browsers.navigate(&entity, url);
    *done = true;
}

fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    keyboard_target: Query<Entity, (With<WebviewSource>, With<CefKeyboardTarget>)>,
    mut ready: Local<Vec<Entity>>,
    mut auxiliary: Local<Vec<Entity>>,
) {
    ready.clear();
    ready.extend(webviews.iter().filter(|&e| browsers.has_browser(e)));
    if ready.is_empty() {
        return;
    }
    ready.sort_by_key(|e| e.to_bits());

    let active = keyboard_target
        .iter()
        .filter(|&k| ready.iter().any(|&e| e == k))
        .min_by_key(|e| e.to_bits())
        .unwrap_or(ready[0]);

    auxiliary.clear();
    auxiliary.extend(ready.iter().copied().filter(|&e| e != active));
    browsers.sync_osr_focus_to_active_pane(Some(active), auxiliary.as_slice());
}

fn cef_command_line_config() -> CommandLineConfig {
    CommandLineConfig::default().with_switch_value(
        "user-agent",
        concat!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ",
            "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        ),
    )
}

fn cef_root_cache_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support/vmux")
                .to_string_lossy()
                .into_owned()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
            .to_str()
            .map(|p| format!("{p}/vmux_cef"))
    }
}
