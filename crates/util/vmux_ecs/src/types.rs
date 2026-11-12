use bevy_app::App;
use bevy_ecs::reflect::AppTypeRegistry;
pub use bevy_reflect::TypeRegistry;

pub trait Schema: Send + Sync + 'static {
    fn register(registry: &mut TypeRegistry);
}

pub trait AppExt {
    fn register_types<S: Schema>(&mut self) -> &mut Self;
}

impl AppExt for App {
    fn register_types<S: Schema>(&mut self) -> &mut Self {
        self.init_resource::<AppTypeRegistry>();
        {
            let mut registry = self.world().resource::<AppTypeRegistry>().write();
            S::register(&mut registry);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "store")]
    use crate::store::{SchemaPlugin, StoreDomain, WorkspaceStore};
    use bevy_ecs::prelude::*;
    use bevy_ecs::reflect::ReflectComponent;
    use bevy_reflect::Reflect;
    use bevy_reflect::tuple_struct::DynamicTupleStruct;
    #[cfg(feature = "store")]
    use moonshine_save::prelude::Save;
    use std::any::TypeId;

    use super::*;

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Amount(i32);

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Label(String);

    #[crate::types(Amount, Label)]
    struct Declaration;

    #[cfg(feature = "store")]
    #[crate::store(domain = WorkspaceStore; Amount)]
    struct Durable;

    #[test]
    fn declared_component_types_support_dynamic_insertion_after_repeated_registration() {
        let mut app = App::new();
        app.register_types::<Declaration>()
            .register_types::<Declaration>();
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let entity = app.world_mut().spawn_empty().id();
        let registration = registry.get(TypeId::of::<Amount>()).unwrap();
        let mut value = DynamicTupleStruct::default();
        value.set_represented_type(Some(registration.type_info()));
        value.insert(23i32);
        registration.data::<ReflectComponent>().unwrap().insert(
            &mut app.world_mut().entity_mut(entity),
            &value,
            &registry,
        );
        let registration = registry.get(TypeId::of::<Label>()).unwrap();
        let mut value = DynamicTupleStruct::default();
        value.set_represented_type(Some(registration.type_info()));
        value.insert(String::from("independent"));
        registration.data::<ReflectComponent>().unwrap().insert(
            &mut app.world_mut().entity_mut(entity),
            &value,
            &registry,
        );
        assert_eq!(app.world().get::<Amount>(entity).unwrap().0, 23);
        assert_eq!(app.world().get::<Label>(entity).unwrap().0, "independent");
    }

    #[cfg(feature = "store")]
    #[test]
    fn reflection_only_registration_is_transient_and_keeps_explicit_store_metadata() {
        let mut app = App::new();
        app.register_types::<Declaration>();
        let entity = app.world_mut().spawn(Amount(7)).id();
        assert!(app.world().get::<Save>(entity).is_none());
        {
            let registry = app.world().resource::<AppTypeRegistry>().read();
            assert!(
                registry
                    .get(TypeId::of::<Amount>())
                    .unwrap()
                    .data::<StoreDomain<WorkspaceStore>>()
                    .is_none()
            );
        }
        app.add_plugins(SchemaPlugin::<Durable>::default());
        app.register_types::<Declaration>();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(TypeId::of::<Amount>())
                .unwrap()
                .data::<StoreDomain<WorkspaceStore>>()
                .is_some()
        );
        assert!(
            registry
                .get(TypeId::of::<Label>())
                .unwrap()
                .data::<StoreDomain<WorkspaceStore>>()
                .is_none()
        );
        assert!(app.world().get::<Save>(entity).is_none());
    }
}
