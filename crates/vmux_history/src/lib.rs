pub mod event;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use moonshine_save::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct CreatedAt(pub i64);

#[cfg(not(target_arch = "wasm32"))]
impl CreatedAt {
    pub fn now() -> Self {
        Self(now_millis())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct LastActivatedAt(pub i64);

#[cfg(not(target_arch = "wasm32"))]
impl LastActivatedAt {
    pub fn now() -> Self {
        Self(now_millis())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Visit;

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
