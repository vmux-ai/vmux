use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_ecs::service::ServiceRequest;

use crate::Terminal;
use crate::event::{MOD_ALT, MOD_CTRL, MOD_SHIFT, TermMouseEvent, TermSelectionRange};

use super::state::{TerminalCopyMode, TerminalMode};

pub(super) struct MousePlugin;

impl Plugin for MousePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(TermMouseEvent,)>::default())
            .add_observer(term);
    }
}

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

impl TerminalMouseState {
    const MULTI_CLICK_WINDOW: Duration = Duration::from_millis(300);
    const MULTI_CLICK_CELL_TOLERANCE: i32 = 1;

    fn apply(
        &mut self,
        process_id: ProcessId,
        event: &TermMouseEvent,
        mouse_capture: bool,
        now: Instant,
        copy_mode: &mut TerminalCopyMode,
    ) -> Vec<ClientMessage> {
        let shift = event.modifiers & MOD_SHIFT != 0;
        let select_mode = event.button == 0 && (!mouse_capture || shift);
        if !select_mode {
            return self.forward_captured(process_id, event, mouse_capture);
        }
        if event.pressed && !event.moving {
            return vec![self.handle_press(process_id, event, shift, now)];
        }
        if event.moving && self.drag_active {
            return self.handle_drag(process_id, event, copy_mode);
        }
        if !event.pressed {
            return self.handle_release(process_id, copy_mode);
        }
        Vec::new()
    }

    fn forward_captured(
        &self,
        process_id: ProcessId,
        event: &TermMouseEvent,
        mouse_capture: bool,
    ) -> Vec<ClientMessage> {
        if !mouse_capture {
            return Vec::new();
        }
        let button = if event.moving {
            event.button + 32
        } else {
            event.button
        };
        vec![ClientMessage::ProcessInput {
            process_id,
            data: Self::sgr_mouse_sequence(
                button,
                event.col,
                event.row,
                event.modifiers,
                event.pressed,
            ),
        }]
    }

    fn handle_press(
        &mut self,
        process_id: ProcessId,
        event: &TermMouseEvent,
        shift: bool,
        now: Instant,
    ) -> ClientMessage {
        let count = match self.last_click {
            Some(previous)
                if now.duration_since(previous.when) <= Self::MULTI_CLICK_WINDOW
                    && (previous.col as i32 - event.col as i32).abs()
                        <= Self::MULTI_CLICK_CELL_TOLERANCE
                    && (previous.row as i32 - event.row as i32).abs()
                        <= Self::MULTI_CLICK_CELL_TOLERANCE =>
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

        match count {
            1 if shift => {
                self.pending_anchor = None;
                ClientMessage::ExtendSelectionTo {
                    process_id,
                    col: event.col,
                    row: event.row,
                }
            }
            1 => {
                self.pending_anchor = Some((event.col, event.row));
                ClientMessage::SetSelection {
                    process_id,
                    range: None,
                }
            }
            2 => {
                self.pending_anchor = None;
                ClientMessage::SelectWordAt {
                    process_id,
                    col: event.col,
                    row: event.row,
                }
            }
            _ => {
                self.pending_anchor = None;
                ClientMessage::SelectLineAt {
                    process_id,
                    row: event.row,
                }
            }
        }
    }

    fn handle_drag(
        &mut self,
        process_id: ProcessId,
        event: &TermMouseEvent,
        copy_mode: &mut TerminalCopyMode,
    ) -> Vec<ClientMessage> {
        if self.last_extend_cell == Some((event.col, event.row)) {
            return Vec::new();
        }
        self.last_extend_cell = Some((event.col, event.row));
        if let Some((start_col, start_row)) = self.pending_anchor.take() {
            self.drag_visual_active = true;
            copy_mode.set(true);
            return vec![
                ClientMessage::EnterCopyMode { process_id },
                ClientMessage::SetSelection {
                    process_id,
                    range: Some(TermSelectionRange {
                        start_col,
                        start_row,
                        end_col: event.col,
                        end_row: event.row,
                        is_block: false,
                    }),
                },
            ];
        }
        vec![ClientMessage::ExtendSelectionTo {
            process_id,
            col: event.col,
            row: event.row,
        }]
    }

    fn handle_release(
        &mut self,
        process_id: ProcessId,
        copy_mode: &mut TerminalCopyMode,
    ) -> Vec<ClientMessage> {
        let messages = if self.drag_visual_active {
            copy_mode.set(false);
            vec![ClientMessage::ExitCopyMode { process_id }]
        } else {
            Vec::new()
        };
        self.drag_active = false;
        self.drag_visual_active = false;
        self.last_extend_cell = None;
        self.pending_anchor = None;
        messages
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
}

fn term(
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

    for message in selection.apply(
        process_id,
        event,
        mode.mouse_capture,
        Instant::now(),
        &mut copy_mode,
    ) {
        service_requests.write(ServiceRequest(message));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl TerminalMouseState {
        fn event(button: u8, col: u16, row: u16, pressed: bool, moving: bool) -> TermMouseEvent {
            TermMouseEvent {
                button,
                col,
                row,
                modifiers: 0,
                pressed,
                moving,
            }
        }
    }

    #[test]
    fn drag_enters_visual_mode_on_first_motion_and_exits_on_release() {
        let mut state = TerminalMouseState::default();
        let mut copy_mode = TerminalCopyMode::default();
        let process_id = ProcessId([1; 16]);
        let now = Instant::now();

        let down = TerminalMouseState::event(0, 2, 3, true, false);
        let down_messages = state.apply(process_id, &down, false, now, &mut copy_mode);
        assert!(matches!(
            down_messages.as_slice(),
            [ClientMessage::SetSelection {
                process_id: pid,
                range: None,
            }] if *pid == process_id
        ));
        assert!(!copy_mode.active);

        let drag = TerminalMouseState::event(0, 5, 3, true, true);
        let drag_messages = state.apply(
            process_id,
            &drag,
            false,
            now + Duration::from_millis(10),
            &mut copy_mode,
        );
        assert!(matches!(
            drag_messages.as_slice(),
            [
                ClientMessage::EnterCopyMode { process_id: pid1 },
                ClientMessage::SetSelection {
                    process_id: pid2,
                    range: Some(TermSelectionRange {
                        start_col: 2,
                        start_row: 3,
                        end_col: 5,
                        end_row: 3,
                        is_block: false,
                    }),
                },
            ] if *pid1 == process_id && *pid2 == process_id
        ));
        assert!(copy_mode.active);

        let release = TerminalMouseState::event(0, 5, 3, false, false);
        let release_messages = state.apply(
            process_id,
            &release,
            false,
            now + Duration::from_millis(20),
            &mut copy_mode,
        );
        assert!(matches!(
            release_messages.as_slice(),
            [ClientMessage::ExitCopyMode { process_id: pid }] if *pid == process_id
        ));
        assert!(!copy_mode.active);
    }

    #[test]
    fn single_click_never_enters_visual_mode() {
        let mut state = TerminalMouseState::default();
        let mut copy_mode = TerminalCopyMode::default();
        let process_id = ProcessId([2; 16]);
        let now = Instant::now();

        let down = TerminalMouseState::event(0, 2, 3, true, false);
        let down_messages = state.apply(process_id, &down, false, now, &mut copy_mode);
        assert!(matches!(
            down_messages.as_slice(),
            [ClientMessage::SetSelection {
                process_id: pid,
                range: None,
            }] if *pid == process_id
        ));

        let release = TerminalMouseState::event(0, 2, 3, false, false);
        let release_messages = state.apply(
            process_id,
            &release,
            false,
            now + Duration::from_millis(20),
            &mut copy_mode,
        );
        assert!(release_messages.is_empty());
        assert!(!copy_mode.active);
    }

    #[test]
    fn captured_mouse_without_shift_still_forwards_drag_motion() {
        let mut state = TerminalMouseState::default();
        let mut copy_mode = TerminalCopyMode::default();
        let process_id = ProcessId([3; 16]);
        let event = TerminalMouseState::event(0, 4, 5, true, true);
        let expected = TerminalMouseState::sgr_mouse_sequence(32, 4, 5, 0, true);
        let messages = state.apply(process_id, &event, true, Instant::now(), &mut copy_mode);
        assert!(matches!(
            messages.as_slice(),
            [ClientMessage::ProcessInput {
                process_id: pid,
                data,
            }] if *pid == process_id && *data == expected
        ));
    }

    #[test]
    fn hover_motion_without_app_capture_is_not_forwarded() {
        let mut state = TerminalMouseState::default();
        let mut copy_mode = TerminalCopyMode::default();
        let process_id = ProcessId([4; 16]);
        let hover = TerminalMouseState::event(3, 9, 4, true, true);
        assert!(
            state
                .apply(process_id, &hover, false, Instant::now(), &mut copy_mode)
                .is_empty()
        );
    }

    #[test]
    fn hover_motion_with_app_capture_is_forwarded() {
        let mut state = TerminalMouseState::default();
        let mut copy_mode = TerminalCopyMode::default();
        let process_id = ProcessId([5; 16]);
        let hover = TerminalMouseState::event(3, 9, 4, true, true);
        let expected = TerminalMouseState::sgr_mouse_sequence(35, 9, 4, 0, true);
        let messages = state.apply(process_id, &hover, true, Instant::now(), &mut copy_mode);
        assert!(matches!(
            messages.as_slice(),
            [ClientMessage::ProcessInput {
                process_id: pid,
                data,
            }] if *pid == process_id && *data == expected
        ));
    }
}
