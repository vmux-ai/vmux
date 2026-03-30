//! Default keyboard shortcuts for vmux apps (e.g. quit).

mod component;
mod system;

pub use component::{AppAction, AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixState};

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Default)]
pub struct VmuxInputPlugin;

impl Plugin for VmuxInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<AppAction>::default())
            .add_systems(Startup, system::spawn_app_input)
            .add_systems(Update, system::exit_on_quit_action);
    }
}
