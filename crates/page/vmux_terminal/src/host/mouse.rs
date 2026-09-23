use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};
use vmux_service::client::{ServiceClient, ServiceHandle};
use vmux_service::protocol::{ClientMessage, ProcessId};

use crate::Terminal;
use crate::event::{MOD_ALT, MOD_CTRL, MOD_SHIFT, TermMouseEvent, TermSelectionRange};

use super::plugin::{LocalCopyModeState, TerminalModeMap};

pub(super) struct MousePlugin;

impl Plugin for MousePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MouseSelectionState>()
            .add_plugins(UiEventPlugin::<(TermMouseEvent,)>::default())
            .add_observer(on_term_mouse);
    }
}

const MULTI_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const MULTI_CLICK_CELL_TOLERANCE: i32 = 1;

#[derive(Resource, Default)]
pub(super) struct MouseSelectionState {
    per_process: HashMap<ProcessId, MouseSessionState>,
}

impl MouseSelectionState {
    pub(super) fn remove(&mut self, process_id: &ProcessId) {
        self.per_process.remove(process_id);
    }
}

#[derive(Default, Clone, Debug)]
struct MouseSessionState {
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
enum MouseTerminalAction {
    ForwardInput(Vec<u8>),
    EnterCopyMode,
    ExitCopyMode,
    SetSelection(Option<TermSelectionRange>),
    ExtendSelectionTo { col: u16, row: u16 },
    SelectWordAt { col: u16, row: u16 },
    SelectLineAt { row: u16 },
}

impl MouseSessionState {
    fn apply(
        &mut self,
        event: &TermMouseEvent,
        mouse_capture: bool,
        now: Instant,
    ) -> Vec<MouseTerminalAction> {
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
            return vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
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
                    vec![MouseTerminalAction::ExtendSelectionTo {
                        col: event.col,
                        row: event.row,
                    }]
                }
                1 => {
                    self.pending_anchor = Some((event.col, event.row));
                    vec![MouseTerminalAction::SetSelection(None)]
                }
                2 => {
                    self.pending_anchor = None;
                    vec![MouseTerminalAction::SelectWordAt {
                        col: event.col,
                        row: event.row,
                    }]
                }
                _ => {
                    self.pending_anchor = None;
                    vec![MouseTerminalAction::SelectLineAt { row: event.row }]
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
                    MouseTerminalAction::EnterCopyMode,
                    MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                        start_col,
                        start_row,
                        end_col: event.col,
                        end_row: event.row,
                        is_block: false,
                    })),
                ];
            }
            return vec![MouseTerminalAction::ExtendSelectionTo {
                col: event.col,
                row: event.row,
            }];
        }

        if !event.pressed {
            let actions = if self.drag_visual_active {
                vec![MouseTerminalAction::ExitCopyMode]
            } else {
                Vec::new()
            };
            self.drag_active = false;
            self.drag_visual_active = false;
            self.last_extend_cell = None;
            self.pending_anchor = None;
            return actions;
        }

        Vec::new()
    }
}

impl MouseTerminalAction {
    fn send(self, service: &ServiceHandle, process_id: ProcessId) {
        match self {
            Self::ForwardInput(data) => {
                service.send(ClientMessage::ProcessInput { process_id, data });
            }
            Self::EnterCopyMode => {
                service.send(ClientMessage::EnterCopyMode { process_id });
            }
            Self::ExitCopyMode => {
                service.send(ClientMessage::ExitCopyMode { process_id });
            }
            Self::SetSelection(range) => {
                service.send(ClientMessage::SetSelection { process_id, range });
            }
            Self::ExtendSelectionTo { col, row } => {
                service.send(ClientMessage::ExtendSelectionTo {
                    process_id,
                    col,
                    row,
                });
            }
            Self::SelectWordAt { col, row } => {
                service.send(ClientMessage::SelectWordAt {
                    process_id,
                    col,
                    row,
                });
            }
            Self::SelectLineAt { row } => {
                service.send(ClientMessage::SelectLineAt { process_id, row });
            }
        }
    }

    fn update_copy_mode(&self, state: &mut LocalCopyModeState, process_id: ProcessId) {
        match self {
            Self::EnterCopyMode => state.set(process_id, true),
            Self::ExitCopyMode => state.set(process_id, false),
            _ => {}
        }
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
    trigger: On<BinReceive<TermMouseEvent>>,
    terminals: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
    modes: Res<TerminalModeMap>,
    mut selections: ResMut<MouseSelectionState>,
    mut copy_mode: ResMut<LocalCopyModeState>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(process_id) = terminals.get(entity).copied() else {
        return;
    };

    if event.button == 64 || event.button == 65 {
        service.0.send(ClientMessage::MouseWheel {
            process_id,
            up: event.button == 64,
            col: event.col,
            row: event.row,
            modifiers: event.modifiers,
        });
        return;
    }

    let mouse_capture = modes
        .modes
        .get(&process_id)
        .is_some_and(|mode| mode.mouse_capture);
    let selection = selections.per_process.entry(process_id).or_default();
    for action in selection.apply(event, mouse_capture, Instant::now()) {
        action.update_copy_mode(&mut copy_mode, process_id);
        action.send(&service.0, process_id);
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
        let mut state = MouseSessionState::default();
        let now = Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            state.apply(&down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let drag = mouse_event(0, 5, 3, true, true);
        assert_eq!(
            state.apply(&drag, false, now + std::time::Duration::from_millis(10),),
            vec![
                MouseTerminalAction::EnterCopyMode,
                MouseTerminalAction::SetSelection(Some(TermSelectionRange {
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
            vec![MouseTerminalAction::ExitCopyMode]
        );
    }

    #[test]
    fn single_click_never_enters_visual_mode() {
        let mut state = MouseSessionState::default();
        let now = Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            state.apply(&down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let release = mouse_event(0, 2, 3, false, false);
        assert_eq!(
            state.apply(&release, false, now + std::time::Duration::from_millis(20),),
            Vec::<MouseTerminalAction>::new()
        );
    }

    #[test]
    fn captured_mouse_without_shift_still_forwards_drag_motion() {
        let mut state = MouseSessionState::default();
        let event = mouse_event(0, 4, 5, true, true);

        assert_eq!(
            state.apply(&event, true, Instant::now()),
            vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
                32, 4, 5, 0, true,
            ))]
        );
    }

    #[test]
    fn hover_motion_without_app_capture_is_not_forwarded() {
        let mut state = MouseSessionState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            state.apply(&hover, false, Instant::now()),
            Vec::<MouseTerminalAction>::new()
        );
    }

    #[test]
    fn hover_motion_with_app_capture_is_forwarded() {
        let mut state = MouseSessionState::default();
        let hover = mouse_event(3, 9, 4, true, true);

        assert_eq!(
            state.apply(&hover, true, Instant::now()),
            vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
                35, 9, 4, 0, true,
            ))]
        );
    }
}
