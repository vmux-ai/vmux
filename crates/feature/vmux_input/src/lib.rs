use bevy::prelude::*;
use bevy::prelude::{App, Plugin};

pub use capture::*;
pub use native_key::{
    ConsumesNativeKey, NativeKey, NativeKeyCapture, NativeKeyClaimSet, NativeKeyInput,
    NativeKeyInputSet, PassesNativeKey,
};
pub use pointer::{
    PointerCapture, PointerInbox, PointerInput, PointerPlugin, PointerSet, PointerState,
    PointerUpdated, WindowGesture,
};

pub(crate) struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod capture;
#[cfg(all(host, any(target_os = "macos", test)))]
mod keyboard;
#[cfg(target_os = "macos")]
mod keyboard_native;
mod native_key;
mod pointer;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            PointerPlugin,
            capture::CapturePlugin,
            bevy_cef::prelude::UiEventPlugin::<(
                vmux_api::input::KeyStroke,
                vmux_api::input::KeyContextRequest,
            )>::default(),
        ));
        #[cfg(all(host, any(target_os = "macos", test)))]
        app.add_plugins(keyboard::KeyboardPlugin);
    }
}

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
