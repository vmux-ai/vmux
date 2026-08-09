#[derive(Debug, Default)]
pub struct CommandBarSizeEmissionState {
    last_emitted: Option<(u64, u32, u32, i32, i32, u32, u32)>,
    scheduled: bool,
}

impl CommandBarSizeEmissionState {
    pub fn should_emit(
        &self,
        open_id: u64,
        width: u32,
        height: u32,
        shell_left: i32,
        shell_top: i32,
        shell_width: u32,
        shell_height: u32,
    ) -> bool {
        open_id != 0
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
        open_id: u64,
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
#[path = "size.test.rs"]
mod tests;
