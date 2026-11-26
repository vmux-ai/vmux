use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use bevy_reflect::FromReflect;
use bevy_world_serialization::{DynamicEntity, DynamicWorld};

#[derive(Component)]
pub struct StoreValidator {
    pub name: &'static str,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub rejects: fn(&str) -> bool,
}

#[derive(Component)]
pub struct RestoreValidator {
    pub name: &'static str,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub validate: fn(&DynamicWorld) -> Result<(), String>,
}

impl RestoreValidator {
    pub fn unique<T>(name: &'static str) -> Self
    where
        T: Component + FromReflect + Eq + Hash,
    {
        Self {
            name,
            validate: Self::validate_unique::<T>,
        }
    }

    pub fn ownership<R: Component>() -> Self {
        Self {
            name: "ownership graph",
            validate: Self::validate_ownership::<R>,
        }
    }

    fn validate_unique<T>(snapshot: &DynamicWorld) -> Result<(), String>
    where
        T: Component + FromReflect + Eq + Hash,
    {
        let mut seen = HashSet::new();
        for entity in &snapshot.entities {
            let Some(value) = RestoreEntity(entity).component::<T>()? else {
                continue;
            };
            if !seen.insert(value) {
                return Err(format!("duplicate {}", std::any::type_name::<T>()));
            }
        }
        Ok(())
    }

    fn validate_ownership<R: Component>(snapshot: &DynamicWorld) -> Result<(), String> {
        let mut entities = HashSet::new();
        let mut parents = HashMap::new();
        for entity in &snapshot.entities {
            if !entities.insert(entity.entity) {
                return Err(format!("duplicate entity {}", entity.entity));
            }
            if let Some(parent) = RestoreEntity(entity).component::<ChildOf>()? {
                parents.insert(entity.entity, parent.parent());
            }
        }
        for entity in &snapshot.entities {
            let Some(parent) = parents.get(&entity.entity) else {
                continue;
            };
            if !entities.contains(parent) && !RestoreEntity(entity).contains::<R>() {
                return Err(format!("{} has missing owner {parent}", entity.entity));
            }
            let mut visited = HashSet::new();
            let mut current = entity.entity;
            while let Some(parent) = parents.get(&current) {
                if !visited.insert(current) {
                    return Err(format!("ownership cycle contains {current}"));
                }
                current = *parent;
            }
        }
        Ok(())
    }
}

struct RestoreEntity<'a>(&'a DynamicEntity);

impl RestoreEntity<'_> {
    fn contains<T: 'static>(&self) -> bool {
        self.0.components.iter().any(|component| {
            component
                .get_represented_type_info()
                .is_some_and(|info| info.type_id() == TypeId::of::<T>())
        })
    }

    fn component<T: FromReflect>(&self) -> Result<Option<T>, String> {
        for component in &self.0.components {
            if component
                .get_represented_type_info()
                .is_none_or(|info| info.type_id() != TypeId::of::<T>())
            {
                continue;
            }
            return T::from_reflect(component.as_partial_reflect())
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "invalid {} on {}",
                        std::any::type_name::<T>(),
                        self.0.entity
                    )
                });
        }
        Ok(None)
    }
}

#[derive(SystemParam)]
pub struct StoreValidators<'w, 's> {
    validators: Query<'w, 's, &'static StoreValidator>,
}

impl StoreValidators<'_, '_> {
    pub fn rejected_by(&self, body: &str) -> Option<&'static str> {
        self.validators
            .iter()
            .find(|validator| (validator.rejects)(body))
            .map(|validator| validator.name)
    }
}
