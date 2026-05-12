pub mod process_id;
pub use process_id::ProcessId;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use moonshine_save::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub struct CorePlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PageMetadata>()
            .register_type::<CreatedAt>()
            .register_type::<LastActivatedAt>()
            .register_type::<Visit>()
            .register_type::<Children>()
            .register_type::<ChildOf>();
    }
}

// ── Time helpers ─────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Shared components ────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[type_path = "vmux_header::system"]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub favicon_url: String,
    pub bg_color: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
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
#[type_path = "vmux_history"]
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
#[type_path = "vmux_history"]
pub struct Visit;
