//! Input actions and prefix-routing markers (re-exported from [`vmux_core`] for layout sharing).

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub use vmux_core::input_root::{
    AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixChordSet, VmuxPrefixState,
};

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    Quit,
}
