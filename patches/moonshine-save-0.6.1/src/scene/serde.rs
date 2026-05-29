use super::{DynamicEntity, DynamicScene};
use bevy_ecs::entity::Entity;
use bevy_platform::collections::HashSet;
use bevy_reflect::{
    serde::{
        ReflectDeserializer, TypeRegistrationDeserializer, TypedReflectDeserializer,
        TypedReflectSerializer,
    },
    PartialReflect, ReflectFromReflect, TypeRegistry,
};
use core::fmt::Formatter;
use serde::{
    de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
    ser::{SerializeMap, SerializeStruct},
    Deserialize, Deserializer, Serialize, Serializer,
};

pub const SCENE_STRUCT: &str = "Scene";
pub const SCENE_RESOURCES: &str = "resources";
pub const SCENE_ENTITIES: &str = "entities";

pub const ENTITY_STRUCT: &str = "Entity";
pub const ENTITY_FIELD_COMPONENTS: &str = "components";

pub struct SceneSerializer<'a> {
    pub scene: &'a DynamicScene,
    pub registry: &'a TypeRegistry,
}

impl<'a> SceneSerializer<'a> {
    pub fn new(scene: &'a DynamicScene, registry: &'a TypeRegistry) -> Self {
        SceneSerializer { scene, registry }
    }
}

impl<'a> Serialize for SceneSerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(SCENE_STRUCT, 2)?;
        state.serialize_field(
            SCENE_RESOURCES,
            &SceneMapSerializer {
                entries: &self.scene.resources,
                registry: self.registry,
            },
        )?;
        state.serialize_field(
            SCENE_ENTITIES,
            &EntitiesSerializer {
                entities: &self.scene.entities,
                registry: self.registry,
            },
        )?;
        state.end()
    }
}

pub struct EntitiesSerializer<'a> {
    pub entities: &'a [DynamicEntity],
    pub registry: &'a TypeRegistry,
}

impl<'a> Serialize for EntitiesSerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(self.entities.len()))?;
        for entity in self.entities {
            state.serialize_entry(
                &entity.entity,
                &EntitySerializer {
                    entity,
                    registry: self.registry,
                },
            )?;
        }
        state.end()
    }
}

pub struct EntitySerializer<'a> {
    pub entity: &'a DynamicEntity,
    pub registry: &'a TypeRegistry,
}

impl<'a> Serialize for EntitySerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(ENTITY_STRUCT, 1)?;
        state.serialize_field(
            ENTITY_FIELD_COMPONENTS,
            &SceneMapSerializer {
                entries: &self.entity.components,
                registry: self.registry,
            },
        )?;
        state.end()
    }
}

pub struct SceneMapSerializer<'a> {
    pub entries: &'a [Box<dyn PartialReflect>],
    pub registry: &'a TypeRegistry,
}

impl<'a> Serialize for SceneMapSerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(self.entries.len()))?;
        let sorted_entries = {
            let mut entries = self
                .entries
                .iter()
                .map(|entry| {
                    (
                        entry.get_represented_type_info().unwrap().type_path(),
                        entry.as_partial_reflect(),
                    )
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|(type_path, _)| *type_path);
            entries
        };

        for (type_path, partial_reflect) in sorted_entries {
            state.serialize_entry(
                type_path,
                &TypedReflectSerializer::new(partial_reflect, self.registry),
            )?;
        }
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum SceneField {
    Resources,
    Entities,
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum EntityField {
    Components,
}

pub struct SceneDeserializer<'a> {
    pub type_registry: &'a TypeRegistry,
}

impl<'a, 'de> DeserializeSeed<'de> for SceneDeserializer<'a> {
    type Value = DynamicScene;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            SCENE_STRUCT,
            &[SCENE_RESOURCES, SCENE_ENTITIES],
            SceneVisitor {
                type_registry: self.type_registry,
            },
        )
    }
}

struct SceneVisitor<'a> {
    pub type_registry: &'a TypeRegistry,
}

impl<'a, 'de> Visitor<'de> for SceneVisitor<'a> {
    type Value = DynamicScene;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("scene struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let resources = seq
            .next_element_seed(SceneMapDeserializer {
                registry: self.type_registry,
            })?
            .ok_or_else(|| Error::missing_field(SCENE_RESOURCES))?;

        let entities = seq
            .next_element_seed(SceneEntitiesDeserializer {
                type_registry: self.type_registry,
            })?
            .ok_or_else(|| Error::missing_field(SCENE_ENTITIES))?;

        Ok(DynamicScene {
            resources,
            entities,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut resources = None;
        let mut entities = None;
        while let Some(key) = map.next_key()? {
            match key {
                SceneField::Resources => {
                    if resources.is_some() {
                        return Err(Error::duplicate_field(SCENE_RESOURCES));
                    }
                    resources = Some(map.next_value_seed(SceneMapDeserializer {
                        registry: self.type_registry,
                    })?);
                }
                SceneField::Entities => {
                    if entities.is_some() {
                        return Err(Error::duplicate_field(SCENE_ENTITIES));
                    }
                    entities = Some(map.next_value_seed(SceneEntitiesDeserializer {
                        type_registry: self.type_registry,
                    })?);
                }
            }
        }

        let resources = resources.ok_or_else(|| Error::missing_field(SCENE_RESOURCES))?;
        let entities = entities.ok_or_else(|| Error::missing_field(SCENE_ENTITIES))?;

        Ok(DynamicScene {
            resources,
            entities,
        })
    }
}

pub struct SceneEntitiesDeserializer<'a> {
    pub type_registry: &'a TypeRegistry,
}

impl<'a, 'de> DeserializeSeed<'de> for SceneEntitiesDeserializer<'a> {
    type Value = Vec<DynamicEntity>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SceneEntitiesVisitor {
            type_registry: self.type_registry,
        })
    }
}

struct SceneEntitiesVisitor<'a> {
    pub type_registry: &'a TypeRegistry,
}

impl<'a, 'de> Visitor<'de> for SceneEntitiesVisitor<'a> {
    type Value = Vec<DynamicEntity>;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("map of entities")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entities = Vec::new();
        while let Some(entity) = map.next_key::<Entity>()? {
            let entity = map.next_value_seed(SceneEntityDeserializer {
                entity,
                type_registry: self.type_registry,
            })?;
            entities.push(entity);
        }

        Ok(entities)
    }
}

pub struct SceneEntityDeserializer<'a> {
    pub entity: Entity,
    pub type_registry: &'a TypeRegistry,
}

impl<'a, 'de> DeserializeSeed<'de> for SceneEntityDeserializer<'a> {
    type Value = DynamicEntity;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            ENTITY_STRUCT,
            &[ENTITY_FIELD_COMPONENTS],
            SceneEntityVisitor {
                entity: self.entity,
                registry: self.type_registry,
            },
        )
    }
}

struct SceneEntityVisitor<'a> {
    pub entity: Entity,
    pub registry: &'a TypeRegistry,
}

impl<'a, 'de> Visitor<'de> for SceneEntityVisitor<'a> {
    type Value = DynamicEntity;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("entities")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let components = seq
            .next_element_seed(SceneMapDeserializer {
                registry: self.registry,
            })?
            .ok_or_else(|| Error::missing_field(ENTITY_FIELD_COMPONENTS))?;

        Ok(DynamicEntity {
            entity: self.entity,
            components,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut components = None;
        while let Some(key) = map.next_key()? {
            match key {
                EntityField::Components => {
                    if components.is_some() {
                        return Err(Error::duplicate_field(ENTITY_FIELD_COMPONENTS));
                    }

                    components = Some(map.next_value_seed(SceneMapDeserializer {
                        registry: self.registry,
                    })?);
                }
            }
        }

        let components = components
            .take()
            .ok_or_else(|| Error::missing_field(ENTITY_FIELD_COMPONENTS))?;
        Ok(DynamicEntity {
            entity: self.entity,
            components,
        })
    }
}

pub struct SceneMapDeserializer<'a> {
    pub registry: &'a TypeRegistry,
}

impl<'a, 'de> DeserializeSeed<'de> for SceneMapDeserializer<'a> {
    type Value = Vec<Box<dyn PartialReflect>>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SceneMapVisitor {
            registry: self.registry,
        })
    }
}

struct SceneMapVisitor<'a> {
    pub registry: &'a TypeRegistry,
}

impl<'a, 'de> Visitor<'de> for SceneMapVisitor<'a> {
    type Value = Vec<Box<dyn PartialReflect>>;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("map of reflect types")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut dynamic_properties = Vec::new();
        while let Some(entity) = seq.next_element_seed(ReflectDeserializer::new(self.registry))? {
            dynamic_properties.push(entity);
        }

        Ok(dynamic_properties)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut added = <HashSet<_>>::default();
        let mut entries = Vec::new();
        while let Some(registration) =
            map.next_key_seed(TypeRegistrationDeserializer::new(self.registry))?
        {
            if !added.insert(registration.type_id()) {
                return Err(Error::custom(format_args!(
                    "duplicate reflect type: `{}`",
                    registration.type_info().type_path(),
                )));
            }

            let value =
                map.next_value_seed(TypedReflectDeserializer::new(registration, self.registry))?;

            let value = self
                .registry
                .get(registration.type_id())
                .and_then(|tr| tr.data::<ReflectFromReflect>())
                .and_then(|fr| fr.from_reflect(value.as_partial_reflect()))
                .map(PartialReflect::into_partial_reflect)
                .unwrap_or(value);

            entries.push(value);
        }

        Ok(entries)
    }
}
