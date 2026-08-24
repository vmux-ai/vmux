use vmux_wire::command_bar::OpenId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBarSize {
    pub width: u32,
    pub height: u32,
    pub shell_left: i32,
    pub shell_top: i32,
    pub shell_width: u32,
    pub shell_height: u32,
}

#[derive(Debug, Default)]
pub struct CommandBarSizeEmissionState {
    last_emitted: Option<(OpenId, CommandBarSize)>,
    scheduled: bool,
}

impl CommandBarSizeEmissionState {
    pub fn should_emit(&self, open_id: OpenId, size: CommandBarSize) -> bool {
        open_id.is_open() && self.last_emitted != Some((open_id, size))
    }

    pub fn mark_emitted(&mut self, open_id: OpenId, size: CommandBarSize) {
        self.last_emitted = Some((open_id, size));
    }

    pub fn schedule(&mut self) -> bool {
        if self.scheduled {
            false
        } else {
            self.scheduled = true;
            true
        }
    }

    pub fn finish_schedule(&mut self) {
        self.scheduled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_size_is_suppressed_until_the_next_open() {
        let mut state = CommandBarSizeEmissionState::default();

        let shown = CommandBarSize {
            width: 576,
            height: 320,
            shell_left: 100,
            shell_top: 80,
            shell_width: 576,
            shell_height: 320,
        };

        assert!(state.should_emit(OpenId(1), shown));
        state.mark_emitted(OpenId(1), shown);
        assert!(!state.should_emit(OpenId(1), shown));
        assert!(state.should_emit(
            OpenId(1),
            CommandBarSize {
                height: 400,
                shell_height: 400,
                ..shown
            }
        ));
        assert!(state.should_emit(
            OpenId(1),
            CommandBarSize {
                shell_left: 110,
                ..shown
            }
        ));
        assert!(state.should_emit(OpenId(2), shown));
    }

    #[test]
    fn animation_frame_requests_coalesce() {
        let mut state = CommandBarSizeEmissionState::default();

        assert!(state.schedule());
        assert!(!state.schedule());
        state.finish_schedule();
        assert!(state.schedule());
    }
}
