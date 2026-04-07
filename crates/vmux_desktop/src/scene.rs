#[cfg(target_os = "macos")]
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

#[cfg(target_os = "macos")]
use bevy::window::RawHandleWrapper;
#[cfg(target_os = "macos")]
use liquid_glass_rs::{GlassOptions, GlassViewManager};
#[cfg(target_os = "macos")]
use raw_window_handle::RawWindowHandle;
#[cfg(target_os = "macos")]
use std::marker::PhantomData;
#[cfg(target_os = "macos")]
use std::rc::Rc;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spawn3dCamera;

#[derive(Default)]
pub struct ScenePlugin;

#[cfg(target_os = "macos")]
struct LiquidGlassMainThread(PhantomData<Rc<()>>);

#[cfg(target_os = "macos")]
impl Default for LiquidGlassMainThread {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Startup, Spawn3dCamera)
            .add_systems(
                Startup,
                (spawn_camera, spawn_directional_light)
                    .chain()
                    .in_set(Spawn3dCamera),
            );

        #[cfg(target_os = "macos")]
        app.insert_resource(ClearColor(Color::NONE))
            .insert_non_send_resource(LiquidGlassMainThread::default())
            .add_systems(Update, apply_liquid_glass);
    }
}

fn spawn_camera(mut commands: Commands) {
    {
        commands.spawn((
            Camera3d::default(),
            Tonemapping::None,
            Transform::from_translation(Vec3::new(0., 0., 3.0)).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
}

fn spawn_directional_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::new(1., 1., 1.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

#[cfg(target_os = "macos")]
fn apply_liquid_glass(
    _main_thread: NonSend<LiquidGlassMainThread>,
    query: Query<(Entity, &RawHandleWrapper), Added<Window>>,
) {
    for (entity, wrapper) in query.iter() {
        let ptr = match wrapper.get_window_handle() {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr().cast::<std::ffi::c_void>(),
            _ => continue,
        };
        if ptr.is_null() {
            continue;
        }

        let manager = GlassViewManager::new();
        match manager.add_glass_view(ptr, GlassOptions::default()) {
            Ok(_) => info!("Liquid Glass successfully applied to window: {:?}", entity),
            Err(e) => bevy_log::error!("Window {:?} not ready for glass: {:?}", entity, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_registers_in_app() {
        let mut app = App::new();
        app.add_plugins(ScenePlugin);
    }
}
