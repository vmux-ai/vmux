use super::NativeBridge;
use crate::present::WindowedFrameRect;

impl NativeBridge {
    pub fn windowed_page_contains_point(_x_px: f32, _y_px: f32) -> bool {
        false
    }

    pub fn command_bar_contains_point(_x_px: f32, _y_px: f32) -> bool {
        false
    }

    pub(crate) fn set_windowed_page_frames(
        mut frames: Vec<WindowedFrameRect>,
    ) -> Vec<WindowedFrameRect> {
        frames.clear();
        frames
    }

    pub(crate) fn windowed_page_bounds() -> Option<WindowedFrameRect> {
        None
    }
}
