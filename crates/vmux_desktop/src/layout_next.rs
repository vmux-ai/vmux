use bevy::prelude::*;
use bevy::ui::{Display, UiSystems};
use bevy::window::{PrimaryWindow, Window as NativeWindow};

use crate::scene::Spawn3dCamera;
use crate::settings::{AppSettings, LoadAppSettings};
use vmux_history::{CreatedAt, LastActivatedAt};

pub struct LayoutNextPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabLayoutSync;

impl Plugin for LayoutNextPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(PostUpdate, TabLayoutSync.after(UiSystems::Layout))
            .add_systems(
                Startup,
                spawn_tab_shell
                    .after(LoadAppSettings)
                    .after(Spawn3dCamera),
            )
            .add_systems(
                PostUpdate,
                sync_layout_from_ui.in_set(TabLayoutSync),
            )
            .add_systems(Update, sync_tab_activation_visuals);
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

#[derive(Component)]
pub(crate) struct Focused;

#[derive(Component, Clone, Copy)]
struct TabStripButton(Entity);

const CAMERA_TO_PLANE: f32 = 3.0;

const COLOR_TAB_ACTIVE: Color = Color::srgb(0.35, 0.45, 0.65);
const COLOR_TAB_INACTIVE: Color = Color::srgb(0.22, 0.22, 0.26);
const COLOR_CONTENT_A: Color = Color::srgb(0.12, 0.14, 0.18);
const COLOR_CONTENT_B: Color = Color::srgb(0.14, 0.12, 0.18);
const COLOR_CONTENT_C: Color = Color::srgb(0.12, 0.16, 0.14);

fn root_column_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::ColumnReverse,
        ..default()
    }
}

fn tab_bar_row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Px(40.0),
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(4.0),
        padding: UiRect::horizontal(Val::Px(6.0)),
        align_items: AlignItems::Center,
        ..default()
    }
}

fn content_stack_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_grow: 1.0,
        min_height: Val::Px(0.0),
        position_type: PositionType::Relative,
        flex_direction: FlexDirection::Column,
        ..default()
    }
}

fn tab_panel_node_active() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        display: Display::Flex,
        ..default()
    }
}

fn tab_panel_node_inactive() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        display: Display::None,
        ..default()
    }
}

fn spawn_tab_shell(mut commands: Commands, camera_q: Query<Entity, With<Camera3d>>) {
    let Ok(camera) = camera_q.single() else {
        return;
    };

    let mut tab_entities: Vec<Entity> = Vec::new();

    commands
        .spawn((
            root_column_node(),
            UiTargetCamera(camera),
            Name::new("LayoutNextRoot"),
        ))
        .with_children(|root| {
            root.spawn((content_stack_node(), Name::new("TabContentStack")))
                .with_children(|stack| {
                    let colors = [COLOR_CONTENT_A, COLOR_CONTENT_B, COLOR_CONTENT_C];
                    let names = ["Tab 1", "Tab 2", "Tab 3"];
                    for i in 0..3 {
                        let mut ec = stack.spawn((
                            Tab,
                            LayoutPlane::default(),
                            BackgroundColor(colors[i]),
                            Name::new(names[i]),
                            CreatedAt::default(),
                            LastActivatedAt::default(),
                        ));
                        if i == 0 {
                            ec.insert((tab_panel_node_active(), Focused));
                        } else {
                            ec.insert(tab_panel_node_inactive());
                        }
                        tab_entities.push(ec.id());
                    }
                });

            root.spawn((tab_bar_row_node(), Name::new("TabBar")))
                .with_children(|bar| {
                    let labels = ["Tab 1", "Tab 2", "Tab 3"];
                    for (i, label) in labels.iter().enumerate() {
                        let tab_entity = tab_entities[i];
                        bar.spawn((
                            Button,
                            TabStripButton(tab_entity),
                            Node {
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(if i == 0 {
                                COLOR_TAB_ACTIVE
                            } else {
                                COLOR_TAB_INACTIVE
                            }),
                            Name::new("TabBarButton"),
                        ))
                        .with_children(|b| {
                            b.spawn((Text::new(*label), TextColor(Color::WHITE)));
                        })
                        .observe(on_tab_strip_click);
                    }
                });
        });
}

fn on_tab_strip_click(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    buttons: Query<&TabStripButton>,
    focused: Query<Entity, With<Focused>>,
) {
    let Ok(&TabStripButton(target)) = buttons.get(click.entity) else {
        return;
    };
    for e in &focused {
        commands.entity(e).remove::<Focused>();
    }
    commands.entity(target).insert(Focused);
}

fn sync_tab_activation_visuals(
    mut tab_nodes: Query<(Entity, &Tab, Option<&Focused>, &mut Node, &mut Visibility)>,
    mut buttons: Query<(&TabStripButton, &mut BackgroundColor)>,
) {
    let Some(focused_tab) = tab_nodes
        .iter()
        .find(|(_, _, f, _, _)| f.is_some())
        .map(|(e, _, _, _, _)| e)
    else {
        return;
    };

    for (entity, _, _, mut node, mut vis) in &mut tab_nodes {
        let active = entity == focused_tab;
        node.display = if active {
            Display::Flex
        } else {
            Display::None
        };
        *vis = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for (TabStripButton(target), mut bg) in &mut buttons {
        let sel = *target == focused_tab;
        *bg = BackgroundColor(if sel {
            COLOR_TAB_ACTIVE
        } else {
            COLOR_TAB_INACTIVE
        });
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
