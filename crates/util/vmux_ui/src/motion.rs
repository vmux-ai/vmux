pub struct Motion;

impl Motion {
    pub const FEEDBACK_MS: u32 = 80;
    pub const CONTENT_MS: u32 = 100;
    pub const LAYOUT_MS: u32 = 120;
    pub const MAX_MS: u32 = 160;

    pub fn bounded(duration_ms: u32) -> u32 {
        duration_ms.min(Self::MAX_MS)
    }
}
