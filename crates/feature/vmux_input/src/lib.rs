use bevy::prelude::*;

#[cfg(target_os = "macos")]
mod keyboard;
pub mod pointer;

#[cfg(target_os = "macos")]
pub use keyboard::KeyboardPlugin;

#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardContext {
    pub page_owns_escape: bool,
    pub text_entry_owns_keys: bool,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyboardContextSet;
