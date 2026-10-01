#[cfg(host)]
use vmux_api::input::KeyModifiers;

#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeKey {
    pub key: bevy::input::keyboard::KeyCode,
    pub modifiers: KeyModifiers,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct ConsumesNativeKey;

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct PassesNativeKey;

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct NativeKeyCapture;

#[cfg(host)]
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct NativeKeyInput {
    pub key: Option<bevy::input::keyboard::KeyCode>,
    pub native_code: u16,
    pub text: String,
    pub modifiers: KeyModifiers,
    pub repeat: bool,
    pub captured: bool,
    pub claim: Option<bevy::prelude::Entity>,
    pub pressed_at_ms: i64,
}

#[cfg(host)]
impl NativeKeyInput {
    pub fn releases_capture(&self) -> bool {
        self.key == Some(bevy::input::keyboard::KeyCode::Tab)
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && !self.modifiers.super_key
    }
}

#[cfg(host)]
#[derive(bevy::prelude::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeKeyClaimSet;

#[cfg(host)]
#[derive(bevy::prelude::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeKeyInputSet;
