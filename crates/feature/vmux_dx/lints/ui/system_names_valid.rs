use bevy_app::{App, Update};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::DeferredWorld;

mod bookmark {
    use super::*;

    fn pin() {}

    fn on_pin() {}

    fn condition() -> bool {
        true
    }

    fn observe(_: On<Add, Pending>) {}

    fn once() {}

    fn bind(_: DeferredWorld, _: HookContext) {}

    fn remove(_: DeferredWorld, _: HookContext) {}

    #[derive(Component)]
    #[component(on_add = bind)]
    struct Pending;

    pub fn install() {
        App::new()
            .add_systems(Update, (pin, on_pin).run_if(condition))
            .add_observer(observe);
        World::new().run_system_once(once).unwrap();
        World::new()
            .register_component_hooks::<Pending>()
            .on_remove(remove);
    }
}

struct Unrelated;

impl Unrelated {
    fn add_systems(&self, _: fn()) {}

    fn run_system_once(&self, _: fn()) {}

    fn on_add(&self, _: fn()) {}
}

fn unrelated_system() {}

fn main() {
    bookmark::install();
    Unrelated.add_systems(unrelated_system);
    Unrelated.run_system_once(unrelated_system);
    Unrelated.on_add(unrelated_system);
}
