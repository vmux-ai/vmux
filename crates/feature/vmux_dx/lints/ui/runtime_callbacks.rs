use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::DeferredWorld;

mod callbacks {
    use super::*;

    pub fn added(_: DeferredWorld, _: HookContext) {}

    pub(crate) fn removed(_: DeferredWorld, _: HookContext) {}

    pub fn once() {}

    #[derive(Component)]
    #[component(on_add = added)]
    struct Pending;

    pub fn install() {
        World::new().run_system_once(once).unwrap();
        World::new()
            .register_component_hooks::<Pending>()
            .on_remove(removed);
    }
}

fn main() {
    callbacks::install();
}
