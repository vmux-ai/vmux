use bevy_app::{App, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::DeferredWorld;

mod bookmark {
    use super::*;

    fn bookmark_pin() {}

    fn pin_system() {}

    fn pin() {}

    fn on_pin() {}

    fn bookmark_condition() -> bool {
        true
    }

    fn bookmark_observe(_: On<Add, Pending>) {}

    fn bookmark_once() {}

    fn bookmark_bind(_: DeferredWorld, _: HookContext) {}

    fn bookmark_remove(_: DeferredWorld, _: HookContext) {}

    #[derive(Component)]
    #[component(on_add = bookmark_bind)]
    struct Pending;

    pub fn install() {
        let aliased = bookmark_pin;
        App::new()
            .add_systems(
                Update,
                (aliased, pin_system, pin, on_pin).run_if(bookmark_condition),
            )
            .add_observer(bookmark_observe);
        World::new().run_system_once(bookmark_once).unwrap();
        World::new()
            .register_component_hooks::<Pending>()
            .on_remove(bookmark_remove);
    }
}

fn lints_sync() {}

fn main() {
    bookmark::install();
    App::new().add_systems(Update, lints_sync);
}
