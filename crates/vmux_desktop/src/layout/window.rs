use crate::{
    browser::Browser,
    command::{AppCommand, ReadAppCommands, WindowCommand},
    layout::glass::{GlassCorners, GlassMaterial},
    layout::pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    layout::side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
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
use vmux_footer::{FOOTER_HEIGHT_PX, FOOTER_WEBVIEW_URL, Footer, FooterBundle};
use vmux_header::{HEADER_HEIGHT_PX, HEADER_WEBVIEW_URL, Header, HeaderBundle};
use vmux_history::{CreatedAt, LastActivatedAt};
use vmux_webview_app::WebviewAppEmbedSet;

pub(crate) const WEBVIEW_Z_MAIN: f32 = 0.018;
pub(crate) const WEBVIEW_Z_FOCUS_RING: f32 = 0.02;
pub(crate) const WEBVIEW_Z_HEADER: f32 = 0.022;
pub(crate) const WEBVIEW_Z_SIDE_SHEET: f32 = 0.022;
pub(crate) const WEBVIEW_Z_MODAL: f32 = 0.06;
pub(crate) const WEBVIEW_MESH_DEPTH_BIAS: f32 = 0.0;

const _: () = {
    assert!(WEBVIEW_Z_MAIN <= 0.025);
    assert!(WEBVIEW_Z_FOCUS_RING > WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_HEADER <= 0.03);
    assert!(WEBVIEW_Z_SIDE_SHEET <= 0.03);
    assert!(WEBVIEW_Z_MODAL <= 0.08);
    assert!(WEBVIEW_MESH_DEPTH_BIAS >= 0.0);
};

pub(crate) struct WindowPlugin;

fn window_glass_base_color() -> Color {
    Color::srgba(0.13, 0.13, 0.14, 1.0)
}

fn chrome_glass_base_color() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.0)
}

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
                crate::layout::tab::open_command_bar_if_no_tabs,
                fit_window_to_screen,
            )
                .chain()
                .after(load_settings)
                .after(crate::scene::setup)
                .after(WebviewAppEmbedSet),
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
                crate::layout::tab::open_command_bar_if_no_tabs,
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
pub(crate) struct VmuxWindow;

#[derive(Component)]
pub(crate) struct Main;

/// Vertical stack inside the root flex-row that owns the Header, Main pane
/// area, and Footer. Sits next to the (left) SideSheet so opening the sheet
/// naturally shrinks this column via flex layout — no manual margin pushing
/// is required on the inner panels.
#[derive(Component)]
pub(crate) struct MainColumn;

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
                flex_direction: FlexDirection::Row,
                padding: UiRect {
                    top: Val::Px(settings.layout.window.pad_top()),
                    right: Val::Px(settings.layout.window.pad_right()),
                    bottom: Val::Px(settings.layout.window.pad_bottom()),
                    left: Val::Px(settings.layout.window.pad_left()),
                },
                column_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        })
        .id();

    // Left side sheet: flex child of root. Hidden via `display: None` until
    // opened; when visible it naturally pushes the MainColumn via flex layout.
    commands.spawn((
        SideSheet,
        SideSheetPosition::Left,
        HostWindow(pw),
        Browser,
        WebviewTransparent,
        Node {
            width: Val::Px(settings.layout.side_sheet.width),
            flex_shrink: 0.0,
            display: Display::None,
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
        ChildOf(root),
    ));

    // Vertical stack: Header / Main pane area / Footer.
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
                row_gap: Val::Px(settings.layout.pane.gap),
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    commands.spawn((
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
            })),
            webview_size: WebviewSize(Vec2::new(1280.0, HEADER_HEIGHT_PX)),
        },
        ChildOf(main_column),
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
        ZIndex(1),
        HostWindow(pw),
        Browser,
        WebviewTransparent,
        Node {
            height: Val::Px(FOOTER_HEIGHT_PX),
            flex_shrink: 0.0,
            ..default()
        },
        FooterBundle {
            marker: Footer,
            source: WebviewSource::new(FOOTER_WEBVIEW_URL),
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
            webview_size: WebviewSize(Vec2::new(1280.0, FOOTER_HEIGHT_PX)),
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
            right: Val::Px(settings.layout.window.pad_right()),
            top: Val::Px(settings.layout.window.pad_top()),
            bottom: Val::Px(settings.layout.window.pad_bottom()),
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
            left: Val::Px(settings.layout.window.pad_left()),
            right: Val::Px(settings.layout.window.pad_right()),
            bottom: Val::Px(settings.layout.window.pad_bottom()),
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
        ChildOf(root),
    ));
}

/// Spawns the default session (Profile/Space/Pane/Tab) if none was loaded.
fn spawn_default_session(
    main_q: Query<Entity, With<Main>>,
    profile_q: Query<(), With<Profile>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    settings: Res<AppSettings>,
    mut new_tab_ctx: ResMut<crate::command_bar::NewTabContext>,
    mut commands: Commands,
) {
    // If profiles already exist (loaded from session.ron) or a session
    // file is present (entities may still be arriving from the load
    // observer), skip default session creation.
    if !profile_q.is_empty() || crate::persistence::session_path().exists() {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    let pw = *primary_window;

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

    let gap = pane_split_gaps(PaneSplitDirection::Row, settings.layout.pane.gap);
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
    new_tab_ctx.needs_open = true;
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
                base_color: chrome_glass_base_color(),
                unlit: true,
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

/// Spawns a glass mesh child behind each overlay panel (Header, Footer, SideSheet Left, Modal).
fn spawn_glass_panes(
    header_q: Query<Entity, With<Header>>,
    footer_q: Query<Entity, With<vmux_footer::Footer>>,
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
    for entity in &footer_q {
        spawn_glass_child(&mut commands, plane.clone(), &mut materials, r, entity);
    }
    for (entity, pos) in &side_sheet_q {
        if *pos == SideSheetPosition::Left {
            spawn_glass_child(&mut commands, plane.clone(), &mut materials, r, entity);
        }
    }
    // Modal glass is handled per-tab (glass mesh spawned as child of
    // the new tab's transparent browser in handle_tab_commands).
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

/// Re-applies layout-affecting settings (window padding, row gap, side sheet
/// insets and width) to existing nodes whenever `AppSettings` changes (e.g.
/// after settings.ron hot-reload). Without this, edits to the file produce a
/// "Settings reloaded" log but no visual change because `setup` only reads
/// settings once at Startup.
fn sync_window_layout_to_settings(
    settings: Res<AppSettings>,
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

    let pad_top = settings.layout.window.pad_top();
    let pad_right = settings.layout.window.pad_right();
    let pad_bottom = settings.layout.window.pad_bottom();
    let pad_left = settings.layout.window.pad_left();
    let gap = settings.layout.pane.gap;
    let cfg_width = settings.layout.side_sheet.width;

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

    // MainColumn (Header / Main / Footer stack) row gap.
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn chrome_glass_has_no_fill_color() {
        assert_eq!(chrome_glass_base_color(), Color::srgba(1.0, 1.0, 1.0, 0.0));
    }

    #[test]
    fn default_session_requests_command_bar_open() {
        let _home = HomeEnvGuard::use_temp_home("default-session");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<crate::command_bar::NewTabContext>();
        app.insert_resource(AppSettings {
            browser: crate::settings::BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: crate::settings::LayoutSettings {
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
            },
            shortcuts: crate::settings::ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
        });
        app.add_systems(Update, spawn_default_session);

        app.world_mut().spawn(PrimaryWindow);
        app.world_mut().spawn(Main);

        app.update();

        let ctx = app.world().resource::<crate::command_bar::NewTabContext>();
        assert!(ctx.tab.is_some());
        assert!(ctx.needs_open);
    }
}
