use crate::event::COMMAND_BAR_PAGE_URL;
use crate::{
    Header, LayoutStartupSet, SpaceFilePresent, TabLayoutSpawnContent, TabLayoutSpawnRequest,
    cef::{Browser, layout_cef_bundle},
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    scene::MainCamera,
    settings::LayoutSettings,
    side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
    stack::stack_bundle,
    tab::{Tab, tab_bundle},
    unit::{PIXELS_PER_METER, WindowExt},
};
use bevy::{
    asset::{Asset, load_internal_asset, uuid_handle},
    material::AlphaMode,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    picking::Pickable,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui::{FlexDirection, UiTargetCamera},
    window::{Monitor, PrimaryWindow, WindowMode},
    winit::WINIT_WINDOWS,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, WindowCommand};
use vmux_core::page::ServerEmbedSet;
use vmux_core::{PageOpenRequest, PageOpenSet, PageOpenTarget};
use vmux_history::{CreatedAt, LastActivatedAt};

pub const SIDE_SHEET_TOP_PADDING_PX: f32 = 22.0;

pub const WEBVIEW_Z_MAIN: f32 = 0.018;
pub const WEBVIEW_Z_FOCUS_RING: f32 = 0.02;
pub const WEBVIEW_Z_HEADER: f32 = 0.022;
pub const WEBVIEW_Z_SIDE_SHEET: f32 = 0.022;
pub const WEBVIEW_Z_MODAL: f32 = 0.06;
pub const WEBVIEW_MESH_DEPTH_BIAS: f32 = 0.0;

const WINDOW_SHADER_HANDLE: Handle<Shader> = uuid_handle!("a3e43dbf-9f06-4d0b-8a17-ef8d5ad4d1f4");

const _: () = {
    assert!(WEBVIEW_Z_MAIN <= 0.025);
    assert!(WEBVIEW_Z_FOCUS_RING > WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_HEADER <= 0.03);
    assert!(WEBVIEW_Z_SIDE_SHEET <= 0.03);
    assert!(WEBVIEW_Z_MODAL <= 0.08);
    assert!(WEBVIEW_MESH_DEPTH_BIAS >= 0.0);
};

pub struct WindowPlugin;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq)]
pub struct WindowCorners {
    #[uniform(100)]
    pub clip: Vec4,
    #[uniform(101)]
    pub corner_mode: Vec4,
}

impl Default for WindowCorners {
    fn default() -> Self {
        Self {
            clip: Vec4::new(0.0, 1.0, 1.0, PIXELS_PER_METER),
            corner_mode: Vec4::ZERO,
        }
    }
}

impl MaterialExtension for WindowCorners {
    fn fragment_shader() -> ShaderRef {
        WINDOW_SHADER_HANDLE.into()
    }
}

pub type WindowMaterial = ExtendedMaterial<StandardMaterial, WindowCorners>;

pub const WINDOW_BACKGROUND_SRGB: [f32; 3] = [0.13, 0.13, 0.14];

fn window_background_color() -> Color {
    let [r, g, b] = WINDOW_BACKGROUND_SRGB;
    Color::srgba(r, g, b, 1.0)
}

fn window_surface_alpha(mode: crate::scene::InteractionMode) -> f32 {
    match mode {
        crate::scene::InteractionMode::User => 0.0,
        crate::scene::InteractionMode::Player => 1.0,
    }
}

fn window_surface_alpha_mode(alpha: f32, radius: f32) -> AlphaMode {
    if alpha < 1.0 {
        AlphaMode::Blend
    } else if radius > 0.0 {
        AlphaMode::AlphaToCoverage
    } else {
        AlphaMode::Opaque
    }
}

fn window_background_material(
    radius: f32,
    size_m: Vec2,
    mode: crate::scene::InteractionMode,
) -> WindowMaterial {
    let alpha = window_surface_alpha(mode);
    WindowMaterial {
        base: StandardMaterial {
            base_color: window_background_color().with_alpha(alpha),
            unlit: true,
            alpha_mode: window_surface_alpha_mode(alpha, radius),
            cull_mode: None,
            ..default()
        },
        extension: WindowCorners {
            clip: Vec4::new(radius, size_m.x, size_m.y, PIXELS_PER_METER),
            ..default()
        },
    }
}

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, WINDOW_SHADER_HANDLE, "window.wgsl", Shader::from_wgsl);

        app.add_plugins(MaterialPlugin::<WindowMaterial>::default())
            .register_type::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_systems(
                Startup,
                setup
                    .in_set(LayoutStartupSet::Window)
                    .after(crate::scene::setup)
                    .after(ServerEmbedSet),
            )
            .add_systems(
                Startup,
                (
                    request_default_layout,
                    spawn_requested_tab_layouts,
                    discard_startup_tab_layout_requests,
                )
                    .chain()
                    .in_set(LayoutStartupSet::DefaultTab),
            )
            .add_systems(
                Startup,
                (
                    crate::stack::open_startup_url_if_no_stacks,
                    fit_window_to_screen,
                )
                    .chain()
                    .in_set(LayoutStartupSet::Post),
            )
            .add_systems(
                PostUpdate,
                (
                    fit_window_to_screen,
                    sync_window_surface_clip,
                    sync_window_surface_alpha,
                    apply_webview_material_defaults,
                    sync_window_layout_to_settings,
                    sync_main_column_gap_to_pane_count,
                ),
            )
            .add_systems(
                Update,
                (
                    crate::stack::open_startup_url_if_no_stacks.before(PageOpenSet::ResolveTarget),
                    spawn_requested_tab_layouts
                        .after(ReadAppCommands)
                        .before(PageOpenSet::ResolveTarget),
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
        if let AppCommand::Layout(LayoutCommand::Window(WindowCommand::Minimize)) = cmd {
            let entity = *primary_window;
            WINIT_WINDOWS.with_borrow(|winit_windows| {
                if let Some(winit_win) = winit_windows.get_window(entity) {
                    winit_win.set_minimized(true);
                }
            });
        }
    }
}

pub(crate) fn window_uses_full_padding(window: &Window, monitors: &Query<&Monitor>) -> bool {
    matches!(
        &window.mode,
        WindowMode::BorderlessFullscreen(_) | WindowMode::Fullscreen(_, _)
    ) || window_fills_monitor(window, monitors)
}

fn window_fills_monitor(window: &Window, monitors: &Query<&Monitor>) -> bool {
    let size = window.resolution.physical_size();
    monitors.iter().any(|monitor| {
        let monitor_size = monitor.physical_size();
        size.x >= monitor_size.x.saturating_sub(2) && size.y >= monitor_size.y.saturating_sub(2)
    })
}

#[derive(Bundle)]
struct WindowBundle<M>
where
    M: Material,
{
    marker: VmuxWindow,
    surface: WindowSurface,
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
pub struct WindowSurface;

/// Persisted primary-window geometry, saved as a singleton entity in `store.ron`.
/// `position`/`size` always describe the windowed frame, even while `fullscreen`,
/// so exiting fullscreen lands on a sane frame.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::window"]
#[require(Save)]
pub struct WindowGeometry {
    pub fullscreen: bool,
    pub position: Option<IVec2>,
    pub size: Option<Vec2>,
}

fn setup(
    window: Single<&Window, With<PrimaryWindow>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    main_camera: Single<Entity, With<MainCamera>>,
    mut commands: Commands,
    settings: Res<LayoutSettings>,
    mode: Res<crate::scene::InteractionMode>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WindowMaterial>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    let m = window.meters();
    let pw = *primary_window;

    let root = commands
        .spawn(WindowBundle {
            marker: VmuxWindow,
            surface: WindowSurface,
            mesh: Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            material: MeshMaterial3d(materials.add(window_background_material(
                settings.radius,
                Vec2::new(m.x, m.y),
                *mode,
            ))),
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
                    top: Val::Px(0.0),
                    left: Val::Px(0.0),
                    right: Val::Px(settings.window.pad_right()),
                    bottom: Val::Px(settings.window.pad_bottom()),
                },
                column_gap: Val::Px(0.0),
                ..default()
            },
            ui_target: UiTargetCamera(*main_camera),
        })
        .id();

    let _left_side_sheet = commands
        .spawn((
            SideSheet,
            SideSheetPosition::Left,
            crate::Open,
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Inherited,
            Node {
                width: Val::Px(crate::event::SIDE_SHEET_WIDTH_PX),
                min_height: Val::Px(0.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                padding: UiRect {
                    top: Val::Px(SIDE_SHEET_TOP_PADDING_PX),
                    ..default()
                },
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
                row_gap: Val::Px(0.0),
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        Header,
        crate::Open,
        ZIndex(1),
        Visibility::Inherited,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            height: Val::Px(crate::event::CEF_RESERVED_HEIGHT_PX),
            flex_shrink: 0.0,
            ..default()
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

    // Right & Bottom side sheets remain absolute overlays (slide-in semantics);
    // they're not part of the natural flex layout.
    commands.spawn((
        SideSheet,
        SideSheetPosition::Right,
        crate::Open,
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
        crate::Open,
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
        (
            Modal,
            HostWindow(pw),
            Browser,
            WebviewTransparent,
            WebviewNativeLiquidGlass,
            WebviewWindowedNativeFocus,
            bevy_cef::prelude::WebviewNativeOverlay,
            bevy_cef::prelude::CefIgnorePinchZoom,
        ),
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
        WebviewSource::new(COMMAND_BAR_PAGE_URL),
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
        WebviewSize(Vec2::new(800.0, 600.0)),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Hidden,
        Pickable::IGNORE,
        ChildOf(root),
    ));

    commands.spawn((
        layout_cef_bundle(pw, &mut meshes, &mut webview_mt),
        ChildOf(root),
    ));
}

fn request_default_layout(
    main_q: Query<Entity, With<Main>>,
    tab_q: Query<(), With<Tab>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    space_file: Option<Res<SpaceFilePresent>>,
    mut requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    if !tab_q.is_empty() || space_file.as_deref().is_some_and(|s| s.0) {
        return;
    }

    let Ok(main) = main_q.single() else { return };
    requests.write(TabLayoutSpawnRequest {
        main,
        primary_window: *primary_window,
        name: None,
        content: TabLayoutSpawnContent::StartupUrlOrPrompt,
        clear_pending_stack: false,
        focus: true,
    });
}

fn discard_startup_tab_layout_requests(mut requests: ResMut<Messages<TabLayoutSpawnRequest>>) {
    requests.clear();
}

pub fn spawn_requested_tab_layouts(
    mut reader: MessageReader<TabLayoutSpawnRequest>,
    settings: Res<LayoutSettings>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<crate::NewStackContext>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut focus: Option<ResMut<crate::stack::FocusedStack>>,
    active_space: Query<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>,
    any_space: Query<Entity, With<crate::space::Space>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        // Parent the tab under the active space; on a fresh start no space is
        // marked active yet (ensure_active runs later), so fall back to any
        // existing space so the tab is adopted into a space container (becomes
        // active + visible) instead of being orphaned under Main.
        let parent = active_space
            .iter()
            .next()
            .or_else(|| any_space.iter().next())
            .unwrap_or(request.main);
        let tab_e = commands
            .spawn((
                tab_bundle(),
                LastActivatedAt::now(),
                CreatedAt::now(),
                ChildOf(parent),
            ))
            .id();
        if let Some(name) = request.name.clone() {
            commands.entity(tab_e).insert(Tab { name });
        }

        let gap = pane_split_gaps(PaneSplitDirection::Row, settings.pane.gap);
        let split_root = commands
            .spawn((
                Pane,
                PaneSplit {
                    direction: PaneSplitDirection::Row,
                },
                HostWindow(request.primary_window),
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
                ChildOf(tab_e),
            ))
            .id();

        let leaf = commands
            .spawn((
                leaf_pane_bundle(),
                LastActivatedAt::now(),
                ChildOf(split_root),
            ))
            .id();

        let stack = commands
            .spawn((
                stack_bundle(),
                LastActivatedAt::now(),
                CreatedAt::now(),
                ChildOf(leaf),
            ))
            .id();

        if request.clear_pending_stack
            && let Some(old_stack) = new_stack_ctx.stack.take()
        {
            commands.entity(old_stack).despawn();
        }
        new_stack_ctx.previous_stack = None;
        new_stack_ctx.dismiss_modal = false;

        match &request.content {
            TabLayoutSpawnContent::StartupUrlOrPrompt => {
                let url = effective_startup_url
                    .as_deref()
                    .map(|u| u.0.clone())
                    .unwrap_or_default();
                if url.is_empty() {
                    new_stack_ctx.stack = Some(stack);
                    new_stack_ctx.needs_open = true;
                } else {
                    new_stack_ctx.stack = None;
                    new_stack_ctx.needs_open = false;
                    page_open_requests.write(PageOpenRequest {
                        target: PageOpenTarget::Stack(stack),
                        url,
                        request_id: None,
                    });
                }
            }
            TabLayoutSpawnContent::Url(url) => {
                new_stack_ctx.stack = None;
                new_stack_ctx.needs_open = false;
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: url.clone(),
                    request_id: None,
                });
            }
        }

        if request.focus
            && let Some(focus) = focus.as_deref_mut()
        {
            focus.tab = Some(tab_e);
            focus.pane = Some(leaf);
            focus.stack = Some(stack);
        }
    }
}

fn sync_window_surface_clip(
    settings: Res<LayoutSettings>,
    mut materials: ResMut<Assets<WindowMaterial>>,
    q: Query<&MeshMaterial3d<WindowMaterial>, With<WindowSurface>>,
) {
    if !settings.is_changed() {
        return;
    }
    for handle in &q {
        if let Some(mut mat) = materials.get_mut(handle) {
            let clip = &mut mat.extension.clip;
            if (clip.x - settings.radius).abs() > 0.01 {
                clip.x = settings.radius;
                mat.base.alpha_mode =
                    window_surface_alpha_mode(mat.base.base_color.alpha(), settings.radius);
            }
        }
    }
}

fn sync_window_surface_alpha(
    mode: Res<crate::scene::InteractionMode>,
    mut materials: ResMut<Assets<WindowMaterial>>,
    q: Query<&MeshMaterial3d<WindowMaterial>, With<WindowSurface>>,
) {
    if !mode.is_changed() {
        return;
    }
    let alpha = window_surface_alpha(*mode);
    for handle in &q {
        if let Some(mut mat) = materials.get_mut(handle) {
            mat.base.base_color = mat.base.base_color.with_alpha(alpha);
            mat.base.alpha_mode = window_surface_alpha_mode(alpha, mat.extension.clip.x);
        }
    }
}

fn apply_webview_material_defaults(
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    q: Query<
        &MeshMaterial3d<WebviewExtendStandardMaterial>,
        Or<(
            Added<WebviewSource>,
            Changed<MeshMaterial3d<WebviewExtendStandardMaterial>>,
        )>,
    >,
) {
    for handle in &q {
        if let Some(mut material) = materials.get_mut(handle) {
            material.base.unlit = true;
            material.base.alpha_mode = AlphaMode::Blend;
            material.base.depth_bias = WEBVIEW_MESH_DEPTH_BIAS;
            material.base.cull_mode = None;
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
    hidden: Option<Res<crate::toggle::LayoutHidden>>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
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
    let gap = crate::event::PANE_GAP_PX;
    let cfg_width = crate::event::SIDE_SHEET_WIDTH_PX;
    let full_padding = hidden.as_deref().is_some_and(|hidden| hidden.0)
        || primary_window
            .single()
            .ok()
            .is_some_and(|window| window_uses_full_padding(window, &monitors));

    // Root window: padding + flex-row column gap. Top and left are flush
    // with the window so the CEF shell / pane meet the system edge; right
    // and bottom keep a gap.
    if let Ok(mut node) = window_q.single_mut() {
        node.padding = UiRect {
            top: Val::Px(if full_padding { pad_top } else { 0.0 }),
            left: Val::Px(if full_padding { pad_left } else { 0.0 }),
            right: Val::Px(pad_right),
            bottom: Val::Px(pad_bottom),
        };
        node.column_gap = Val::Px(gap);
    }

    // MainColumn row_gap (between Header and Main pane container) is
    // managed by sync_main_column_gap_to_pane_count, which keeps it 0
    // when the active tab has a single pane and switches to the window
    // padding when split. Don't override here.
    let _ = main_column_q.single_mut();

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

/// Keep MainColumn's row_gap at 0 when the active tab has a single pane
/// (so the url row sits flush against the pane content) and switch to the
/// window's top padding when it's split (so the panes get a visible gap
/// below the url bar, matching their outer padding).
fn sync_main_column_gap_to_pane_count(
    focus: Res<crate::stack::FocusedStack>,
    settings: Res<LayoutSettings>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    mut main_column_q: Query<&mut Node, With<MainColumn>>,
) {
    let pane_count = focus
        .tab
        .map(|tab_e| {
            let mut leaves = Vec::new();
            crate::stack::collect_leaf_panes(tab_e, &all_children, &leaf_panes, &mut leaves);
            leaves.len()
        })
        .unwrap_or(0);
    let target = if pane_count > 1 {
        settings.window.pad_top()
    } else {
        0.0
    };
    for mut node in &mut main_column_q {
        let current = match node.row_gap {
            Val::Px(v) => v,
            _ => f32::NAN,
        };
        if (current - target).abs() > 0.01 {
            node.row_gap = Val::Px(target);
        }
    }
}

pub fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    settings: Res<LayoutSettings>,
    mut materials: ResMut<Assets<WindowMaterial>>,
    mut last_size: Local<Vec2>,
    mut q: Query<(&mut Transform, &MeshMaterial3d<WindowMaterial>), With<VmuxWindow>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    let r = settings.radius;

    for (mut tf, handle) in &mut q {
        tf.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        tf.scale = Vec3::new(m.x, m.y, 1.0);

        if let Some(mut mat) = materials.get_mut(handle) {
            mat.extension.clip = Vec4::new(r, m.x, m.y, PIXELS_PER_METER);
            mat.base.alpha_mode = window_surface_alpha_mode(mat.base.base_color.alpha(), r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cef::LayoutCef;
    use bevy::ecs::relationship::Relationship;
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
    fn window_uses_dark_finder_style_background() {
        assert_eq!(
            window_background_color(),
            Color::srgba(0.13, 0.13, 0.14, 1.0)
        );
    }

    #[test]
    fn window_surface_is_transparent_in_user_mode() {
        assert_eq!(
            window_surface_alpha(crate::scene::InteractionMode::User),
            0.0
        );
    }

    #[test]
    fn window_surface_is_opaque_in_player_mode() {
        assert_eq!(
            window_surface_alpha(crate::scene::InteractionMode::Player),
            1.0
        );
    }

    #[test]
    fn window_background_material_is_opaque_in_player_mode() {
        let material = window_background_material(
            0.0,
            Vec2::new(4.0, 3.0),
            crate::scene::InteractionMode::Player,
        );

        assert_eq!(material.base.base_color.alpha(), 1.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
        assert_eq!(material.base.cull_mode, None);
        assert_eq!(material.base.specular_transmission, 0.0);
        assert_eq!(material.base.diffuse_transmission, 0.0);
    }

    #[test]
    fn window_background_material_alpha_to_coverage_for_rounded_player_corners() {
        let material = window_background_material(
            12.0,
            Vec2::new(4.0, 3.0),
            crate::scene::InteractionMode::Player,
        );

        assert_eq!(material.base.base_color.alpha(), 1.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
    }

    #[test]
    fn sync_window_surface_alpha_preserves_rounded_player_corner_alpha_to_coverage() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::scene::InteractionMode::User)
            .init_resource::<Assets<WindowMaterial>>()
            .add_systems(Update, sync_window_surface_alpha);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WindowMaterial>>()
            .add(window_background_material(
                12.0,
                Vec2::new(4.0, 3.0),
                crate::scene::InteractionMode::User,
            ));
        app.world_mut()
            .spawn((WindowSurface, MeshMaterial3d(handle.clone())));

        let mut mode = app
            .world_mut()
            .resource_mut::<crate::scene::InteractionMode>();
        *mode = crate::scene::InteractionMode::Player;

        app.update();

        let material = app
            .world()
            .resource::<Assets<WindowMaterial>>()
            .get(&handle)
            .expect("window material");

        assert_eq!(material.base.base_color.alpha(), 1.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::AlphaToCoverage);
    }

    #[test]
    fn window_background_material_is_transparent_in_user_mode() {
        let material = window_background_material(
            12.0,
            Vec2::new(4.0, 3.0),
            crate::scene::InteractionMode::User,
        );

        assert_eq!(material.base.base_color.alpha(), 0.0);
        assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
    }

    #[test]
    fn window_background_material_keeps_corner_clip() {
        let material = window_background_material(
            12.0,
            Vec2::new(4.0, 3.0),
            crate::scene::InteractionMode::Player,
        );

        assert_eq!(
            material.extension.clip,
            Vec4::new(12.0, 4.0, 3.0, PIXELS_PER_METER)
        );
        assert_eq!(material.extension.corner_mode, Vec4::ZERO);
    }

    #[test]
    fn apply_webview_material_defaults_renders_from_both_sides() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, apply_webview_material_defaults);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WebviewExtendStandardMaterial>>()
            .add(WebviewExtendStandardMaterial::default());
        app.world_mut().spawn((
            WebviewSource::new("https://example.com/"),
            MeshMaterial3d(handle.clone()),
        ));
        app.update();

        let material = app
            .world()
            .resource::<Assets<WebviewExtendStandardMaterial>>()
            .get(&handle)
            .expect("webview material");

        assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.base.depth_bias, WEBVIEW_MESH_DEPTH_BIAS);
        assert_eq!(material.base.cull_mode, None);
    }

    fn test_settings(gap: f32) -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings {
                padding: 0.0,
            },
            pane: crate::settings::PaneSettings { gap },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        }
    }

    fn setup_window_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::scene::InteractionMode::User)
            .insert_resource(test_settings(8.0))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WindowMaterial>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>();
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn(crate::scene::MainCamera);
        app.add_systems(Startup, setup);
        app
    }

    #[test]
    fn header_lives_in_main_column_above_main() {
        let mut app = setup_window_app();
        app.update();

        let header = app
            .world_mut()
            .query_filtered::<Entity, With<Header>>()
            .single(app.world())
            .expect("header");
        let main_col = app
            .world_mut()
            .query_filtered::<Entity, With<MainColumn>>()
            .single(app.world())
            .expect("main column");
        let parent = app
            .world()
            .get::<ChildOf>(header)
            .map(Relationship::get)
            .expect("header parent");

        assert_eq!(parent, main_col);
    }

    #[test]
    fn setup_spawns_one_window_surface() {
        let mut app = setup_window_app();
        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<WindowSurface>>()
            .iter(app.world())
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn command_bar_modal_backend_is_mode_driven() {
        let mut app = setup_window_app();
        app.update();

        let modal = app
            .world_mut()
            .query_filtered::<Entity, With<Modal>>()
            .single(app.world())
            .expect("modal");

        assert!(app.world().get::<WebviewWindowed>(modal).is_none());
    }

    #[test]
    fn layout_uses_transparent_osr_native_overlay() {
        let mut app = setup_window_app();
        app.update();

        let layout_shell = app
            .world_mut()
            .query_filtered::<Entity, With<LayoutCef>>()
            .single(app.world())
            .expect("layout shell");
        let modal = app
            .world_mut()
            .query_filtered::<Entity, With<Modal>>()
            .single(app.world())
            .expect("modal");

        assert!(
            app.world()
                .get::<WebviewOpaqueWindowedBackground>(layout_shell)
                .is_none()
        );
        assert!(app.world().get::<WebviewWindowed>(layout_shell).is_none());
        assert!(
            app.world()
                .get::<WebviewTransparent>(layout_shell)
                .is_some()
        );
        assert!(
            app.world()
                .get::<WebviewNativeOverlay>(layout_shell)
                .is_none()
        );
        assert!(
            app.world()
                .get::<WebviewMaxFrameRate>(layout_shell)
                .is_none()
        );
        assert!(
            app.world()
                .get::<WebviewOpaqueWindowedBackground>(modal)
                .is_none()
        );
        assert!(app.world().get::<WebviewNativeLiquidGlass>(modal).is_some());
    }

    #[test]
    fn command_bar_modal_allows_windowed_native_focus() {
        let mut app = setup_window_app();
        app.update();

        let modal = app
            .world_mut()
            .query_filtered::<Entity, With<Modal>>()
            .single(app.world())
            .expect("modal");

        assert!(
            app.world()
                .get::<WebviewWindowedNativeFocus>(modal)
                .is_some()
        );
    }

    #[test]
    fn default_tab_requests_command_bar_open() {
        let _home = HomeEnvGuard::use_temp_home("default-tab");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::NewStackContext>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings {
                    padding: 0.0,
                },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .add_systems(
                Update,
                (request_default_layout, spawn_requested_tab_layouts).chain(),
            );

        app.world_mut().spawn(PrimaryWindow);
        app.world_mut().spawn(Main);

        app.update();

        let ctx = app.world().resource::<crate::NewStackContext>();
        assert!(ctx.stack.is_some());
        assert!(ctx.needs_open);
    }

    #[test]
    fn cold_start_seeds_exactly_one_default_tab() {
        let _home = HomeEnvGuard::use_temp_home("cold-start-one-tab");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::NewStackContext>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings {
                    padding: 0.0,
                },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .insert_resource(crate::settings::EffectiveStartupUrl(
                "vmux://agent/vibe/".to_string(),
            ))
            .add_systems(
                Startup,
                (
                    request_default_layout,
                    spawn_requested_tab_layouts,
                    discard_startup_tab_layout_requests,
                )
                    .chain(),
            )
            .add_systems(Update, spawn_requested_tab_layouts);

        app.world_mut().spawn(PrimaryWindow);
        app.world_mut().spawn(Main);

        app.update();

        let mut tabs = app.world_mut().query_filtered::<Entity, With<Tab>>();
        assert_eq!(
            tabs.iter(app.world()).count(),
            1,
            "cold start must seed exactly one default tab; the Startup-written request must not be re-read by the Update consumer"
        );
    }

    #[test]
    fn default_tab_adopts_existing_space_when_none_active() {
        use bevy::ecs::relationship::Relationship;
        let _home = HomeEnvGuard::use_temp_home("default-tab-adopts-space");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::NewStackContext>()
            .add_message::<crate::TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings {
                    padding: 0.0,
                },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .insert_resource(crate::settings::EffectiveStartupUrl(
                "vmux://agent/vibe/".to_string(),
            ))
            .add_systems(
                Startup,
                (request_default_layout, spawn_requested_tab_layouts).chain(),
            );

        app.world_mut().spawn(Main);
        app.world_mut().spawn(PrimaryWindow);
        // Fresh start: a space exists but isn't Active yet (ensure_active runs in
        // Update, after this Startup). The default tab must still be adopted into
        // the space so it becomes active + visible — not orphaned under Main.
        let space = app.world_mut().spawn(crate::space::Space).id();

        app.update();

        let mut tabs = app.world_mut().query_filtered::<&ChildOf, With<Tab>>();
        let child_of = tabs
            .iter(app.world())
            .next()
            .expect("a default tab should be spawned");
        assert_eq!(
            child_of.get(),
            space,
            "default tab must be parented under the existing space, not Main"
        );
    }

    #[test]
    fn window_padding_tracks_layout_window_settings() {
        let source = include_str!("window.rs");
        let sync_fn = source
            .split("fn sync_window_layout_to_settings")
            .nth(1)
            .and_then(|tail| tail.split("fn sync_main_column_gap_to_pane_count").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("settings.window.pad_top()"));
        assert!(sync_fn.contains("settings.window.pad_right()"));
        assert!(sync_fn.contains("settings.window.pad_bottom()"));
        assert!(sync_fn.contains("settings.window.pad_left()"));
    }

    #[test]
    fn fills_monitor_window_sync_applies_all_window_padding_edges() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::toggle::LayoutHidden(false))
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings {
                    padding: 16.0,
                },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .insert_resource(SideSheetWidth(0.0))
            .add_systems(Update, sync_window_layout_to_settings);
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn(Monitor {
            name: None,
            physical_width: 1200,
            physical_height: 800,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: None,
            scale_factor: 1.0,
            video_modes: Vec::new(),
        });
        let root = app.world_mut().spawn((VmuxWindow, Node::default())).id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(16.0));
        assert_eq!(node.padding.left, Val::Px(16.0));
        assert_eq!(node.padding.right, Val::Px(16.0));
        assert_eq!(node.padding.bottom, Val::Px(16.0));
    }

    #[test]
    fn window_geometry_round_trips_position_size_fullscreen() {
        let g = WindowGeometry {
            fullscreen: true,
            position: Some(IVec2::new(100, 200)),
            size: Some(Vec2::new(1280.0, 800.0)),
        };
        assert_eq!(g, g);
        assert_eq!(g.position, Some(IVec2::new(100, 200)));
        assert_eq!(g.size, Some(Vec2::new(1280.0, 800.0)));
        assert!(g.fullscreen);
        assert_eq!(
            WindowGeometry::default(),
            WindowGeometry {
                fullscreen: false,
                position: None,
                size: None,
            }
        );
    }
}
