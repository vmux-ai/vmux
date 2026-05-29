use core::any::{Any, TypeId};

use bevy_ecs::component::ComponentCloneBehavior;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::reflect::{ReflectMapEntities, ReflectResource};
use bevy_ecs::relationship::RelationshipHookMode;
use bevy_ecs::{
    component::{Component, ComponentId},
    entity::{Entity, EntityHashMap, SceneEntityMapper},
    reflect::{AppTypeRegistry, ReflectComponent},
    resource::Resource,
    world::World,
};
use bevy_platform::collections::{hash_set::IntoIter, HashSet};
use bevy_reflect::{PartialReflect, ReflectFromReflect, TypePath, TypeRegistration, TypeRegistry};
use thiserror::Error;

use alloc::collections::BTreeMap;

pub mod serde;

pub(super) fn clone_reflect_value(
    value: &dyn PartialReflect,
    type_registration: &TypeRegistration,
) -> Box<dyn PartialReflect> {
    value
        .reflect_clone()
        .map(PartialReflect::into_partial_reflect)
        .unwrap_or_else(|_| {
            type_registration
                .data::<ReflectFromReflect>()
                .and_then(|fr| fr.from_reflect(value.as_partial_reflect()))
                .map(PartialReflect::into_partial_reflect)
                .unwrap_or_else(|| value.to_dynamic())
        })
}

#[derive(Error, Debug)]
pub enum SceneSpawnError {
    #[error("scene contains the unregistered component `{type_path}`. consider adding `#[reflect(Component)]` to your type")]
    UnregisteredComponent { type_path: String },
    #[error("scene contains the unregistered resource `{type_path}`. consider adding `#[reflect(Resource)]` to your type")]
    UnregisteredResource { type_path: String },
    #[error(
        "scene contains the reflected type `{type_path}` but it was not found in the type registry. \
        consider registering the type using `app.register_type::<T>()``"
    )]
    UnregisteredButReflectedType { type_path: String },
    #[error("scene contains dynamic type `{type_path}` without a represented type. consider changing this using `set_represented_type`.")]
    NoRepresentedType { type_path: String },
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum SceneFilter {
    #[default]
    Unset,
    Allowlist(HashSet<TypeId>),
    Denylist(HashSet<TypeId>),
}

impl SceneFilter {
    pub fn allow_all() -> Self {
        Self::Denylist(HashSet::default())
    }

    pub fn deny_all() -> Self {
        Self::Allowlist(HashSet::default())
    }

    #[must_use]
    pub fn allow<T: Any>(self) -> Self {
        self.allow_by_id(TypeId::of::<T>())
    }

    #[must_use]
    pub fn allow_by_id(mut self, type_id: TypeId) -> Self {
        match &mut self {
            Self::Unset => {
                self = Self::Allowlist([type_id].into_iter().collect());
            }
            Self::Allowlist(list) => {
                list.insert(type_id);
            }
            Self::Denylist(list) => {
                list.remove(&type_id);
            }
        }
        self
    }

    #[must_use]
    pub fn deny<T: Any>(self) -> Self {
        self.deny_by_id(TypeId::of::<T>())
    }

    #[must_use]
    pub fn deny_by_id(mut self, type_id: TypeId) -> Self {
        match &mut self {
            Self::Unset => self = Self::Denylist([type_id].into_iter().collect()),
            Self::Allowlist(list) => {
                list.remove(&type_id);
            }
            Self::Denylist(list) => {
                list.insert(type_id);
            }
        }
        self
    }

    pub fn is_allowed<T: Any>(&self) -> bool {
        self.is_allowed_by_id(TypeId::of::<T>())
    }

    pub fn is_allowed_by_id(&self, type_id: TypeId) -> bool {
        match self {
            Self::Unset => true,
            Self::Allowlist(list) => list.contains(&type_id),
            Self::Denylist(list) => !list.contains(&type_id),
        }
    }

    pub fn is_denied<T: Any>(&self) -> bool {
        self.is_denied_by_id(TypeId::of::<T>())
    }

    pub fn is_denied_by_id(&self, type_id: TypeId) -> bool {
        !self.is_allowed_by_id(type_id)
    }

    pub fn iter(&self) -> Box<dyn ExactSizeIterator<Item = &TypeId> + '_> {
        match self {
            Self::Unset => Box::new(core::iter::empty()),
            Self::Allowlist(list) | Self::Denylist(list) => Box::new(list.iter()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Unset => 0,
            Self::Allowlist(list) | Self::Denylist(list) => list.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Unset => true,
            Self::Allowlist(list) | Self::Denylist(list) => list.is_empty(),
        }
    }
}

impl IntoIterator for SceneFilter {
    type Item = TypeId;
    type IntoIter = IntoIter<TypeId>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Unset => Default::default(),
            Self::Allowlist(list) | Self::Denylist(list) => list.into_iter(),
        }
    }
}

#[derive(TypePath, Default)]
pub struct DynamicScene {
    pub resources: Vec<Box<dyn PartialReflect>>,
    pub entities: Vec<DynamicEntity>,
}

pub struct DynamicEntity {
    pub entity: Entity,
    pub components: Vec<Box<dyn PartialReflect>>,
}

impl DynamicScene {
    pub fn from_world(world: &World) -> Self {
        DynamicSceneBuilder::from_world(world)
            .extract_entities(
                world
                    .archetypes()
                    .iter()
                    .flat_map(bevy_ecs::archetype::Archetype::entities)
                    .map(bevy_ecs::archetype::ArchetypeEntity::id),
            )
            .extract_resources()
            .build()
    }

    pub fn write_to_world_with(
        &self,
        world: &mut World,
        entity_map: &mut EntityHashMap<Entity>,
        type_registry: &AppTypeRegistry,
    ) -> Result<(), SceneSpawnError> {
        let type_registry = type_registry.read();

        for scene_entity in &self.entities {
            entity_map
                .entry(scene_entity.entity)
                .or_insert_with(|| world.spawn_empty().id());
        }

        for scene_entity in &self.entities {
            let entity = *entity_map
                .get(&scene_entity.entity)
                .expect("should have previously spawned an empty entity");

            for component in &scene_entity.components {
                let type_info = component.get_represented_type_info().ok_or_else(|| {
                    SceneSpawnError::NoRepresentedType {
                        type_path: component.reflect_type_path().to_string(),
                    }
                })?;
                let registration = type_registry.get(type_info.type_id()).ok_or_else(|| {
                    SceneSpawnError::UnregisteredButReflectedType {
                        type_path: type_info.type_path().to_string(),
                    }
                })?;
                let reflect_component =
                    registration.data::<ReflectComponent>().ok_or_else(|| {
                        SceneSpawnError::UnregisteredComponent {
                            type_path: type_info.type_path().to_string(),
                        }
                    })?;

                {
                    let component_id = reflect_component.register_component(world);
                    let component_info =
                        unsafe { world.components().get_info_unchecked(component_id) };
                    if matches!(
                        *component_info.clone_behavior(),
                        ComponentCloneBehavior::Ignore
                    ) {
                        continue;
                    }
                }

                SceneEntityMapper::world_scope(entity_map, world, |world, mapper| {
                    reflect_component.apply_or_insert_mapped(
                        &mut world.entity_mut(entity),
                        component.as_partial_reflect(),
                        &type_registry,
                        mapper,
                        RelationshipHookMode::Skip,
                    );
                });
            }
        }

        for resource in &self.resources {
            let type_info = resource.get_represented_type_info().ok_or_else(|| {
                SceneSpawnError::NoRepresentedType {
                    type_path: resource.reflect_type_path().to_string(),
                }
            })?;
            let registration = type_registry.get(type_info.type_id()).ok_or_else(|| {
                SceneSpawnError::UnregisteredButReflectedType {
                    type_path: type_info.type_path().to_string(),
                }
            })?;
            registration.data::<ReflectResource>().ok_or_else(|| {
                SceneSpawnError::UnregisteredResource {
                    type_path: type_info.type_path().to_string(),
                }
            })?;
            let reflect_component = registration.data::<ReflectComponent>().ok_or_else(|| {
                SceneSpawnError::UnregisteredResource {
                    type_path: type_info.type_path().to_string(),
                }
            })?;

            let resource_id = reflect_component.register_component(world);

            let mut cloned_resource =
                clone_reflect_value(resource.as_partial_reflect(), registration);
            if let Some(map_entities) = registration.data::<ReflectMapEntities>() {
                SceneEntityMapper::world_scope(entity_map, world, |_, mapper| {
                    map_entities.map_entities(cloned_resource.as_partial_reflect_mut(), mapper);
                });
            }

            world.insert_reflect_resource(resource_id, cloned_resource);
        }

        Ok(())
    }

    pub fn write_to_world(
        &self,
        world: &mut World,
        entity_map: &mut EntityHashMap<Entity>,
    ) -> Result<(), SceneSpawnError> {
        let registry = world.resource::<AppTypeRegistry>().clone();
        self.write_to_world_with(world, entity_map, &registry)
    }

    pub fn serialize(&self, registry: &TypeRegistry) -> Result<String, ron::Error> {
        serialize_ron(serde::SceneSerializer::new(self, registry))
    }
}

pub fn serialize_ron<S>(serialize: S) -> Result<String, ron::Error>
where
    S: ::serde::Serialize,
{
    let pretty_config = ron::ser::PrettyConfig::default()
        .indentor("  ".to_string())
        .new_line("\n".to_string());
    ron::ser::to_string_pretty(&serialize, pretty_config)
}

pub struct DynamicSceneBuilder<'w> {
    extracted_resources: BTreeMap<ComponentId, Box<dyn PartialReflect>>,
    extracted_scene: BTreeMap<Entity, DynamicEntity>,
    component_filter: SceneFilter,
    resource_filter: SceneFilter,
    original_world: &'w World,
}

impl<'w> DynamicSceneBuilder<'w> {
    pub fn from_world(world: &'w World) -> Self {
        Self {
            extracted_resources: Default::default(),
            extracted_scene: Default::default(),
            component_filter: SceneFilter::default(),
            resource_filter: SceneFilter::default(),
            original_world: world,
        }
    }

    #[must_use]
    pub fn with_component_filter(mut self, filter: SceneFilter) -> Self {
        self.component_filter = filter;
        self
    }

    #[must_use]
    pub fn with_resource_filter(mut self, filter: SceneFilter) -> Self {
        self.resource_filter = filter;
        self
    }

    pub fn allow_all(mut self) -> Self {
        self.component_filter = SceneFilter::allow_all();
        self.resource_filter = SceneFilter::allow_all();
        self
    }

    pub fn deny_all(mut self) -> Self {
        self.component_filter = SceneFilter::deny_all();
        self.resource_filter = SceneFilter::deny_all();
        self
    }

    #[must_use]
    pub fn allow_component<T: Component>(mut self) -> Self {
        self.component_filter = self.component_filter.allow::<T>();
        self
    }

    #[must_use]
    pub fn deny_component<T: Component>(mut self) -> Self {
        self.component_filter = self.component_filter.deny::<T>();
        self
    }

    #[must_use]
    pub fn allow_all_components(mut self) -> Self {
        self.component_filter = SceneFilter::allow_all();
        self
    }

    #[must_use]
    pub fn deny_all_components(mut self) -> Self {
        self.component_filter = SceneFilter::deny_all();
        self
    }

    #[must_use]
    pub fn allow_resource<T: Resource>(mut self) -> Self {
        self.resource_filter = self.resource_filter.allow::<T>();
        self
    }

    #[must_use]
    pub fn deny_resource<T: Resource>(mut self) -> Self {
        self.resource_filter = self.resource_filter.deny::<T>();
        self
    }

    #[must_use]
    pub fn allow_all_resources(mut self) -> Self {
        self.resource_filter = SceneFilter::allow_all();
        self
    }

    #[must_use]
    pub fn deny_all_resources(mut self) -> Self {
        self.resource_filter = SceneFilter::deny_all();
        self
    }

    #[must_use]
    pub fn build(self) -> DynamicScene {
        DynamicScene {
            resources: self.extracted_resources.into_values().collect(),
            entities: self.extracted_scene.into_values().collect(),
        }
    }

    #[must_use]
    pub fn extract_entity(self, entity: Entity) -> Self {
        self.extract_entities(core::iter::once(entity))
    }

    #[must_use]
    pub fn remove_empty_entities(mut self) -> Self {
        self.extracted_scene
            .retain(|_, entity| !entity.components.is_empty());

        self
    }

    #[must_use]
    pub fn extract_entities(mut self, entities: impl Iterator<Item = Entity>) -> Self {
        let type_registry = self.original_world.resource::<AppTypeRegistry>().read();

        for entity in entities {
            if self.extracted_scene.contains_key(&entity) {
                continue;
            }

            let mut entry = DynamicEntity {
                entity,
                components: Vec::new(),
            };

            let original_entity = self.original_world.entity(entity);
            for &component_id in original_entity.archetype().components().iter() {
                let mut extract_and_push = || {
                    let type_id = self
                        .original_world
                        .components()
                        .get_info(component_id)?
                        .type_id()?;

                    let is_denied = self.component_filter.is_denied_by_id(type_id);

                    if is_denied {
                        return None;
                    }

                    let type_registration = type_registry.get(type_id)?;

                    let component = type_registration
                        .data::<ReflectComponent>()?
                        .reflect(original_entity)?;

                    let component =
                        clone_reflect_value(component.as_partial_reflect(), type_registration);

                    entry.components.push(component);
                    Some(())
                };
                extract_and_push();
            }
            self.extracted_scene.insert(entity, entry);
        }

        self
    }

    #[must_use]
    pub fn extract_resources(mut self) -> Self {
        let original_world_dqf_id = self
            .original_world
            .components()
            .get_valid_id(TypeId::of::<DefaultQueryFilters>());

        let type_registry = self.original_world.resource::<AppTypeRegistry>().read();

        for (component_id, resource_entity) in self.original_world.resource_entities().iter() {
            if Some(component_id) == original_world_dqf_id {
                continue;
            }
            let mut extract_and_push = || {
                let type_id = self
                    .original_world
                    .components()
                    .get_info(component_id)?
                    .type_id()?;

                let is_denied = self.resource_filter.is_denied_by_id(type_id);

                if is_denied {
                    return None;
                }

                let type_registration = type_registry.get(type_id)?;

                type_registration.data::<ReflectResource>()?;

                let resource = self
                    .original_world
                    .get_reflect(resource_entity, type_id)
                    .ok()?;

                let resource =
                    clone_reflect_value(resource.as_partial_reflect(), type_registration);

                self.extracted_resources.insert(component_id, resource);
                Some(())
            };
            extract_and_push();
        }

        drop(type_registry);
        self
    }
}
