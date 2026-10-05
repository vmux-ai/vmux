use std::time::Instant;

#[cfg(test)]
use vmux_api::command_bar::CommandBarOpenEvent;
use vmux_api::command_bar::OpenId;

use super::controller::{
    COMMAND_BAR_NATIVE_REVEAL_TIMEOUT, COMMAND_BAR_REVEAL_FALLBACK_FRAMES,
    COMMAND_BAR_REVEAL_FRAMES, PendingCommandBarReveal,
};

impl PendingCommandBarReveal {
    pub fn is_active(&self) -> bool {
        self.open_id.is_open()
    }

    pub(super) fn next_frame(
        &self,
        rendered_open_id: Option<OpenId>,
        native_windowed: bool,
        native_overlay: bool,
        has_native_size: bool,
    ) -> Option<u8> {
        if (native_windowed || native_overlay)
            && self.open_id.is_open()
            && (rendered_open_id != Some(self.open_id) || (native_windowed && !has_native_size))
        {
            return Some(self.frames.saturating_add(1));
        }
        if !self.open_id.is_open() {
            return Some(self.frames);
        }
        if rendered_open_id != Some(self.open_id) {
            if self.frames >= COMMAND_BAR_REVEAL_FALLBACK_FRAMES {
                return None;
            }
            return Some(self.frames + 1);
        }
        if self.frames >= COMMAND_BAR_REVEAL_FRAMES {
            None
        } else {
            Some(self.frames + 1)
        }
    }

    pub(super) fn timed_out(
        &self,
        now: Instant,
        rendered_open_id: Option<OpenId>,
        native_windowed: bool,
        native_overlay: bool,
        has_native_size: bool,
    ) -> bool {
        let elapsed = self
            .started_at
            .map(|started_at| now.duration_since(started_at))
            .unwrap_or_default();
        (native_windowed || native_overlay)
            && self.open_id.is_open()
            && elapsed >= COMMAND_BAR_NATIVE_REVEAL_TIMEOUT
            && (rendered_open_id != Some(self.open_id) || (native_windowed && !has_native_size))
    }

    pub(super) fn should_retry(&self, rendered_open_id: Option<OpenId>) -> bool {
        self.open_id.is_open() && self.payload.is_some() && rendered_open_id != Some(self.open_id)
    }

    pub(super) fn accepts_size(&self) -> bool {
        self.open_id.is_open() && self.payload.is_some()
    }

    #[cfg(test)]
    pub(super) fn waiting(frames: u8, open_id: OpenId) -> Self {
        Self {
            frames,
            open_id,
            payload: Some(CommandBarOpenEvent {
                open_id,
                ..Default::default()
            }),
            started_at: Some(Instant::now()),
            last_retry: None,
        }
    }
}
