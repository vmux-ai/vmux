use bevy::prelude::*;

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>();
    }
}

#[derive(Message)]
pub enum AppCommand {
    NewSpace,
    NewWindow,
    SplitPaneHorizontal,
    SplitPaneVertical,
    NewTab,
    CloseTab,
}
