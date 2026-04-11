use crate::{
    command::{AppCommand, ReadAppCommands, TabCommand, WriteAppCommands},
    rounded::{RoundedCorners, RoundedMaterial},
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    ecs::query::Has,
    ecs::relationship::Relationship,
    prelude::*,
    ui::{UiGlobalTransform, UiSystems, UiTargetCamera, ZIndex},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::RenderTextureMessage;
use std::{collections::HashSet, path::PathBuf};
use vmux_status_bar::{
    STATUS_BAR_WEBVIEW_URL, StatusBar, StatusBarBundle,
    event::{TABS_EVENT, TabRow, TabsHostEvent},
};
use vmux_webview_app::{JsEmitUiReadyPlugin, UiReady};

pub struct Layout3Plugin;

impl Plugin for Layout3Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            CefPlugin {
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
            Update,
            (
                write_tab_hotkeys.in_set(WriteAppCommands),
                on_viewport_tab_command.in_set(ReadAppCommands),
                push_tabs_host_emit.after(on_viewport_tab_command),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                fit_display_glass_to_window,
                sync_tab_visibility_and_keyboard_target,
                sync_children_to_ui,
                sync_webview_pane_corner_clip,
                sync_osr_webview_focus,
                kick_tab_startup_navigation,
                flush_pending_osr_textures,
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
    transform: Transform,
    global_transform: GlobalTransform,
    node: Node,
}

#[derive(Component)]
struct Main;

#[derive(Component, Clone, Copy, Debug)]
struct Tab;

#[derive(Component)]
struct Active;

#[derive(Component, Clone, Debug)]
struct PageMetadata {
    title: String,
    url: String,
}

#[derive(Bundle)]
struct TabRootBundle {
    tab: Tab,
    metadata: PageMetadata,
    child_of: ChildOf,
    transform: Transform,
    global_transform: GlobalTransform,
    node: Node,
}

#[derive(Bundle)]
struct TabBrowserBundle {
    browser: Browser,
    source: WebviewSource,
    mesh: Mesh3d,
    material: MeshMaterial3d<WebviewExtendStandardMaterial>,
    webview_size: WebviewSize,
    child_of: ChildOf,
    transform: Transform,
    global_transform: GlobalTransform,
    node: Node,
    visibility: Visibility,
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

    let main_viewport = commands
        .spawn((
            ZIndex(0),
            HostWindow(pw),
            MainBundle {
                marker: Main,
                child_of: ChildOf(display),
                transform: Transform::default(),
                global_transform: GlobalTransform::default(),
                node: Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(STATUS_BAR_HEIGHT_PX),
                    ..default()
                },
            },
        ))
        .id();

    let startup = settings.browser.startup_url.as_str();
    let seeds: [(&str, &str); 3] = [
        ("Start", startup),
        ("Example", "https://example.com/"),
        ("Bevy", "https://bevyengine.org/"),
    ];

    for (i, (title, url)) in seeds.into_iter().enumerate() {
        let tab_root = commands
            .spawn(TabRootBundle {
                tab: Tab,
                metadata: PageMetadata {
                    title: title.to_string(),
                    url: url.to_string(),
                },
                child_of: ChildOf(main_viewport),
                transform: Transform::default(),
                global_transform: GlobalTransform::default(),
                node: Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
            })
            .id();
        if i == 0 {
            commands.entity(tab_root).insert(Active);
        }
        commands.spawn(TabBrowserBundle {
            browser: Browser,
            source: WebviewSource::new(url),
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
            child_of: ChildOf(tab_root),
            transform: Transform::default(),
            global_transform: GlobalTransform::default(),
            node: Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            visibility: if i == 0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        });
    }

    commands.spawn((
        ChildOf(display),
        ZIndex(1),
        HostWindow(pw),
        Browser,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Percent(100.0),
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

fn write_tab_hotkeys(keyboard: Res<ButtonInput<KeyCode>>, mut writer: MessageWriter<AppCommand>) {
    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let meta = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);
    if !keyboard.just_pressed(KeyCode::Tab) || (!ctrl && !meta) {
        return;
    }
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if shift {
        writer.write(AppCommand::Tab(TabCommand::Previous));
    } else {
        writer.write(AppCommand::Tab(TabCommand::Next));
    }
}

fn on_viewport_tab_command(
    mut reader: MessageReader<AppCommand>,
    main: Single<&Children, With<Main>>,
    tab_filter: Query<(), With<Tab>>,
    active_q: Query<Entity, With<Active>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
        let tabs: Vec<Entity> = main.iter().filter(|&e| tab_filter.contains(e)).collect();
        if tabs.len() < 2 {
            continue;
        }
        let Ok(current) = active_q.single() else {
            continue;
        };
        let Some(pos) = tabs.iter().position(|&e| e == current) else {
            continue;
        };
        let n = tabs.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target = tabs[idx];
        commands.entity(current).remove::<Active>();
        commands.entity(target).insert(Active);
    }
}

fn sync_tab_visibility_and_keyboard_target(
    browsers: NonSend<Browsers>,
    main: Single<&Children, With<Main>>,
    children_q: Query<&Children>,
    tab_roots: Query<(), With<Tab>>,
    active_root: Query<Entity, With<Active>>,
    mut browser_q: Query<(Entity, &mut Visibility, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Ok(active_entity) = active_root.single() else {
        return;
    };
    for tab_root in main.iter() {
        if !tab_roots.contains(tab_root) {
            continue;
        }
        let Ok(tab_children) = children_q.get(tab_root) else {
            continue;
        };
        let is_active = tab_root == active_entity;
        for browser_e in tab_children.iter() {
            let Ok((_, mut visibility, has_kb)) = browser_q.get_mut(browser_e) else {
                continue;
            };
            *visibility = if is_active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            browsers.set_osr_not_hidden(&browser_e);
            if is_active && !has_kb {
                commands.entity(browser_e).insert(CefKeyboardTarget);
            } else if !is_active && has_kb {
                commands.entity(browser_e).remove::<CefKeyboardTarget>();
            }
        }
    }
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
    mut browser_q: Query<
        (
            &mut Transform,
            &ComputedNode,
            &UiGlobalTransform,
            &ChildOf,
            &mut WebviewSize,
            Option<&StatusBar>,
        ),
        With<Browser>,
    >,
    tab_rect: Query<(&ComputedNode, &UiGlobalTransform), With<Tab>>,
    glass: Single<(Entity, &ComputedNode, &UiGlobalTransform), With<DisplayGlass>>,
) {
    let &(glass_entity, glass_node, glass_ui_gt) = &*glass;

    for (mut tf, self_computed, self_ui_gt, child_of, mut webview_size, status) in
        browser_q.iter_mut()
    {
        let parent = child_of.get();
        let (computed, ui_gt) = match tab_rect.get(parent) {
            Ok((cn, gt)) => (cn, gt),
            Err(_) => (self_computed, self_ui_gt),
        };

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

        let center_ui = ui_gt.transform_point2(Vec2::ZERO);
        let glass_center_ui = glass_ui_gt.transform_point2(Vec2::ZERO);
        let delta_px = center_ui - glass_center_ui;

        let tx = delta_px.x / glass_size_px.x;
        let ty = -delta_px.y / glass_size_px.y;
        let z = if status.is_some() {
            WEBVIEW_Z_STATUS
        } else if parent != glass_entity {
            WEBVIEW_Z_MAIN
        } else {
            0.01 + self_computed.stack_index as f32 * 0.001
        };
        tf.translation = Vec3::new(tx, ty, z);

        let dip = (size_px * computed.inverse_scale_factor).max(Vec2::splat(1.0));
        if webview_size.0 != dip {
            webview_size.0 = dip;
        }
    }
}

fn sync_webview_pane_corner_clip(
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    tabs: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<Browser>>,
    status: Query<(&WebviewSize, &MeshMaterial3d<WebviewExtendStandardMaterial>), With<StatusBar>>,
) {
    let r = settings.layout.pane.radius;
    for (size, mat_h) in &tabs {
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

fn kick_tab_startup_navigation(
    browsers: NonSend<Browsers>,
    q: Query<(Entity, &WebviewSource), With<Browser>>,
    mut kicked: Local<HashSet<u64>>,
) {
    for (entity, source) in &q {
        let WebviewSource::Url(url) = source else {
            continue;
        };
        let key = entity.to_bits();
        if kicked.contains(&key) {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        browsers.navigate(&entity, url);
        kicked.insert(key);
    }
}

fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    keyboard_target: Query<Entity, (With<WebviewSource>, With<CefKeyboardTarget>)>,
    status_chrome: Query<Entity, (With<StatusBar>, With<Browser>)>,
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
    for e in status_chrome.iter() {
        browsers.set_osr_not_hidden(&e);
    }
}

fn push_tabs_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    main: Single<&Children, With<Main>>,
    status: Single<Entity, (With<StatusBar>, With<UiReady>)>,
    tab_q: Query<(&PageMetadata, Option<&Active>), With<Tab>>,
    mut last: Local<String>,
) {
    let status_e = *status;
    if !browsers.has_browser(status_e) || !browsers.host_emit_ready(&status_e) {
        return;
    }
    let mut rows: Vec<TabRow> = Vec::new();
    for child in main.iter() {
        if let Ok((meta, active)) = tab_q.get(child) {
            rows.push(TabRow {
                title: meta.title.clone(),
                url: meta.url.clone(),
                is_active: active.is_some(),
            });
        }
    }
    let payload = TabsHostEvent { tabs: rows };
    let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
    if ron_body.as_str() == last.as_str() {
        return;
    }
    commands.trigger(HostEmitEvent::new(status_e, TABS_EVENT, &ron_body));
    *last = ron_body;
}

fn flush_pending_osr_textures(
    mut ew: MessageWriter<RenderTextureMessage>,
    browsers: NonSend<Browsers>,
) {
    while let Ok(texture) = browsers.try_receive_texture() {
        ew.write(texture);
    }
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
