#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPaletteSurface {
    Modal,
    Start,
}

impl CommandPaletteSurface {
    pub const fn is_start(self) -> bool {
        matches!(self, Self::Start)
    }
}
