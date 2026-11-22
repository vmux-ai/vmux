use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::system::RunSystemOnce;
use bevy_ecs::world::DeferredWorld;

mod callbacks {
    use super::*;

    pub fn added(_: DeferredWorld, _: HookContext) {}

    pub(crate) fn removed(_: DeferredWorld, _: HookContext) {}

    pub fn once() {}

    pub fn explicit(_: DeferredWorld, _: HookContext) {}

    pub fn tail(_: DeferredWorld, _: HookContext) {}

    pub fn fallible_added(_: DeferredWorld, _: HookContext) {}

    pub fn fallible_inserted(_: DeferredWorld, _: HookContext) {}

    pub fn fallible_discarded(_: DeferredWorld, _: HookContext) {}

    pub fn fallible_removed(_: DeferredWorld, _: HookContext) {}

    pub fn fallible_despawned(_: DeferredWorld, _: HookContext) {}

    struct Explicit;

    impl Component for Explicit {
        const STORAGE_TYPE: bevy_ecs::component::StorageType = bevy_ecs::component::StorageType::Table;
        type Mutability = bevy_ecs::component::Mutable;

        fn on_add() -> Option<bevy_ecs::lifecycle::ComponentHook> {
            return Some(explicit);
        }

        fn on_insert() -> Option<bevy_ecs::lifecycle::ComponentHook> {
            Some(tail)
        }
    }

    #[derive(Component)]
    struct Plain;

    struct UnrelatedHooks;

    impl UnrelatedHooks {
        fn try_on_add(&self, _: fn()) {}
    }

    pub fn unrelated() {}

    #[derive(Component)]
    #[component(on_add = added)]
    struct Pending;

    pub fn install() {
        World::new().run_system_once(once).unwrap();
        World::new()
            .register_component_hooks::<Pending>()
            .on_remove(removed);
        World::new().register_component_hooks::<Plain>().try_on_add(fallible_added);
        World::new().register_component_hooks::<Plain>().try_on_insert(fallible_inserted);
        World::new().register_component_hooks::<Plain>().try_on_discard(fallible_discarded);
        World::new().register_component_hooks::<Plain>().try_on_remove(fallible_removed);
        World::new().register_component_hooks::<Plain>().try_on_despawn(fallible_despawned);
        UnrelatedHooks.try_on_add(unrelated);
    }
}

fn main() {
    callbacks::install();
}
