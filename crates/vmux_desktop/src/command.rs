use bevy::prelude::*;
use strum::{Display, EnumIter, EnumProperty, IntoEnumIterator};
use vmux_macro::{NativeMenu, NativeMenuLeaf};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, _app: &mut App) {}
}

#[derive(Event, NativeMenu, Debug, Clone, Copy, PartialEq, Eq, Display, EnumIter, EnumProperty)]
pub enum AppCommand {
    #[strum(props(Label = "Space"))]
    Space(SpaceCommand),

    #[strum(props(Label = "Pane"))]
    Pane(PaneCommand),
}

#[derive(
    NativeMenuLeaf, Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, EnumProperty,
)]
pub enum SpaceCommand {
    #[default]
    #[strum(props(Id = "new_space", Label = "New Space"))]
    New,
}

#[derive(
    NativeMenuLeaf, Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumIter, EnumProperty,
)]
pub enum PaneCommand {
    #[default]
    #[strum(props(Id = "split_v", Label = "Split Vertically"))]
    SplitV,
    #[strum(props(Id = "split_h", Label = "Split Horizontally"))]
    SplitH,
}

pub fn app_command_from_menu_id(id: &str) -> Option<AppCommand> {
    SpaceCommand::iter()
        .find(|c| c.get_str("Id") == Some(id))
        .map(AppCommand::Space)
        .or_else(|| {
            PaneCommand::iter()
                .find(|c| c.get_str("Id") == Some(id))
                .map(AppCommand::Pane)
        })
}
