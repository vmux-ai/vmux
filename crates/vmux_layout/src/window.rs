use crate::event::COMMAND_BAR_WEBVIEW_URL;
use crate::{
    Footer, Header, LayoutStartupSet, SessionFilePresent,
    chrome::{Browser, layout_chrome_bundle},
    command::{AppCommand, ReadAppCommands, WindowCommand},
    event::FOOTER_HEIGHT_PX,
    glass::{GlassCorners, GlassMaterial},
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    profile::Profile,
    scene::MainCamera,
    settings::LayoutSettings,
    side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
    space::space_bundle,
    tab::tab_bundle,
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    ecs::relationship::Relationship,
    picking::Pickable,
    prelude::*,
    render::alpha::AlphaMode,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
    winit::WINIT_WINDOWS,
};
use bevy_cef::prelude::*;
use vmux_history::{CreatedAt, LastActivatedAt};
use vmux_webview_app::WebviewAppEmbedSet;

pub const WEBVIEW_Z_MAIN: f32 = 0.018;
pub const WEBVIEW_Z_FOCUS_RING: f32 = 0.02;
pub const WEBVIEW_Z_HEADER: f32 = 0.022;
pub const WEBVIEW_Z_SIDE_SHEET: f32 = 0.022;
pub const WEBVIEW_Z_MODAL: f32 = 0.06;
pub const WEBVIEW_MESH_DEPTH_BIAS: f32 = 0.0;

const _: () = {
    assert!(WEBVIEW_Z_MAIN <= 0.025);
    assert!(WEBVIEW_Z_FOCUS_RING > WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_HEADER <= 0.03);
    assert!(WEBVIEW_Z_SIDE_SHEET <= 0.03);
    assert!(WEBVIEW_Z_MODAL <= 0.08);
    assert!(WEBVIEW_MESH_DEPTH_BIAS >= 0.0);
};

pub struct WindowPlugin;

fn window_glass_base_color() -> Color {
    Color::srgba(0.13, 0.13, 0.14, 1.0)
}

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup
                .in_set(LayoutStartupSet::Window)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
        )
        .add_systems(
            Startup,
            spawn_default_session.in_set(LayoutStartupSet::DefaultSession),
        )
        .add_systems(
            Startup,
            (
                spawn_glass_panes,
                crate::tab::open_command_bar_if_no_tabs,
                fit_window_to_screen,
            )
                .chain()
                .in_set(LayoutStartupSet::Post),
        )
        .add_systems(
            PostUpdate,
            (
                fit_window_to_screen,
                sync_glass_pane_clip,
                sync_window_layout_to_settings,
            ),
        )
        .add_systems(
            Update,
            (
                maximize_window_to_screen.run_if(not(resource_exists::<ScreenMaximized>)),
                crate::tab::open_command_bar_if_no_tabs,
            ),
        )
        .add_systems(Update, handle_window_commands.in_set(ReadAppCommands));
    }
}

/// Handle `WindowCommand` events (e.g. minimize via Cmd+M).
fn handle_window_commands(
    mut reader: MessageReader<AppCommand>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
) {
    for cmd in reader.read() {
        if let AppCommand::Window(WindowCommand::Minimize) = cmd {
            let entity = *primary_window;
            WINIT_WINDOWS.with_borrow(|winit_windows| {
                if let Some(winit_win) = winit_windows.get_window(entity) {
                    winit_win.set_minimized(true);
                }
            });
        }
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
pub struct VmuxWindow;

#[derive(Component)]
pub struct Main;

#[derive(Component)]
pub struct MainColumn;

#[derive(Component)]
pub struct Modal;

#[derive(Component)]
pub struct Glass;

fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<LayoutSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GlassMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;

    let root = commands
        .spawn(WindowBundle {
            marker: VmuxWindow,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(GlassMaterial {
                base: StandardMaterial {
                    base_color: window_glass_base_color(),
                    unlit: true,
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
                    clip: Vec4::new(settings.pane.radius, m.x, m.y, PIXELS_PER_METER),
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
                flex_direction: FlexDirection::Row,
                padding: UiRect {
                    top: Val::Px(settings.window.pad_top()),
                    right: Val::Px(settings.window.pad_right()),
                    bottom: Val::Px(settings.window.pad_bottom()),
                    left: Val::Px(settings.window.pad_left()),
                },
                column_gap: Val::Px(settings.pane.gap),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        })
        .id();

    let left_side_sheet = commands
        .spawn((
            SideSheet,
            SideSheetPosition::Left,
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Hidden,
            Node {
                width: Val::Px(settings.side_sheet.width),
                min_height: Val::Px(0.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            ZIndex(2),
            ChildOf(root),
        ))
        .id();

    let main_column = commands
        .spawn((
            MainColumn,
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(settings.pane.gap),
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        Header,
        ZIndex(1),
        Visibility::Hidden,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            height: Val::Px(0.0),
            flex_shrink: 0.0,
            display: Display::None,
            ..default()
        },
        ChildOf(left_side_sheet),
    ));

    commands.spawn((
        Main,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        },
        ChildOf(main_column),
    ));

    commands.spawn((
        Footer,
        crate::Open,
        ZIndex(1),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Inherited,
        Node {
            height: Val::Px(FOOTER_HEIGHT_PX),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(main_column),
    ));

    // Right & Bottom side sheets remain absolute overlays (slide-in semantics);
    // they're not part of the natural flex layout.
    commands.spawn((
        SideSheet,
        SideSheetPosition::Right,
        Node {
            width: Val::Px(280.0),
            position_type: PositionType::Absolute,
            right: Val::Px(settings.window.pad_right()),
            top: Val::Px(settings.window.pad_top()),
            bottom: Val::Px(settings.window.pad_bottom()),
            display: Display::None,
            ..default()
        },
        ChildOf(root),
    ));

    commands.spawn((
        SideSheet,
        SideSheetPosition::Bottom,
        Node {
            height: Val::Px(200.0),
            position_type: PositionType::Absolute,
            left: Val::Px(settings.window.pad_left()),
            right: Val::Px(settings.window.pad_right()),
            bottom: Val::Px(settings.window.pad_bottom()),
            display: Display::None,
            ..default()
        },
        ChildOf(root),
    ));

    commands.spawn((
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
        Pickable::IGNORE,
        ChildOf(root),
    ));

    commands.spawn((
        layout_chrome_bundle(pw, &mut meshes, &mut webview_mt),
        ChildOf(root),
    ));
}

/// Spawns the default session (Profile/Space/Pane/Tab) if none was loaded.
fn spawn_default_session(
    main_q: Query<Entity, With<Main>>,
    profile_q: Query<(), With<Profile>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<LayoutSettings>,
    session_file: Option<Res<SessionFilePresent>>,
    mut new_tab_ctx: ResMut<crate::NewTabContext>,
    mut commands: Commands,
) {
    // If profiles already exist (loaded from session.ron) or a session
    // file is present (entities may still be arriving from the load
    // observer), skip default session creation.
    if !profile_q.is_empty() || session_file.as_deref().is_some_and(|s| s.0) {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    spawn_default_session_layout(
        main,
        *primary_window,
        &settings,
        &mut new_tab_ctx,
        &mut commands,
    );
}

pub fn spawn_default_session_layout(
    main: Entity,
    pw: Entity,
    settings: &LayoutSettings,
    new_tab_ctx: &mut crate::NewTabContext,
    commands: &mut Commands,
) {
    commands.spawn(Profile::default_profile());
    let space = commands
        .spawn((
            space_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(main),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, settings.pane.gap);
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
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
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

    // Create an empty tab (no browser content) and open the command bar
    // so the user is immediately prompted instead of seeing an empty pane.
    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(leaf),
        ))
        .id();
    new_tab_ctx.tab = Some(tab);
    new_tab_ctx.previous_tab = None;
    new_tab_ctx.needs_open = true;
}

fn spawn_glass_panes() {}

fn sync_glass_pane_clip(
    q: Query<(&ChildOf, &MeshMaterial3d<GlassMaterial>), With<Glass>>,
    parent_q: Query<&ComputedNode>,
    settings: Res<LayoutSettings>,
    mut materials: ResMut<Assets<GlassMaterial>>,
) {
    let r = settings.pane.radius;
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

/// Re-applies layout-affecting settings (window padding, row gap, side sheet
/// insets and width) to existing nodes whenever `LayoutSettings` changes (e.g.
/// after settings.ron hot-reload). Without this, edits to the file produce a
/// "Settings reloaded" log but no visual change because `setup` only reads
/// settings once at Startup.
fn sync_window_layout_to_settings(
    settings: Res<LayoutSettings>,
    mut window_q: Query<&mut Node, (With<VmuxWindow>, Without<SideSheet>, Without<MainColumn>)>,
    mut main_column_q: Query<
        &mut Node,
        (With<MainColumn>, Without<VmuxWindow>, Without<SideSheet>),
    >,
    mut sheet_q: Query<
        (&SideSheetPosition, &mut Node),
        (With<SideSheet>, Without<VmuxWindow>, Without<MainColumn>),
    >,
    mut sheet_width: ResMut<SideSheetWidth>,
) {
    if !settings.is_changed() {
        return;
    }

    let pad_top = settings.window.pad_top();
    let pad_right = settings.window.pad_right();
    let pad_bottom = settings.window.pad_bottom();
    let pad_left = settings.window.pad_left();
    let gap = settings.pane.gap;
    let cfg_width = settings.side_sheet.width;

    // Root window: padding + flex-row column gap.
    if let Ok(mut node) = window_q.single_mut() {
        node.padding = UiRect {
            top: Val::Px(pad_top),
            right: Val::Px(pad_right),
            bottom: Val::Px(pad_bottom),
            left: Val::Px(pad_left),
        };
        node.column_gap = Val::Px(gap);
    }

    if let Ok(mut node) = main_column_q.single_mut() {
        node.row_gap = Val::Px(gap);
    }

    // Side sheet width resource: initialise from settings on first run.
    if sheet_width.0 <= 0.0 {
        sheet_width.0 = cfg_width;
    }
    let live_width = sheet_width.0;

    // Left sheet is a flex child — only its width tracks settings.
    // Right & Bottom sheets remain absolute overlays — their insets follow
    // the window padding.
    for (pos, mut node) in &mut sheet_q {
        match pos {
            SideSheetPosition::Left => {
                node.width = Val::Px(live_width);
            }
            SideSheetPosition::Right => {
                node.right = Val::Px(pad_right);
                node.top = Val::Px(pad_top);
                node.bottom = Val::Px(pad_bottom);
            }
            SideSheetPosition::Bottom => {
                node.left = Val::Px(pad_left);
                node.right = Val::Px(pad_right);
                node.bottom = Val::Px(pad_bottom);
            }
        }
    }
}

pub fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    settings: Res<LayoutSettings>,
    mut materials: ResMut<Assets<GlassMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<GlassMaterial>), With<VmuxWindow>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    let r = settings.pane.radius;

    for (mut tf, handle) in &mut q {
        tf.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        tf.scale = Vec3::new(m.x, m.y, 1.0);

        if let Some(mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_cef::prelude::WebviewExtendStandardMaterial;

    static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = HOME_ENV_LOCK.lock().expect("home env lock");
            let old_home = std::env::var_os("HOME");
            let home =
                std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).expect("create temp home");
            unsafe {
                std::env::set_var("HOME", &home);
            }
            Self {
                _guard: guard,
                old_home,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(home) = &self.old_home {
                    std::env::set_var("HOME", home);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    #[test]
    fn window_glass_uses_dark_finder_style_background() {
        assert_eq!(
            window_glass_base_color(),
            Color::srgba(0.13, 0.13, 0.14, 1.0)
        );
    }

    fn test_settings(gap: f32) -> LayoutSettings {
        LayoutSettings {
            window: crate::settings::WindowSettings {
                padding: 0.0,
                padding_top: None,
                padding_right: None,
                padding_bottom: None,
                padding_left: None,
            },
            pane: crate::settings::PaneSettings { gap, radius: 8.0 },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        }
    }

    fn setup_window_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(test_settings(8.0));
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<GlassMaterial>>();
        app.init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn(crate::scene::MainCamera);
        app.add_systems(Startup, (setup, spawn_glass_panes).chain());
        app
    }

    #[test]
    fn header_lives_inside_left_side_sheet() {
        let mut app = setup_window_app();
        app.update();

        let header = app
            .world_mut()
            .query_filtered::<Entity, With<Header>>()
            .single(app.world())
            .expect("header");
        let side_sheet = app
            .world_mut()
            .query_filtered::<(Entity, &SideSheetPosition), With<SideSheet>>()
            .iter(app.world())
            .find_map(|(entity, position)| (*position == SideSheetPosition::Left).then_some(entity))
            .expect("left side sheet");
        let parent = app
            .world()
            .get::<ChildOf>(header)
            .map(Relationship::get)
            .expect("header parent");

        assert_eq!(parent, side_sheet);
    }

    #[test]
    fn chrome_panels_do_not_spawn_separate_glass() {
        let mut app = setup_window_app();
        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<Glass>>()
            .iter(app.world())
            .count();

        assert_eq!(count, 0);
    }

    #[test]
    fn default_session_requests_command_bar_open() {
        let _home = HomeEnvGuard::use_temp_home("default-session");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<crate::NewTabContext>();
        app.insert_resource(LayoutSettings {
            window: crate::settings::WindowSettings {
                padding: 0.0,
                padding_top: None,
                padding_right: None,
                padding_bottom: None,
                padding_left: None,
            },
            pane: crate::settings::PaneSettings {
                gap: 0.0,
                radius: 0.0,
            },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        });
        app.add_systems(Update, spawn_default_session);

        app.world_mut().spawn(PrimaryWindow);
        app.world_mut().spawn(Main);

        app.update();

        let ctx = app.world().resource::<crate::NewTabContext>();
        assert!(ctx.tab.is_some());
        assert!(ctx.needs_open);
    }
}
