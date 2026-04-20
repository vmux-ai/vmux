use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::Main,
    settings::AppSettings,
};
use bevy::{
    prelude::*,
    ui::UiSystems,
    window::PrimaryWindow,
};
use vmux_header::Header;

pub(crate) struct SideSheetPlugin;

impl Plugin for SideSheetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SideSheetOpen(true))
            .insert_resource(SideSheetWidth(0.0)) // set from settings on first sync
            .add_systems(Update, handle_side_sheet_toggle.in_set(ReadAppCommands))
            .add_systems(Update, side_sheet_drag_resize)
            .add_systems(
                PostUpdate,
                sync_side_sheet_visibility.before(UiSystems::Layout),
            );
    }
}

#[derive(Component)]
pub(crate) struct SideSheet;

#[derive(Component, PartialEq, Eq)]
pub(crate) enum SideSheetPosition {
    Left,
    Right,
    Bottom,
}

#[derive(Resource)]
pub(crate) struct SideSheetOpen(pub bool);

/// Current width of the left side sheet (mutable during drag).
#[derive(Resource)]
struct SideSheetWidth(f32);

/// Marker component for an active drag on the side sheet edge.
#[derive(Component)]
struct SideSheetDrag {
    start_cursor_x: f32,
    start_width: f32,
}

const MIN_SIDE_SHEET_WIDTH: f32 = 120.0;
const MAX_SIDE_SHEET_WIDTH: f32 = 800.0;
const EDGE_HIT_ZONE: f32 = 6.0;

fn handle_side_sheet_toggle(
    mut reader: MessageReader<AppCommand>,
    mut open: ResMut<SideSheetOpen>,
) {
    for cmd in reader.read() {
        match cmd {
            AppCommand::SideSheet(SideSheetCommand::Toggle) => {
                open.0 = !open.0;
            }
            AppCommand::SideSheet(SideSheetCommand::ToggleRight) => {}
            AppCommand::SideSheet(SideSheetCommand::ToggleBottom) => {}
            _ => {}
        }
    }
}

fn side_sheet_drag_resize(
    windows: Query<&Window, With<PrimaryWindow>>,

    open: Res<SideSheetOpen>,
    mut width_res: ResMut<SideSheetWidth>,
    sheet_q: Query<(&SideSheetPosition, &ComputedNode, &UiGlobalTransform), With<SideSheet>>,
    active_drags: Query<(Entity, &SideSheetDrag)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut side_sheet_q: Query<(&SideSheetPosition, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    if !open.0 {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.physical_cursor_position() else { return };
    let cursor_x = cursor_pos.x as f32;

    // Handle active drag
    if let Ok((drag_entity, drag)) = active_drags.single() {
        if mouse.pressed(MouseButton::Left) {
            let new_width = (drag.start_width + cursor_x - drag.start_cursor_x)
                .clamp(MIN_SIDE_SHEET_WIDTH, MAX_SIDE_SHEET_WIDTH);
            width_res.0 = new_width;
            let sheet_total = new_width + settings.layout.pane.gap;

            for (pos, mut node) in &mut side_sheet_q {
                if *pos == SideSheetPosition::Left {
                    node.width = Val::Px(new_width);
                }
            }
            for mut node in &mut header_q {
                node.margin.left = Val::Px(sheet_total);
            }
            for mut node in &mut main_q {
                node.margin.left = Val::Px(sheet_total);
            }
        } else {
            commands.entity(drag_entity).despawn();
            return;
        }

        return;
    }

    // Hover detection on right edge of left side sheet
    for (pos, cn, gt) in &sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        let center = gt.transform_point2(Vec2::ZERO);
        let right_edge = center.x + cn.size.x * 0.5;
        let top = center.y - cn.size.y * 0.5;
        let bottom = center.y + cn.size.y * 0.5;
        let cursor_y = cursor_pos.y as f32;

        if cursor_x >= right_edge - EDGE_HIT_ZONE
            && cursor_x <= right_edge + EDGE_HIT_ZONE
            && cursor_y >= top
            && cursor_y <= bottom
        {
            if mouse.just_pressed(MouseButton::Left) {
                commands.spawn(SideSheetDrag {
                    start_cursor_x: cursor_x,
                    start_width: width_res.0,
                });
            }
        }
    }


}

fn sync_side_sheet_visibility(
    open: Res<SideSheetOpen>,
    settings: Res<AppSettings>,
    mut width_res: ResMut<SideSheetWidth>,
    mut side_sheet_q: Query<(&SideSheetPosition, &mut Visibility, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
) {
    if !open.is_changed() {
        return;
    }

    // Initialize width from settings if not yet set
    if width_res.0 <= 0.0 {
        width_res.0 = settings.layout.side_sheet.width;
    }

    let width = width_res.0;
    let sheet_total = width + settings.layout.pane.gap;
    for (pos, mut vis, mut node) in &mut side_sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        if open.0 {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.width = Val::Px(width);
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    for mut node in &mut main_q {
        node.margin.left = if open.0 {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}
