//! Default keyboard shortcuts for vmux apps (e.g. quit) and tmux-style **Ctrl+B** chord registration.
//!
//! Add `vmux_settings::SettingsPlugin` **before** this plugin so `VmuxAppSettings` is initialized for chord systems.

/// Emacs-style readline bindings for `<input>` / `<textarea>` (Ctrl+A/E/…); **Cmd+A** selects all on macOS.
pub const TEXT_INPUT_EMACS_BINDINGS_PRELOAD: &str = include_str!("text_input_emacs_bindings.js");

mod cef_keyboard_target;
mod component;
mod system;

pub use cef_keyboard_target::{
    consume_keyboard_for_prefix_routing, sync_cef_keyboard_target,
    sync_cef_osr_focus_with_active_pane, sync_cef_pointer_suppression_for_prefix,
};
pub use component::{
    AppAction, AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixChordSet, VmuxPrefixState,
};
pub use system::{TmuxChordInput, ctrl_arrow_focus_commands, tmux_prefix_commands};
pub use vmux_core::Active;

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardInputSet, render_standard_materials};
use leafwing_input_manager::prelude::*;
use vmux_layout::{cycle_pane_focus, split_active_pane, sync_cef_sizes_after_pane_layout};

#[derive(Default)]
pub struct VmuxInputPlugin;

impl Plugin for VmuxInputPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, VmuxPrefixChordSet)
            .add_plugins(InputManagerPlugin::<AppAction>::default())
            .add_systems(Startup, system::spawn_app_input)
            .add_systems(
                PreUpdate,
                (
                    cef_keyboard_target::consume_keyboard_for_prefix_routing
                        .after(InputSystems)
                        .before(CefKeyboardInputSet),
                    cef_keyboard_target::sync_cef_pointer_suppression_for_prefix
                        .after(InputSystems)
                        .before(CefKeyboardInputSet),
                    cef_keyboard_target::sync_cef_keyboard_target
                        .after(InputSystems)
                        .before(CefKeyboardInputSet),
                ),
            )
            .add_systems(
                PostUpdate,
                cef_keyboard_target::sync_cef_osr_focus_with_active_pane
                    .after(sync_cef_sizes_after_pane_layout)
                    .before(render_standard_materials),
            )
            .add_systems(
                Update,
                (
                    system::exit_on_quit_action,
                    system::ctrl_arrow_focus_commands,
                    system::tmux_prefix_commands.in_set(VmuxPrefixChordSet),
                ),
            )
            .add_systems(
                Update,
                cef_keyboard_target::sync_cef_keyboard_target
                    .after(split_active_pane)
                    .after(cycle_pane_focus),
            );
    }
}
