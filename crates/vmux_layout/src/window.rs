use crate::event::COMMAND_BAR_PAGE_URL;
#[cfg(feature = "player-mode")]
use crate::unit::PIXELS_PER_METER;
use crate::{
    Header, LayoutStartupSet, SpaceFilePresent, TabLayoutSpawnContent, TabLayoutSpawnRequest,
    cef::{Browser, layout_cef_bundle},
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    scene::MainCamera,
    settings::LayoutSettings,
    side_sheet::{SideSheet, SideSheetPosition, SideSheetWidth},
    stack::stack_bundle,
    tab::{Tab, tab_bundle},
    unit::WindowExt,
};
use bevy::{
    asset::Asset,
    picking::Pickable,
    prelude::*,
    ui::{FlexDirection, UiTargetCamera},
    window::PrimaryWindow,
    winit::WINIT_WINDOWS,
};
#[cfg(feature = "player-mode")]
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    material::AlphaMode,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, WindowCommand};
use vmux_core::page::ServerEmbedSet;
use vmux_core::{PageOpenRequest, PageOpenSet, PageOpenTarget};
use vmux_history::{CreatedAt, LastActivatedAt};

pub struct WindowLayoutPlugin;

impl Plugin for WindowLayoutPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "player-mode")]
        load_internal_asset!(app, WINDOW_SHADER_HANDLE, "window.wgsl", Shader::from_wgsl);

        app.register_type::<WindowGeometry>()
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

        #[cfg(feature = "player-mode")]
        app.add_plugins(MaterialPlugin::<WindowMaterial>::default())
            .add_systems(
                PostUpdate,
                (
                    sync_window_surface_clip,
                    sync_window_surface_alpha,
                    apply_webview_material_defaults,
                ),
            );

        #[cfg(not(feature = "player-mode"))]
        app.init_resource::<Assets<WindowMaterial>>();
    }
}

pub const SIDE_SHEET_TOP_PADDING_PX: f32 = 22.0;

pub const WEBVIEW_Z_MAIN: f32 = 0.018;
pub const WEBVIEW_Z_FOCUS_RING: f32 = 0.02;
pub const WEBVIEW_Z_HEADER: f32 = 0.022;
pub const WEBVIEW_Z_SIDE_SHEET: f32 = 0.022;
pub const WEBVIEW_Z_MODAL: f32 = 0.06;
pub const WEBVIEW_MESH_DEPTH_BIAS: f32 = 0.0;

#[cfg(feature = "player-mode")]
const WINDOW_SHADER_HANDLE: Handle<Shader> = uuid_handle!("a3e43dbf-9f06-4d0b-8a17-ef8d5ad4d1f4");

const _: () = {
    assert!(WEBVIEW_Z_MAIN <= 0.025);
    assert!(WEBVIEW_Z_FOCUS_RING > WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_HEADER <= 0.03);
    assert!(WEBVIEW_Z_SIDE_SHEET <= 0.03);
    assert!(WEBVIEW_Z_MODAL <= 0.08);
    assert!(WEBVIEW_MESH_DEPTH_BIAS >= 0.0);
};

#[cfg(feature = "player-mode")]
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq)]
pub struct WindowCorners {
    #[uniform(100)]
    pub clip: Vec4,
    #[uniform(101)]
    pub corner_mode: Vec4,
}

#[cfg(feature = "player-mode")]
impl Default for WindowCorners {
    fn default() -> Self {
        Self {
            clip: Vec4::new(0.0, 1.0, 1.0, PIXELS_PER_METER),
            corner_mode: Vec4::ZERO,
        }
    }
}

#[cfg(feature = "player-mode")]
impl MaterialExtension for WindowCorners {
    fn fragment_shader() -> ShaderRef {
        WINDOW_SHADER_HANDLE.into()
    }
}

#[cfg(feature = "player-mode")]
pub type WindowMaterial = ExtendedMaterial<StandardMaterial, WindowCorners>;

#[cfg(not(feature = "player-mode"))]
#[derive(Asset, TypePath)]
pub struct WindowMaterial;

pub const WINDOW_BACKGROUND_SRGB: [f32; 3] = [0.13, 0.13, 0.14];

#[cfg(feature = "player-mode")]
fn window_background_color() -> Color {
    let [r, g, b] = WINDOW_BACKGROUND_SRGB;
    Color::srgba(r, g, b, 1.0)
}

#[cfg(feature = "player-mode")]
fn window_surface_alpha(mode: crate::scene::InteractionMode) -> f32 {
    match mode {
        crate::scene::InteractionMode::User => 0.0,
        crate::scene::InteractionMode::Player => 1.0,
    }
}

#[cfg(feature = "player-mode")]
fn window_surface_alpha_mode(alpha: f32, radius: f32) -> AlphaMode {
    if alpha < 1.0 {
        AlphaMode::Blend
    } else if radius > 0.0 {
        AlphaMode::AlphaToCoverage
    } else {
        AlphaMode::Opaque
    }
}

#[cfg(feature = "player-mode")]
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

#[derive(Bundle)]
struct WindowBundle {
    marker: VmuxWindow,
    surface: WindowSurface,
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
    #[cfg(not(feature = "player-mode"))]
    let _ = (&mode, &mut materials);

    let root_commands = commands.spawn(WindowBundle {
        marker: VmuxWindow,
        surface: WindowSurface,
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
            column_gap: Val::Px(crate::event::PANE_GAP_PX),
            ..default()
        },
        ui_target: UiTargetCamera(*main_camera),
    });
    #[cfg(feature = "player-mode")]
    let mut root_commands = root_commands;
    #[cfg(feature = "player-mode")]
    root_commands.insert((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
        MeshMaterial3d(materials.add(window_background_material(
            settings.radius,
            Vec2::new(m.x, m.y),
            *mode,
        ))),
    ));
    let root = root_commands.id();

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
            // OSR, composited as a native overlay above the windowed pages. The page paints its
            // own themed shell through the transparent surface.
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
        WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
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
    tab_q: Query<(), With<Tab>>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    space_file: Option<Res<SpaceFilePresent>>,
    effective_startup_dir: Option<Res<crate::settings::EffectiveStartupDir>>,
    mut requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    if !tab_q.is_empty() || space_file.as_deref().is_some_and(|s| s.0) {
        return;
    }

    let Some((space, startup_dir)) = effective_startup_dir
        .as_deref()
        .and_then(|effective| effective.0.clone())
    else {
        return;
    };
    requests.write(TabLayoutSpawnRequest {
        space,
        primary_window: *primary_window,
        name: None,
        startup_dir: startup_dir.clone(),
        content: TabLayoutSpawnContent::StartupUrlOrPrompt,
        clear_pending_stack: false,
        focus: true,
    });
}

fn discard_startup_tab_layout_requests(mut requests: ResMut<Messages<TabLayoutSpawnRequest>>) {
    requests.clear();
}

pub struct TabScaffold {
    pub tab: Entity,
    pub pane: Entity,
    pub stack: Entity,
}

pub fn spawn_tab_scaffold_in_space(
    commands: &mut Commands,
    space: Entity,
    primary_window: Entity,
    gap_px: f32,
) -> TabScaffold {
    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(space),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, gap_px);
    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            HostWindow(primary_window),
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
            ChildOf(tab),
        ))
        .id();

    let pane = commands
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
            ChildOf(pane),
        ))
        .id();

    TabScaffold { tab, pane, stack }
}

pub fn spawn_requested_tab_layouts(
    mut reader: MessageReader<TabLayoutSpawnRequest>,
    settings: Res<LayoutSettings>,
    effective_startup_url: Option<Res<crate::settings::EffectiveStartupUrl>>,
    mut new_stack_ctx: ResMut<crate::NewStackContext>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut focus: Option<ResMut<crate::stack::FocusedStack>>,
    spaces: Query<(), With<crate::space::Space>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        if spaces.get(request.space).is_err() {
            continue;
        }
        let startup_dir = request
            .startup_dir
            .as_ref()
            .and_then(|startup_dir| startup_dir.canonicalize().ok())
            .filter(|startup_dir| startup_dir.is_dir())
            .and_then(|startup_dir| startup_dir.to_str().map(str::to_string));
        let TabScaffold {
            tab: tab_e,
            pane: leaf,
            stack,
        } = spawn_tab_scaffold_in_space(
            &mut commands,
            request.space,
            request.primary_window,
            settings.pane.gap,
        );
        commands.entity(tab_e).insert(Tab {
            name: request.name.clone().unwrap_or_default(),
            startup_dir,
        });
        if !request.focus {
            commands.entity(tab_e).insert(LastActivatedAt(0));
            commands.entity(leaf).insert(LastActivatedAt(0));
            commands.entity(stack).insert(LastActivatedAt(0));
        }
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
            TabLayoutSpawnContent::Url {
                url,
                pending_prompt,
            } => {
                new_stack_ctx.stack = None;
                new_stack_ctx.needs_open = false;
                if let Some(prompt) = pending_prompt {
                    commands
                        .entity(stack)
                        .insert(vmux_core::PendingPrompt(prompt.clone()));
                }
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

#[cfg(feature = "player-mode")]
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

#[cfg(feature = "player-mode")]
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

#[cfg(feature = "player-mode")]
fn apply_webview_material_defaults(
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    q: Query<
        &WebviewMaterialHandle<WebviewExtendStandardMaterial>,
        Or<(
            Added<WebviewSource>,
            Changed<WebviewMaterialHandle<WebviewExtendStandardMaterial>>,
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
    let full_padding = hidden.as_deref().is_some_and(|hidden| hidden.0);

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

#[cfg(feature = "player-mode")]
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

#[cfg(not(feature = "player-mode"))]
pub fn fit_window_to_screen(
    window: Single<&bevy::window::Window, With<PrimaryWindow>>,
    mut last_size: Local<Vec2>,
    mut q: Query<&mut Transform, With<VmuxWindow>>,
) {
    let m = window.meters();
    if (m.x - last_size.x).abs() < 0.001 && (m.y - last_size.y).abs() < 0.001 {
        return;
    }
    *last_size = m;

    for mut transform in &mut q {
        transform.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        transform.scale = Vec3::new(m.x, m.y, 1.0);
    }
}

#[cfg(test)]
#[path = "window.test.rs"]
mod tests;
