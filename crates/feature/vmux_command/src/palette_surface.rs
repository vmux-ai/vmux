#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandPaletteSurface {
    #[default]
    Modal,
    Start,
}

impl CommandPaletteSurface {
    pub const fn is_start(self) -> bool {
        matches!(self, Self::Start)
    }
}
