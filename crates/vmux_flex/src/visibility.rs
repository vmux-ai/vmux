use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

impl Visibility {
    pub fn hidden(hidden: bool) -> Self {
        if hidden { Self::Hidden } else { Self::Visible }
    }

    pub fn is_hidden(self) -> bool {
        matches!(self, Self::Hidden)
    }
}
