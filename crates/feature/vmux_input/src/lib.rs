use bevy::prelude::*;

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
pub(crate) type Feature = CapturePlugin;

mod capture;
#[cfg(target_os = "macos")]
mod keyboard;
pub mod pointer;

pub use capture::*;
#[cfg(target_os = "macos")]
pub use keyboard::KeyboardPlugin;

#[derive(Message, Clone, Copy, Debug)]
pub struct ExitFullscreenShortcut;

#[derive(Message, Clone, Copy, Debug)]
pub struct HideWindowsShortcut;

#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardContext {
    pub page_owns_escape: bool,
    pub text_entry_owns_keys: bool,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyboardContextSet;
