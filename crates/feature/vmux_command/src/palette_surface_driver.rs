use crate::CommandPaletteSurface;

impl CommandPaletteSurface {
    pub const fn is_start(self) -> bool {
        matches!(self, Self::Start)
    }
}
