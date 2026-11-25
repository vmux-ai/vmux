use std::marker::PhantomData;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryFilter;
use bevy_ecs::reflect::AppTypeRegistry;
use bevy_reflect::{FromType, TypeRegistry};
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::SaveWorld;

pub struct SchemaPlugin<S: Schema>(PhantomData<fn() -> S>);

impl<S: Schema> Default for SchemaPlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: Schema> Plugin for SchemaPlugin<S> {
    fn build(&self, app: &mut App) {
        app.init_resource::<AppTypeRegistry>();
        let filter = {
            let mut registry = app.world().resource::<AppTypeRegistry>().write();
            S::register(&mut registry);
            S::filter(&registry)
        };
        S::track(app);
        app.insert_resource(Types::<S> {
            filter,
            schema: PhantomData,
        });
    }
}

pub trait Schema: Send + Sync + Sized + 'static {
    fn register(registry: &mut TypeRegistry);

    fn filter(registry: &TypeRegistry) -> WorldFilter {
        StoreDomain::<Self>::filter(registry)
    }

    fn track(_app: &mut App) {}
}

#[derive(Resource)]
pub struct Types<S: Schema> {
    filter: WorldFilter,
    schema: PhantomData<fn() -> S>,
}

impl<S: Schema> Clone for Types<S> {
    fn clone(&self) -> Self {
        Self {
            filter: self.filter.clone(),
            schema: PhantomData,
        }
    }
}

impl<S: Schema> Types<S> {
    pub fn apply<F: QueryFilter>(&self, save: &mut SaveWorld<F>) {
        save.components = self.filter.clone();
        save.resources = self.filter.clone();
    }
}

pub struct StoreDomain<D>(PhantomData<fn() -> D>);

impl<D> Clone for StoreDomain<D> {
    fn clone(&self) -> Self {
        Self(PhantomData)
    }
}

impl<T, D: 'static> FromType<T> for StoreDomain<D> {
    fn from_type() -> Self {
        Self(PhantomData)
    }
}

impl<D: 'static> StoreDomain<D> {
    pub fn filter(registry: &TypeRegistry) -> WorldFilter {
        let mut filter = WorldFilter::deny_all();
        for registration in registry.iter() {
            if registration.data::<Self>().is_some() {
                filter = filter.allow_by_id(registration.type_id());
            }
        }
        filter
    }
}
