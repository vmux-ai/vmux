use bevy::prelude::*;

use crate::plugin::{
    RunShellRequest, TerminalFontSizeCommand, TerminalReinputRequest, TerminalSendRequest,
};

pub struct TerminalContractPlugin;

impl Plugin for TerminalContractPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RunShellRequest>()
            .add_message::<TerminalFontSizeCommand>()
            .add_message::<TerminalReinputRequest>()
            .add_message::<TerminalSendRequest>();
    }

    fn is_unique(&self) -> bool {
        false
    }
}
