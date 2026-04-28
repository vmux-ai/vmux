use crate::{
    browser::Browser,
    layout::glass::{GlassCorners, GlassMaterial},
    layout::pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle},
    layout::side_sheet::{SideSheet, SideSheetPosition},
    layout::space::space_bundle,
    layout::tab::tab_bundle,
    profile::Profile,
    scene::MainCamera,
    settings::{AppSettings, load_settings},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    render::alpha::AlphaMode,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
    winit::WINIT_WINDOWS,
};
use bevy_cef::prelude::*;
use vmux_command_bar::COMMAND_BAR_WEBVIEW_URL;
use vmux_header::{HEADER_HEIGHT_PX, HEADER_WEBVIEW_URL, Header, HeaderBundle};
use vmux_history::{CreatedAt, LastActivatedAt};
use vmux_webview_app::WebviewAppEmbedSet;

pub(crate) const WEBVIEW_Z_MAIN: f32 = 0.12;
pub(crate) const WEBVIEW_Z_FOCUS_RING: f32 = 0.122;
pub(crate) const WEBVIEW_Z_HEADER: f32 = 0.125;
pub(crate) const WEBVIEW_Z_SIDE_SHEET: f32 = 0.125;
pub(crate) const WEBVIEW_Z_MODAL: f32 = 0.5;
pub(crate) const WEBVIEW_MESH_DEPTH_BIAS: f32 = -4.0;

pub(crate) struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                setup,
                spawn_glass_panes,
                crate::persistence::load_session_on_startup,
                spawn_default_session,
                crate::persistence::rebuild_session_views,
                crate::persistence::ensure_layout_state_entities,
                crate::persistence::apply_persisted_layout_state,
                fit_window_to_screen,
            )
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(PostUpdate, (fit_window_to_screen, sync_glass_pane_clip))
        .add_systems(
            Update,
            maximize_window_to_screen.run_if(not(resource_exists::<ScreenMaximized>)),
        );
    }
}

/// One-shot resource: window has been sized to fill the screen.
#[derive(Resource)]
struct ScreenMaximized;

/// Size the window to fill the current monitor (runs once at startup).
fn maximize_window_to_screen(
    mut window_q: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok((entity, mut window)) = window_q.single_mut() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_win) = winit_windows.get_window(entity) else {
            return;
        };
        let Some(monitor) = winit_win.current_monitor() else {
            return;
        };
        let size = monitor.size();
        let scale = monitor.scale_factor() as f32;
        let logical_w = size.width as f32 / scale;
        let logical_h = size.height as f32 / scale;
        window.resolution.set(logical_w, logical_h);
        commands.insert_resource(ScreenMaximized);
    });
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

/// Marker for glass mesh entities spawned behind overlay panels (header, side sheet, modal).
#[derive(Component)]
pub(crate) struct Glass;

/// Spawns the window shell: VmuxWindow, header, side sheets, Main container.
/// Does NOT spawn session entities (Profile/Space/Pane/Tab).
fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlassMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;

    commands.spawn((
        WindowBundle {
            marker: VmuxWindow,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(GlassMaterial {
                base: StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 0.08),
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    perceptual_roughness: 0.23,
                    specular_transmission: 0.9,
                    diffuse_transmission: 1.0,
                    thickness: 1.8,
                    ior: 1.5,
                    ..default()
                },
                extension: GlassCorners {
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
                WebviewTransparent,
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
                WebviewTransparent,
                Visibility::Hidden,
                Node {
                    height: Val::Px(0.0),
                    flex_shrink: 0.0,
                    display: Display::None,
                    ..default()
                },
                HeaderBundle {
                    marker: Header,
                    source: WebviewSource::new(HEADER_WEBVIEW_URL),
                    mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
                    material: MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                        base: StandardMaterial {
                            unlit: true,
                            alpha_mode: AlphaMode::Blend,
                            depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                            ..default()
                        },
                        ..default()
                    },)),
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
                HostWindow(pw),
                Browser,
                WebviewTransparent,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
                ZIndex(3),
                WebviewSource::new(COMMAND_BAR_WEBVIEW_URL),
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
                WebviewSize(Vec2::new(800.0, 600.0)),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Hidden,
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
    // If profiles already exist (loaded from session.ron) or a session
    // file is present (entities may still be arriving from the load
    // observer), skip default session creation.
    if !profile_q.is_empty() || crate::persistence::session_path().exists() {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;
    let startup_url = settings.browser.startup_url.as_str();

    // Spawn a Profile so that on next launch, this function is skipped
    // when session.ron is loaded (the guard checks profile_q.is_empty()).
    commands.spawn(Profile::default_profile());

    let space = commands
        .spawn((
            space_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(main),
        ))
        .id();

    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
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
        ))
        .id();

    let leaf = commands
        .spawn((
            leaf_pane_bundle(),
            LastActivatedAt::now(),
            ChildOf(split_root),
        ))
        .id();

    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(leaf),
        ))
        .id();

    commands.spawn((
        Browser::new(&mut meshes, &mut webview_mt, startup_url),
        ChildOf(tab),
    ));
}

fn spawn_glass_child(
    commands: &mut Commands,
    plane: Handle<Mesh>,
    materials: &mut ResMut<Assets<GlassMaterial>>,
    r: f32,
    parent: Entity,
) {
    commands.spawn((
        Glass,
        Mesh3d(plane),
        MeshMaterial3d(materials.add(GlassMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.08),
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                perceptual_roughness: 0.1,
                metallic: 0.05,
                specular_transmission: 0.6,
                diffuse_transmission: 0.4,
                thickness: 0.02,
                ior: 1.5,
                ..default()
            },
            extension: GlassCorners {
                clip: Vec4::new(r, 1.0, 1.0, PIXELS_PER_METER),
                ..default()
            },
        })),
        Transform {
            translation: Vec3::new(0.0, 0.0, -0.002),
            ..default()
        },
        GlobalTransform::default(),
        ChildOf(parent),
    ));
}

/// Spawns a glass mesh child behind each overlay panel (Header, SideSheet Left, Modal).
fn spawn_glass_panes(
    header_q: Query<Entity, With<Header>>,
    side_sheet_q: Query<(Entity, &SideSheetPosition), (With<SideSheet>, With<Browser>)>,
    _modal_q: Query<Entity, With<Modal>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlassMaterial>>,
) {
    let r = settings.layout.pane.radius;
    let plane = meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)));

    for entity in &header_q {
        spawn_glass_child(&mut commands, plane.clone(), &mut materials, r, entity);
    }
    for (entity, pos) in &side_sheet_q {
        if *pos == SideSheetPosition::Left {
            spawn_glass_child(&mut commands, plane.clone(), &mut materials, r, entity);
        }
    }
    // Modal and panes use CSS-based glass, not 3D meshes.
}

/// Keeps each Glass's GlassCorners clip in sync with its parent panel's computed size.
fn sync_glass_pane_clip(
    q: Query<(&ChildOf, &MeshMaterial3d<GlassMaterial>), With<Glass>>,
    parent_q: Query<&ComputedNode>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<GlassMaterial>>,
) {
    let r = settings.layout.pane.radius;
    for (child_of, handle) in &q {
        let Ok(computed) = parent_q.get(child_of.get()) else {
            continue;
        };
        let size_logical = computed.size * computed.inverse_scale_factor;
        if size_logical.x <= 0.0 || size_logical.y <= 0.0 {
            continue;
        }
        let w_m = size_logical.x / PIXELS_PER_METER;
        let h_m = size_logical.y / PIXELS_PER_METER;
        if let Some(mat) = materials.get_mut(handle) {
            let clip = &mut mat.extension.clip;
            if (clip.x - r).abs() > 0.01
                || (clip.y - w_m).abs() > 0.01
                || (clip.z - h_m).abs() > 0.01
            {
                *clip = Vec4::new(r, w_m, h_m, PIXELS_PER_METER);
            }
        }
    }
}

pub(crate) fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut materials: ResMut<Assets<GlassMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<GlassMaterial>), With<VmuxWindow>>,
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
