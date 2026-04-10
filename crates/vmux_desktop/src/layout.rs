use bevy::asset::*;
use bevy::camera::CameraUpdateSystems;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::ui::UiSystems;
use bevy::window::{PrimaryWindow, Window as NativeWindow};

use crate::command::{AppCommand, PaneCommand, ReadAppCommands, SpaceCommand};
use crate::settings::{AppSettings, LoadAppSettings};
use vmux_history::{CreatedAt, LastActivatedAt};

pub struct LayoutPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabLayoutSync;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MaterialPlugin::<OutlineMaterial>::default(),))
            .init_resource::<InFocusNode>()
            .configure_sets(PostUpdate, TabLayoutSync.after(UiSystems::Layout))
            .add_systems(
                Startup,
                (spawn_global_outline, spawn_space_on_startup)
                    .chain()
                    .after(LoadAppSettings),
            )
            .add_systems(PreUpdate, sync_pane_flex_from_orientation)
            .add_systems(
                PostUpdate,
                (
                    sync_in_focus_path.before(TabLayoutSync),
                    sync_layout_from_ui.in_set(TabLayoutSync),
                ),
            )
            .add_systems(
                PostUpdate,
                sync_nomadic_outline
                    .after(TabLayoutSync)
                    .after(CameraUpdateSystems),
            )
            .add_systems(Update, tick_outline_gradient_time)
            .add_observer(attach_pane_pointer_observer)
            .add_systems(
                Update,
                (
                    on_new_space_command,
                    on_split_vertically_command,
                    on_split_horizontally_command,
                )
                    .in_set(ReadAppCommands),
            );
        load_internal_asset!(app, OUTLINE_SHADER, "./outline.wgsl", Shader::from_wgsl);
    }
}

#[derive(Bundle, Default)]
struct SpatialBundle {
    transform: Transform,
    global_transform: GlobalTransform,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
}

#[derive(Bundle)]
struct SpaceBundle {
    space: Space,
    node: Node,
    name: Name,
    spatial: SpatialBundle,
}

#[derive(Component)]
struct Space;

#[derive(Bundle)]
struct WindowBundle {
    window: Window,
    node: Node,
    name: Name,
    spatial: SpatialBundle,
}

#[derive(Component)]
struct Window;

#[derive(Bundle)]
struct PaneBundle {
    pane: Pane,
    orientation: Orientation,
    weight: Weight,
    node: Node,
    pickable: Pickable,
    name: Name,
    spatial: SpatialBundle,
}

impl PaneBundle {
    fn new(orientation: Orientation, weight: f32) -> Self {
        let w = Weight(weight);
        Self {
            pane: Pane,
            orientation,
            weight: w,
            node: pane_flex_node_for(orientation, weight),
            pickable: Pickable::default(),
            name: Name::new(format!("Pane {:.2}", w.0)),
            spatial: SpatialBundle::default(),
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Pane;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

impl Orientation {
    fn flex_direction(self) -> FlexDirection {
        match self {
            Orientation::Horizontal => FlexDirection::Row,
            Orientation::Vertical => FlexDirection::Column,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct Weight(f32);

#[derive(Component)]
struct NomadicOutline;

#[derive(Component)]
pub(crate) struct InFocusPath;

#[derive(Component)]
pub(crate) struct Focused;

#[derive(Resource, Default)]
pub(crate) struct InFocusNode(pub Option<Entity>);

fn attach_pane_pointer_observer(add: On<Add, Pane>, mut commands: Commands) {
    commands.entity(add.entity).observe(pane_pointer_set_focus);
}

fn pane_pointer_set_focus(
    click: On<Pointer<Click>>,
    mut focus_node: ResMut<InFocusNode>,
    mut commands: Commands,
    pane_q: Query<(), With<Pane>>,
    children_q: Query<&Children>,
    tabs: Query<Entity, With<Tab>>,
    focused_tabs: Query<Entity, With<Focused>>,
) {
    if !pane_q.contains(click.entity) {
        return;
    }
    focus_node.0 = Some(click.entity);
    for tab in &focused_tabs {
        commands.entity(tab).remove::<Focused>();
    }
    let Ok(children) = children_q.get(click.entity) else {
        return;
    };
    for child in children.iter() {
        if tabs.contains(child) {
            commands.entity(child).insert(Focused);
            return;
        }
    }
}

fn sync_pane_flex_from_orientation(mut q: Query<(&Orientation, &Weight, &mut Node), With<Pane>>) {
    for (orientation, weight, mut node) in &mut q {
        let dir = orientation.flex_direction();
        if node.flex_direction != dir {
            node.flex_direction = dir;
        }
        let g = weight.0;
        if node.flex_grow != g {
            node.flex_grow = g;
        }
    }
}

fn sync_in_focus_path(
    mut commands: Commands,
    had_path: Query<Entity, With<InFocusPath>>,
    focused_tab: Query<Entity, (With<Tab>, With<Focused>)>,
    child_of: Query<&ChildOf>,
) {
    for e in &had_path {
        commands.entity(e).remove::<InFocusPath>();
    }
    let Ok(tab) = focused_tab.single() else {
        return;
    };
    let mut cur = child_of.get(tab).ok().map(|c| c.parent());
    while let Some(e) = cur {
        commands.entity(e).insert(InFocusPath);
        cur = child_of.get(e).ok().map(|c| c.parent());
    }
}

const OUTLINE_SHADER: Handle<Shader> = uuid_handle!("c4a8e901-2b7d-4c1e-9f63-7a2d8e5b1044");

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub(crate) struct OutlineMaterial {
    #[uniform(0)]
    pub pane_inner: Vec4,
    #[uniform(1)]
    pub pane_outer: Vec4,
    #[uniform(2)]
    pub border_color: Vec4,
    #[uniform(3)]
    pub glow_params: Vec4,
    #[uniform(4)]
    pub gradient_params: Vec4,
    #[uniform(5)]
    pub border_accent: Vec4,
    pub alpha_mode: AlphaMode,
}

impl Material for OutlineMaterial {
    fn fragment_shader() -> ShaderRef {
        OUTLINE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct LayoutPlane {
    pub inner_px: Vec2,
    pub border_px: f32,
    pub r_px: f32,
    pub inner_world_half: Vec2,
    pub outer_world_half: Vec2,
}

impl Default for LayoutPlane {
    fn default() -> Self {
        Self {
            inner_px: Vec2::splat(800.0),
            border_px: 0.0,
            r_px: 0.0,
            inner_world_half: Vec2::ONE,
            outer_world_half: Vec2::ONE,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Tab;

#[derive(Bundle)]
struct TabBundle {
    tab: Tab,
    node: Node,
    name: Name,
    spatial: SpatialBundle,
    created_at: CreatedAt,
    last_activated_at: LastActivatedAt,
    layout_plane: LayoutPlane,
}

impl SpaceBundle {
    fn new(padding: f32) -> Self {
        Self {
            space: Space,
            node: space_root_node(padding),
            name: Name::new("Space 1"),
            spatial: SpatialBundle::default(),
        }
    }
}

impl Default for WindowBundle {
    fn default() -> Self {
        Self {
            window: Window,
            node: window_flex_node(),
            name: Name::new("Default Window"),
            spatial: SpatialBundle::default(),
        }
    }
}

impl Default for TabBundle {
    fn default() -> Self {
        Self {
            tab: Tab,
            node: tab_flex_node(),
            name: Name::new("New Tab"),
            spatial: SpatialBundle::default(),
            created_at: CreatedAt::default(),
            last_activated_at: LastActivatedAt::default(),
            layout_plane: LayoutPlane::default(),
        }
    }
}

fn spawn_space_on_startup(mut commands: Commands, q: Query<&Space>, mut writer: MessageWriter<AppCommand>) {
    if q.is_empty() {
        writer.write(AppCommand::Space(SpaceCommand::New));
    }
}

fn on_new_space_command(
    mut reader: MessageReader<AppCommand>,
    mut commands: Commands,
    mut focus_node: ResMut<InFocusNode>,
    settings: Res<AppSettings>,
    camera_q: Query<Entity, With<Camera3d>>,
) {
    for cmd in reader.read() {
        let AppCommand::Space(SpaceCommand::New) = *cmd else {
            continue;
        };
        let Ok(camera) = camera_q.single() else {
            return;
        };
        let inset = settings.layout.window.padding.max(0.0);
        let mut pane_entity = Entity::PLACEHOLDER;
        commands
            .spawn((SpaceBundle::new(inset), UiTargetCamera(camera)))
            .with_children(|space| {
                space
                    .spawn(WindowBundle::default())
                    .with_children(|window| {
                        window
                            .spawn(PaneBundle::new(Orientation::default(), 1.0))
                            .with_children(|pane| {
                                pane_entity = pane.target_entity();
                                pane.spawn((TabBundle::default(), Focused));
                            });
                    });
            });
        focus_node.0 = Some(pane_entity);
    }
}

fn on_split_vertically_command(
    mut reader: MessageReader<AppCommand>,
    mut commands: Commands,
    mut focus_node: ResMut<InFocusNode>,
    children_q: Query<&Children>,
    tabs: Query<Entity, With<Tab>>,
    panes: Query<(), With<Pane>>,
    focused_tabs: Query<Entity, With<Focused>>,
    mut pane_orientations: Query<&mut Orientation, With<Pane>>,
) {
    for cmd in reader.read() {
        let AppCommand::Pane(PaneCommand::SplitV) = *cmd else {
            continue;
        };
        split_focused_pane(
            &mut commands,
            &mut *focus_node,
            Orientation::Horizontal,
            &children_q,
            &tabs,
            &panes,
            &focused_tabs,
            &mut pane_orientations,
        );
    }
}

fn on_split_horizontally_command(
    mut reader: MessageReader<AppCommand>,
    mut commands: Commands,
    mut focus_node: ResMut<InFocusNode>,
    children_q: Query<&Children>,
    tabs: Query<Entity, With<Tab>>,
    panes: Query<(), With<Pane>>,
    focused_tabs: Query<Entity, With<Focused>>,
    mut pane_orientations: Query<&mut Orientation, With<Pane>>,
) {
    for cmd in reader.read() {
        let AppCommand::Pane(PaneCommand::SplitH) = *cmd else {
            continue;
        };
        split_focused_pane(
            &mut commands,
            &mut *focus_node,
            Orientation::Vertical,
            &children_q,
            &tabs,
            &panes,
            &focused_tabs,
            &mut pane_orientations,
        );
    }
}

fn split_focused_pane(
    commands: &mut Commands,
    focus_node: &mut InFocusNode,
    split_axis: Orientation,
    children_q: &Query<&Children>,
    tabs: &Query<Entity, With<Tab>>,
    panes: &Query<(), With<Pane>>,
    focused_tabs: &Query<Entity, With<Focused>>,
    pane_orientations: &mut Query<&mut Orientation, With<Pane>>,
) {
    let Some(pane) = focus_node.0 else {
        return;
    };
    if !panes.contains(pane) {
        return;
    }
    let Ok(children) = children_q.get(pane) else {
        return;
    };
    let mut tab_entity = None;
    for child in children.iter() {
        if tabs.contains(child) {
            tab_entity = Some(child);
            break;
        }
    }
    let Some(tab) = tab_entity else {
        return;
    };
    let Ok(mut orientation) = pane_orientations.get_mut(pane) else {
        return;
    };
    *orientation = split_axis;

    let left = commands
        .spawn((PaneBundle::new(Orientation::Vertical, 1.0), ChildOf(pane)))
        .id();
    let right = commands
        .spawn((PaneBundle::new(Orientation::Vertical, 1.0), ChildOf(pane)))
        .id();

    commands.entity(tab).insert(ChildOf(left));

    for t in focused_tabs.iter() {
        commands.entity(t).remove::<Focused>();
    }
    commands.entity(right).with_children(|pane| {
        pane.spawn((TabBundle::default(), Focused));
    });
    focus_node.0 = Some(right);
}

fn space_root_node(padding: f32) -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(padding)),
        ..default()
    }
}

fn window_flex_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

fn pane_flex_node_for(orientation: Orientation, flex_grow: f32) -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_grow,
        min_height: Val::Px(0.0),
        flex_direction: orientation.flex_direction(),
        ..default()
    }
}

fn tab_flex_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

const CAMERA_TO_PLANE: f32 = 3.0;
const OUTLINE_Z_BACK: f32 = 0.002;

fn outline_material_for_plane(
    plane: &LayoutPlane,
    settings: &AppSettings,
    time_secs: f32,
) -> OutlineMaterial {
    let w_i = plane.inner_px.x.max(1.0e-6);
    let h_i = plane.inner_px.y.max(1.0e-6);
    let b = plane.border_px;
    let w_o = w_i + 2.0 * b;
    let h_o = h_i + 2.0 * b;
    let r_i = plane.r_px;
    let m_o = w_o.min(h_o);
    let r_o = (r_i + b).min(m_o * 0.5);
    let c = &settings.layout.pane.outline.color;
    let border_color = Color::srgb(c.r, c.g, c.b).to_linear().to_vec4();
    let g = &settings.layout.pane.outline.gradient;
    let accent = &g.accent;
    let border_accent = Color::srgb(accent.r, accent.g, accent.b)
        .to_linear()
        .to_vec4();
    let grad_on = if g.enabled { 1.0 } else { 0.0 };
    let gradient_params = Vec4::new(grad_on, g.speed, g.cycles.max(0.01), time_secs);
    let spread = settings.layout.pane.outline.glow.spread.max(0.5);
    let intensity = settings.layout.pane.outline.glow.intensity.max(0.0);
    let glow_on = if intensity > 1.0e-4 { 1.0 } else { 0.0 };
    OutlineMaterial {
        pane_inner: Vec4::new(r_i, w_i, h_i, 0.0),
        pane_outer: Vec4::new(r_o, w_o, h_o, 0.0),
        border_color,
        glow_params: Vec4::new(glow_on, intensity, spread, 0.0),
        gradient_params,
        border_accent,
        alpha_mode: AlphaMode::Blend,
    }
}

fn layout_plane_from_inner_logical(
    inner_logical: Vec2,
    settings: &AppSettings,
    full_window_logical: Vec2,
    base_half: Vec2,
) -> LayoutPlane {
    let fw = full_window_logical.x.max(1.0e-6);
    let fh = full_window_logical.y.max(1.0e-6);
    let w_i = inner_logical.x.max(1.0e-6);
    let h_i = inner_logical.y.max(1.0e-6);
    let border_px = settings.layout.pane.outline.width.max(0.0);
    let w_o = w_i + 2.0 * border_px;
    let h_o = h_i + 2.0 * border_px;
    let m = w_i.min(h_i);
    let r_px = settings.layout.pane.radius.min(m * 0.5).max(0.0);
    let inner_world_half = Vec2::new(base_half.x * w_i / fw, base_half.y * h_i / fh);
    let outer_world_half = Vec2::new(base_half.x * w_o / fw, base_half.y * h_o / fh);
    LayoutPlane {
        inner_px: Vec2::new(w_i, h_i),
        border_px,
        r_px,
        inner_world_half,
        outer_world_half,
    }
}

fn window_projection_context(
    window: &Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: &Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: &Query<&Camera>,
) -> (Vec2, Vec2) {
    let full_px = primary_window_logical_px(window, cameras);
    let fw = full_px.x.max(1.0e-6);
    let fh = full_px.y.max(1.0e-6);
    let base_half = camera_proj
        .single()
        .map(|(_, projection)| world_half_extents_fill_plane(projection, fw / fh, CAMERA_TO_PLANE))
        .unwrap_or_else(|_| Vec2::new(fw * 0.5, fh * 0.5));
    (full_px, base_half)
}

fn sync_layout_from_ui(
    settings: Res<AppSettings>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
    mut tabs: Query<(&mut LayoutPlane, &ComputedNode), With<Tab>>,
) {
    let (full_px, base_half) = window_projection_context(&window, &camera_proj, &cameras);
    for (mut lp, computed) in tabs.iter_mut() {
        let inner_logical = computed.size() * computed.inverse_scale_factor;
        let next = layout_plane_from_inner_logical(inner_logical, &settings, full_px, base_half);
        if next != *lp {
            *lp = next;
        }
    }
}

fn spawn_global_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
    settings: Res<AppSettings>,
    time: Res<Time>,
) {
    let plane = LayoutPlane::default();
    let outline_mat = outline_materials.add(outline_material_for_plane(
        &plane,
        &settings,
        time.elapsed_secs(),
    ));
    let outer_mesh = meshes.add(Plane3d::new(Vec3::Z, plane.outer_world_half));
    commands.spawn((
        NomadicOutline,
        Mesh3d(outer_mesh),
        MeshMaterial3d(outline_mat),
        Transform::from_translation(Vec3::new(0.0, 0.0, -OUTLINE_Z_BACK)),
        GlobalTransform::default(),
        Visibility::Hidden,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
    ));
}

fn sync_nomadic_outline(
    focus_node: Res<InFocusNode>,
    settings: Res<AppSettings>,
    time: Res<Time>,
    window: Query<&NativeWindow, With<PrimaryWindow>>,
    camera_proj: Query<(&Camera, &Projection), With<Camera3d>>,
    cameras: Query<&Camera>,
    panes: Query<&ComputedNode, With<Pane>>,
    mut outline_q: Query<
        (
            &mut Mesh3d,
            &MeshMaterial3d<OutlineMaterial>,
            &mut Visibility,
        ),
        With<NomadicOutline>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let Ok((mesh_3d, mat_h, mut visibility)) = outline_q.single_mut() else {
        return;
    };
    let Some(pane) = focus_node.0 else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Ok(computed) = panes.get(pane) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let inner_logical = computed.size() * computed.inverse_scale_factor;
    if inner_logical.x <= 0.0 || inner_logical.y <= 0.0 {
        *visibility = Visibility::Hidden;
        return;
    }
    let (full_px, base_half) = window_projection_context(&window, &camera_proj, &cameras);
    let plane = layout_plane_from_inner_logical(inner_logical, &settings, full_px, base_half);
    if plane.border_px <= 0.0 {
        *visibility = Visibility::Hidden;
        return;
    }
    *visibility = Visibility::Visible;
    let time_secs = time.elapsed_secs();
    if let Some(mesh) = meshes.get_mut(&mesh_3d.0) {
        *mesh = Mesh::from(Plane3d::new(Vec3::Z, plane.outer_world_half));
    }
    if let Some(m) = outline_materials.get_mut(&mat_h.0) {
        *m = outline_material_for_plane(&plane, &settings, time_secs);
    }
}

fn tick_outline_gradient_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
    outlines: Query<&MeshMaterial3d<OutlineMaterial>, With<NomadicOutline>>,
) {
    let t = time.elapsed_secs();
    for mesh_mat in &outlines {
        let id = mesh_mat.id();
        let Some(m) = materials.get(id) else {
            continue;
        };
        if m.gradient_params.x <= 0.5 {
            continue;
        };
        let Some(m) = materials.get_mut(id) else {
            continue;
        };
        m.gradient_params.w = t;
    }
}

fn primary_window_logical_px(
    window: &Query<&NativeWindow, With<PrimaryWindow>>,
    cameras: &Query<&Camera>,
) -> Vec2 {
    if let Ok(w) = window.single() {
        let width = w.width();
        let height = w.height();
        if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
            return Vec2::new(width, height);
        }
    }
    for cam in cameras.iter() {
        if let Some(size) = cam.logical_viewport_size()
            && size.x > 0.0
            && size.y > 0.0
            && size.x.is_finite()
            && size.y.is_finite()
        {
            return size;
        }
    }
    Vec2::splat(800.0)
}

fn world_half_extents_fill_plane(
    projection: &Projection,
    window_aspect: f32,
    camera_to_plane: f32,
) -> Vec2 {
    let aspect = if window_aspect.is_finite() && window_aspect > 0.0 {
        window_aspect
    } else {
        1.0
    };
    match projection {
        Projection::Perspective(p) => {
            let half_h = camera_to_plane * (p.fov * 0.5).tan();
            let half_w = half_h * aspect;
            Vec2::new(half_w, half_h)
        }
        _ => Vec2::new(camera_to_plane * 0.5 * aspect, camera_to_plane * 0.5),
    }
}
