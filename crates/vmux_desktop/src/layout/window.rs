use crate::{
    browser::Browser,
    layout::pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle},
    layout::rounded::{RoundedCorners, RoundedMaterial},
    layout::side_sheet::{SideSheet, SideSheetPosition},
    layout::space::space_bundle,
    layout::tab::tab_bundle,
    profile::Profile,
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use vmux_webview_app::WebviewAppEmbedSet;
use bevy::{
    prelude::*,
    render::alpha::AlphaMode,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
};
use bevy_cef::prelude::*;
use vmux_header::{HEADER_HEIGHT_PX, HEADER_WEBVIEW_URL, Header, HeaderBundle};
use vmux_history::{CreatedAt, LastActivatedAt};

pub(crate) const WEBVIEW_Z_MAIN: f32 = 0.12;
pub(crate) const WEBVIEW_Z_FOCUS_RING: f32 = 0.13;
pub(crate) const WEBVIEW_Z_HEADER: f32 = 0.125;
pub(crate) const WEBVIEW_Z_SIDE_SHEET: f32 = 0.125;
pub(crate) const WEBVIEW_MESH_DEPTH_BIAS: f32 = -4.0;

pub(crate) struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (setup, spawn_default_session, fit_window_to_screen)
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(PostUpdate, fit_window_to_screen);
    }
}

#[derive(Bundle)]
struct WindowBundle<M>
where
    M: Material,
{
    marker: VmuxWindow,
    mesh: Mesh3d,
    material: MeshMaterial3d<M>,
    transform: Transform,
    node: Node,
    ui_target: UiTargetCamera,
}

#[derive(Component)]
pub(crate) struct VmuxWindow;

#[derive(Component)]
pub(crate) struct Main;

#[derive(Component)]
pub(crate) struct BottomBar;

#[derive(Component)]
pub(crate) struct Modal;

/// Spawns the window shell: VmuxWindow, header, side sheets, Main container.
/// Does NOT spawn session entities (Profile/Space/Pane/Tab).
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

    commands.spawn((
        WindowBundle {
            marker: VmuxWindow,
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
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(settings.layout.window.padding)),
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        },
        children![
            (
                SideSheet,
                SideSheetPosition::Left,
                HostWindow(pw),
                Browser,
                Node {
                    width: Val::Px(settings.layout.side_sheet.width),
                    flex_shrink: 0.0,
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    left: Val::Px(settings.layout.window.padding),
                    top: Val::Px(settings.layout.window.padding),
                    bottom: Val::Px(settings.layout.window.padding),
                    ..default()
                },
                ZIndex(2),
                WebviewSource::new("vmux://side-sheet/"),
                Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(settings.layout.side_sheet.width, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Hidden,
            ),
            (
                ZIndex(1),
                HostWindow(pw),
                Browser,
                Node {
                    height: Val::Px(HEADER_HEIGHT_PX),
                    flex_shrink: 0.0,
                    ..default()
                },
                HeaderBundle {
                    marker: Header,
                    source: WebviewSource::new(HEADER_WEBVIEW_URL),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                    material: MeshMaterial3d(webview_mt.add(
                        WebviewExtendStandardMaterial {
                            base: StandardMaterial {
                                unlit: true,
                                alpha_mode: AlphaMode::Blend,
                                depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                                ..default()
                            },
                            ..default()
                        },
                    )),
                    webview_size: WebviewSize(Vec2::new(1280.0, HEADER_HEIGHT_PX)),
                },
            ),
            (
                Main,
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    ..default()
                },
            ),
            (
                BottomBar,
                Node {
                    height: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
            ),
            (
                SideSheet,
                SideSheetPosition::Right,
                Node {
                    width: Val::Px(280.0),
                    position_type: PositionType::Absolute,
                    right: Val::Px(settings.layout.window.padding),
                    top: Val::Px(settings.layout.window.padding),
                    bottom: Val::Px(settings.layout.window.padding),
                    display: Display::None,
                    ..default()
                },
            ),
            (
                SideSheet,
                SideSheetPosition::Bottom,
                Node {
                    height: Val::Px(200.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(settings.layout.window.padding),
                    right: Val::Px(settings.layout.window.padding),
                    bottom: Val::Px(settings.layout.window.padding),
                    display: Display::None,
                    ..default()
                },
            ),
            (
                Modal,
                Node {
                    width: Val::Px(600.0),
                    height: Val::Px(400.0),
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-300.0),
                        top: Val::Px(-200.0),
                        ..default()
                    },
                    display: Display::None,
                    ..default()
                },
            ),
        ],
    ));
}

/// Spawns the default session (Profile/Space/Pane/Tab) if none was loaded.
fn spawn_default_session(
    main_q: Query<Entity, With<Main>>,
    profile_q: Query<(), With<Profile>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    // If profiles already exist (loaded from session.ron), skip.
    if !profile_q.is_empty() {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;
    let startup_url = settings.browser.startup_url.as_str();

    let space = commands.spawn((
        space_bundle(),
        LastActivatedAt::now(),
        CreatedAt::now(),
        ChildOf(main),
    )).id();

    let split_root = commands.spawn((
        Pane::default(),
        PaneSplit { direction: PaneSplitDirection::Row },
        HostWindow(pw),
        ZIndex(0),
        Transform::default(),
        GlobalTransform::default(),
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            column_gap: Val::Px(settings.layout.pane.gap),
            row_gap: Val::Px(settings.layout.pane.gap),
            ..default()
        },
        ChildOf(space),
    )).id();

    let leaf = commands.spawn((
        leaf_pane_bundle(),
        LastActivatedAt::now(),
        ChildOf(split_root),
    )).id();

    let tab = commands.spawn((
        tab_bundle(),
        LastActivatedAt::now(),
        CreatedAt::now(),
        ChildOf(leaf),
    )).id();

    commands.spawn((
        Browser::new(&mut meshes, &mut webview_mt, startup_url),
        ChildOf(tab),
    ));
}

pub(crate) fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<RoundedMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<RoundedMaterial>), With<VmuxWindow>>,
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
