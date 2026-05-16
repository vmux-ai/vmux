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
            .register_type::<Url>()
            .register_type::<VisitCount>()
            .register_type::<LastVisitedAt>()
            .register_type::<VisitedUrl>()
            .register_type::<TransitionType>()
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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct Url;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitCount(pub u32);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct LastVisitedAt(pub i64);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_history"]
pub struct VisitedUrl(pub Entity);

#[cfg(not(target_arch = "wasm32"))]
impl Default for VisitedUrl {
    fn default() -> Self {
        Self(Entity::PLACEHOLDER)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_history"]
pub enum TransitionType {
    #[default]
    Link,
    Typed,
    Reload,
    BackForward,
    Redirect,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_new_history_components() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(registry.get(std::any::TypeId::of::<Url>()).is_some());
        assert!(registry.get(std::any::TypeId::of::<VisitCount>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<LastVisitedAt>())
                .is_some()
        );
        assert!(registry.get(std::any::TypeId::of::<VisitedUrl>()).is_some());
        assert!(
            registry
                .get(std::any::TypeId::of::<TransitionType>())
                .is_some()
        );
    }
}
