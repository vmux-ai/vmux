use super::{Open, SideSheetState};
use crate::{
    command::{AppCommand, ReadAppCommands, SideSheetCommand},
    layout::window::Main,
    settings::AppSettings,
};
use bevy::{prelude::*, ui::UiSystems, window::PrimaryWindow, winit::WinitWindows};
use vmux_header::Header;

pub(crate) struct SideSheetLayoutPlugin;

impl Plugin for SideSheetLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SideSheetWidth(0.0)) // set from settings on first sync
            .add_systems(Update, handle_side_sheet_toggle.in_set(ReadAppCommands))
            .add_systems(Update, side_sheet_drag_resize)
            .add_systems(
                PostUpdate,
                (
                    sync_side_sheet_visibility.before(UiSystems::Layout),
                    sync_window_buttons_visibility,
                ),
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
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>), With<SideSheet>>,
    state_q: Query<(Entity, Has<Open>), With<SideSheetState>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        match cmd {
            AppCommand::SideSheet(SideSheetCommand::Toggle) => {
                for (entity, pos, is_open) in &side_sheet_q {
                    if *pos == SideSheetPosition::Left {
                        if is_open {
                            commands.entity(entity).remove::<Open>();
                        } else {
                            commands.entity(entity).insert(Open);
                        }
                    }
                }
                for (entity, is_open) in &state_q {
                    if is_open {
                        commands.entity(entity).remove::<Open>();
                    } else {
                        commands.entity(entity).insert(Open);
                    }
                }
            }
            AppCommand::SideSheet(SideSheetCommand::ToggleRight) => {}
            AppCommand::SideSheet(SideSheetCommand::ToggleBottom) => {}
            _ => {}
        }
    }
}

fn side_sheet_drag_resize(
    windows: Query<&Window, With<PrimaryWindow>>,

    mut width_res: ResMut<SideSheetWidth>,
    sheet_q: Query<
        (
            &SideSheetPosition,
            Has<Open>,
            &ComputedNode,
            &UiGlobalTransform,
        ),
        With<SideSheet>,
    >,
    active_drags: Query<(Entity, &SideSheetDrag)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut side_sheet_q: Query<(&SideSheetPosition, &mut Node), With<SideSheet>>,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    let is_open = sheet_q
        .iter()
        .any(|(pos, open, _, _)| *pos == SideSheetPosition::Left && open);
    if !is_open {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor_pos) = window.physical_cursor_position() else {
        return;
    };
    let cursor_x = cursor_pos.x;

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
    for (pos, _, cn, gt) in &sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        let center = gt.transform_point2(Vec2::ZERO);
        let right_edge = center.x + cn.size.x * 0.5;
        let top = center.y - cn.size.y * 0.5;
        let bottom = center.y + cn.size.y * 0.5;
        let cursor_y = cursor_pos.y;

        if cursor_x >= right_edge - EDGE_HIT_ZONE
            && cursor_x <= right_edge + EDGE_HIT_ZONE
            && cursor_y >= top
            && cursor_y <= bottom
            && mouse.just_pressed(MouseButton::Left)
        {
            commands.spawn(SideSheetDrag {
                start_cursor_x: cursor_x,
                start_width: width_res.0,
            });
        }
    }
}

fn sync_side_sheet_visibility(
    settings: Res<AppSettings>,
    mut width_res: ResMut<SideSheetWidth>,
    mut side_sheet_q: Query<
        (Entity, &SideSheetPosition, &mut Visibility, &mut Node),
        With<SideSheet>,
    >,
    mut header_q: Query<&mut Node, (With<Header>, Without<SideSheet>, Without<Main>)>,
    mut main_q: Query<&mut Node, (With<Main>, Without<SideSheet>, Without<Header>)>,
    added: Query<Entity, (With<SideSheet>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    // Determine if the left side sheet opened or closed
    let mut left_open: Option<bool> = None;
    for entity in &added {
        if let Ok((_, pos, _, _)) = side_sheet_q.get(entity)
            && *pos == SideSheetPosition::Left
        {
            left_open = Some(true);
        }
    }
    for entity in removed.read() {
        if let Ok((_, pos, _, _)) = side_sheet_q.get(entity)
            && *pos == SideSheetPosition::Left
        {
            left_open = Some(false);
        }
    }

    let Some(is_open) = left_open else { return };

    // Initialize width from settings if not yet set
    if width_res.0 <= 0.0 {
        width_res.0 = settings.layout.side_sheet.width;
    }

    let width = width_res.0;
    let sheet_total = width + settings.layout.pane.gap;
    for (_, pos, mut vis, mut node) in &mut side_sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        if is_open {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.width = Val::Px(width);
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
    for mut node in &mut header_q {
        node.margin.left = if is_open {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
    for mut node in &mut main_q {
        node.margin.left = if is_open {
            Val::Px(sheet_total)
        } else {
            Val::Px(0.0)
        };
    }
}

/// Show/hide macOS traffic-light buttons to match the side-sheet state.
#[cfg(target_os = "macos")]
fn sync_window_buttons_visibility(
    side_sheet_q: Query<(&SideSheetPosition, Has<Open>), With<SideSheet>>,
    added: Query<Entity, (With<SideSheet>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
    winit_windows: Option<NonSend<WinitWindows>>,
    window_q: Query<Entity, With<PrimaryWindow>>,
) {
    // Only react to left side sheet changes
    let mut changed = false;
    let mut is_open = false;
    for entity in &added {
        if let Ok((pos, _)) = side_sheet_q.get(entity)
            && *pos == SideSheetPosition::Left
        {
            changed = true;
            is_open = true;
        }
    }
    if !changed {
        for entity in removed.read() {
            if let Ok((pos, _)) = side_sheet_q.get(entity)
                && *pos == SideSheetPosition::Left
            {
                changed = true;
            }
        }
    }

    if !changed {
        return;
    }
    let Some(winit_windows) = winit_windows else {
        return;
    };
    let Ok(entity) = window_q.single() else {
        return;
    };
    let Some(winit_win) = winit_windows.get_window(entity) else {
        return;
    };

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = winit_win.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };

    // ns_view -> [view window] -> standardWindowButton: for each button type
    let ns_view = appkit.ns_view.as_ptr();
    unsafe {
        use objc_ffi::{objc_msgSend, sel};
        let ns_window = objc_msgSend(ns_view, sel("window"));
        if ns_window.is_null() {
            return;
        }
        let hidden: libc::c_int = if is_open { 0 } else { 1 };
        // NSWindowButton values: Close=0, Miniaturize=1, Zoom=2
        for button_type in 0u64..=2 {
            let button = objc_msgSend(ns_window, sel("standardWindowButton:"), button_type);
            if !button.is_null() {
                objc_msgSend(button, sel("setHidden:"), hidden);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn sync_window_buttons_visibility() {}

// -- minimal objc runtime helpers (avoids adding objc2 as a direct dep) ------

#[cfg(target_os = "macos")]
mod objc_ffi {
    unsafe extern "C" {
        pub fn objc_msgSend(
            obj: *mut libc::c_void,
            sel: *const libc::c_void,
            ...
        ) -> *mut libc::c_void;
        pub fn sel_registerName(name: *const libc::c_char) -> *const libc::c_void;
    }

    pub fn sel(name: &str) -> *const libc::c_void {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { sel_registerName(c.as_ptr()) }
    }
}
