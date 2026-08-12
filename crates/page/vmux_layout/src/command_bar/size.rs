use vmux_wire::command_bar::OpenId;

#[derive(Debug, Default)]
pub struct CommandBarSizeEmissionState {
    last_emitted: Option<(OpenId, u32, u32, i32, i32, u32, u32)>,
    scheduled: bool,
}

impl CommandBarSizeEmissionState {
    pub fn should_emit(
        &self,
        open_id: OpenId,
        width: u32,
        height: u32,
        shell_left: i32,
        shell_top: i32,
        shell_width: u32,
        shell_height: u32,
    ) -> bool {
        open_id.is_open()
            && self.last_emitted
                != Some((
                    open_id,
                    width,
                    height,
                    shell_left,
                    shell_top,
                    shell_width,
                    shell_height,
                ))
    }

    pub fn mark_emitted(
        &mut self,
        open_id: OpenId,
        width: u32,
        height: u32,
        shell_left: i32,
        shell_top: i32,
        shell_width: u32,
        shell_height: u32,
    ) {
        self.last_emitted = Some((
            open_id,
            width,
            height,
            shell_left,
            shell_top,
            shell_width,
            shell_height,
        ));
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

        assert!(state.should_emit(OpenId(1), 576, 320, 100, 80, 576, 320));
        state.mark_emitted(OpenId(1), 576, 320, 100, 80, 576, 320);
        assert!(!state.should_emit(OpenId(1), 576, 320, 100, 80, 576, 320));
        assert!(state.should_emit(OpenId(1), 576, 400, 100, 80, 576, 400));
        assert!(state.should_emit(OpenId(1), 576, 320, 110, 80, 576, 320));
        assert!(state.should_emit(OpenId(2), 576, 320, 100, 80, 576, 320));
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
