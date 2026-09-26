use std::time::Instant;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_service::client::ServiceRequest;

use crate::Terminal;
use crate::event::{MOD_ALT, MOD_CTRL, MOD_SHIFT, TermMouseEvent, TermSelectionRange};

use super::state::{TerminalCopyMode, TerminalMode};

pub(super) struct MousePlugin;

impl Plugin for MousePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(TermMouseEvent,)>::default())
            .add_observer(on_term_mouse);
    }
}

const MULTI_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const MULTI_CLICK_CELL_TOLERANCE: i32 = 1;

#[derive(Component, Default, Clone, Debug)]
pub(crate) struct TerminalMouseState {
    last_click: Option<MouseClickRecord>,
    drag_active: bool,
    drag_visual_active: bool,
    last_extend_cell: Option<(u16, u16)>,
    pending_anchor: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug)]
struct MouseClickRecord {
    when: Instant,
    col: u16,
    row: u16,
    count: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MouseTerminalEffect {
    ForwardInput(Vec<u8>),
    EnterCopyMode,
    ExitCopyMode,
    SetSelection(Option<TermSelectionRange>),
    ExtendSelectionTo { col: u16, row: u16 },
    SelectWordAt { col: u16, row: u16 },
    SelectLineAt { row: u16 },
}

impl TerminalMouseState {
    fn apply(
        &mut self,
        event: &TermMouseEvent,
        mouse_capture: bool,
        now: Instant,
    ) -> Vec<MouseTerminalEffect> {
        let shift = event.modifiers & MOD_SHIFT != 0;
        let is_left = event.button == 0;
        let select_mode = is_left && (!mouse_capture || shift);

        if !select_mode {
            if !mouse_capture {
                return Vec::new();
            }
            let button = if event.moving {
                event.button + 32
            } else {
                event.button
            };
            return vec![MouseTerminalEffect::ForwardInput(sgr_mouse_sequence(
                button,
                event.col,
                event.row,
                event.modifiers,
                event.pressed,
            ))];
        }

        if event.pressed && !event.moving {
            let count = match self.last_click {
                Some(previous)
                    if now.duration_since(previous.when) <= MULTI_CLICK_WINDOW
                        && (previous.col as i32 - event.col as i32).abs()
                            <= MULTI_CLICK_CELL_TOLERANCE
                        && (previous.row as i32 - event.row as i32).abs()
                            <= MULTI_CLICK_CELL_TOLERANCE =>
                {
                    if previous.count >= 3 {
                        1
                    } else {
                        previous.count + 1
                    }
                }
                _ => 1,
            };
            self.last_click = Some(MouseClickRecord {
                when: now,
                col: event.col,
                row: event.row,
                count,
            });
            self.drag_active = count == 1;
            self.drag_visual_active = false;
            self.last_extend_cell = Some((event.col, event.row));

            return match count {
                1 if shift => {
                    self.pending_anchor = None;
                    vec![MouseTerminalEffect::ExtendSelectionTo {
                        col: event.col,
                        row: event.row,
                    }]
                }
                1 => {
                    self.pending_anchor = Some((event.col, event.row));
                    vec![MouseTerminalEffect::SetSelection(None)]
                }
                2 => {
                    self.pending_anchor = None;
                    vec![MouseTerminalEffect::SelectWordAt {
                        col: event.col,
                        row: event.row,
                    }]
                }
                _ => {
                    self.pending_anchor = None;
                    vec![MouseTerminalEffect::SelectLineAt { row: event.row }]
                }
            };
        }

        if event.moving && self.drag_active {
            if self.last_extend_cell == Some((event.col, event.row)) {
                return Vec::new();
            }
            self.last_extend_cell = Some((event.col, event.row));
            if let Some((start_col, start_row)) = self.pending_anchor.take() {
                self.drag_visual_active = true;
                return vec![
                    MouseTerminalEffect::EnterCopyMode,
                    MouseTerminalEffect::SetSelection(Some(TermSelectionRange {
                        start_col,
                        start_row,
                        end_col: event.col,
                        end_row: event.row,
                        is_block: false,
                    })),
                ];
            }
            return vec![MouseTerminalEffect::ExtendSelectionTo {
                col: event.col,
                row: event.row,
            }];
        }

        if !event.pressed {
            let effects = if self.drag_visual_active {
                vec![MouseTerminalEffect::ExitCopyMode]
            } else {
                Vec::new()
            };
            self.drag_active = false;
            self.drag_visual_active = false;
            self.last_extend_cell = None;
            self.pending_anchor = None;
            return effects;
        }

        Vec::new()
    }
}

impl MouseTerminalEffect {
    fn message(self, process_id: ProcessId) -> ClientMessage {
        match self {
            MouseTerminalEffect::ForwardInput(data) => {
                ClientMessage::ProcessInput { process_id, data }
            }
            MouseTerminalEffect::EnterCopyMode => ClientMessage::EnterCopyMode { process_id },
            MouseTerminalEffect::ExitCopyMode => ClientMessage::ExitCopyMode { process_id },
            MouseTerminalEffect::SetSelection(range) => {
                ClientMessage::SetSelection { process_id, range }
            }
            MouseTerminalEffect::ExtendSelectionTo { col, row } => {
                ClientMessage::ExtendSelectionTo {
                    process_id,
                    col,
                    row,
                }
            }
            MouseTerminalEffect::SelectWordAt { col, row } => ClientMessage::SelectWordAt {
                process_id,
                col,
                row,
            },
            MouseTerminalEffect::SelectLineAt { row } => {
                ClientMessage::SelectLineAt { process_id, row }
            }
        }
    }
}

fn update_copy_mode(effect: &MouseTerminalEffect, state: &mut TerminalCopyMode) {
    match effect {
        MouseTerminalEffect::EnterCopyMode => state.set(true),
        MouseTerminalEffect::ExitCopyMode => state.set(false),
        _ => {}
    }
}

fn sgr_mouse_sequence(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool) -> Vec<u8> {
    let mut button = button as u32;
    if modifiers & MOD_SHIFT != 0 {
        button += 4;
    }
    if modifiers & MOD_ALT != 0 {
        button += 8;
    }
    if modifiers & MOD_CTRL != 0 {
        button += 16;
    }
    let suffix = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, suffix).into_bytes()
}

fn on_term_mouse(
    trigger: On<UiInput<TermMouseEvent>>,
    mut terminals: Query<
        (
            &ProcessId,
            &TerminalMode,
            &mut TerminalMouseState,
            &mut TerminalCopyMode,
        ),
        With<Terminal>,
    >,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Ok((process_id, mode, mut selection, mut copy_mode)) = terminals.get_mut(entity) else {
        return;
    };
    let process_id = *process_id;

    if event.button == 64 || event.button == 65 {
        service_requests.write(ServiceRequest(ClientMessage::MouseWheel {
            process_id,
            up: event.button == 64,
            col: event.col,
            row: event.row,
            modifiers: event.modifiers,
        }));
        return;
    }

    for effect in selection.apply(event, mode.mouse_capture, Instant::now()) {
        update_copy_mode(&effect, &mut copy_mode);
        service_requests.write(ServiceRequest(effect.message(process_id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_event(button: u8, col: u16, row: u16, pressed: bool, moving: bool) -> TermMouseEvent {
        TermMouseEvent {
            button,
            col,
            row,
            modifiers: 0,
            pressed,
            moving,
        }
    }

    #[test]
    fn drag_enters_visual_mode_on_first_motion_and_exits_on_release() {
        let mut state = TerminalMouseState::default();
        let now = Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            state.apply(&down, false, now),
            vec![MouseTerminalEffect::SetSelection(None)]
        );

        let drag = mouse_event(0, 5, 3, true, true);
        assert_eq!(
            state.apply(&drag, false, now + std::time::Duration::from_millis(10),),
            vec![
                MouseTerminalEffect::EnterCopyMode,
                MouseTerminalEffect::SetSelection(Some(TermSelectionRange {
                    start_col: 2,
                    start_row: 3,
                    end_col: 5,
                    end_row: 3,
                    is_block: false,
                })),
            ]
        );

        let release = mouse_event(0, 5, 3, false, false);
        assert_eq!(
            state.apply(&release, false, now + std::time::Duration::from_millis(20),),
            vec![MouseTerminalEffect::ExitCopyMode]
        );
    }

    #[test]
    fn single_click_never_enters_visual_mode() {
        let mut state = TerminalMouseState::default();
        let now = Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            state.apply(&down, false, now),
            vec![MouseTerminalEffect::SetSelection(None)]
        );

        let release = mouse_event(0, 2, 3, false, false);
        assert_eq!(
            state.apply(&release, false, now + std::time::Duration::from_millis(20),),
            Vec::<MouseTerminalEffect>::new()
        );
    }

    #[test]
    fn captured_mouse_without_shift_still_forwards_drag_motion() {
        let mut state = TerminalMouseState::default();
        let event = mouse_event(0, 4, 5, true, true);

        assert_eq!(
            state.apply(&event, true, Instant::now()),
            vec![MouseTerminalEffect::ForwardInput(sgr_mouse_sequence(
                32, 4, 5, 0, true,
            ))]
        );
    }

    #[test]
    fn hover_motion_without_app_capture_is_not_forwarded() {
        let mut state = TerminalMouseState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            state.apply(&hover, false, Instant::now()),
            Vec::<MouseTerminalEffect>::new()
        );
    }

    #[test]
    fn hover_motion_with_app_capture_is_forwarded() {
        let mut state = TerminalMouseState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            state.apply(&hover, true, Instant::now()),
            vec![MouseTerminalEffect::ForwardInput(sgr_mouse_sequence(
                35, 9, 4, 0, true,
            ))]
        );
    }
}
