use std::borrow::Borrow;
use std::marker::PhantomData;

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::prelude::Component, bevy_reflect::Reflect)
)]
#[cfg_attr(feature = "bevy", component(immutable))]
#[cfg_attr(feature = "bevy", reflect(Component, no_field_bounds))]
#[cfg_attr(feature = "bevy", type_path = "vmux_api")]
#[serde(transparent)]
#[repr(transparent)]
pub struct Id<T> {
    value: String,
    #[serde(skip)]
    #[cfg_attr(feature = "bevy", reflect(ignore))]
    #[rkyv(omit_bounds)]
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        Self::new(self.value.clone())
    }
}

impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Id").field(&self.value).finish()
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.value, state);
    }
}

impl<T> Id<T> {
    pub fn new(value: impl Into<String>) -> Self {
        Self::try_new(value).expect("ID must not be empty")
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("ID must not be empty".to_string());
        }
        Ok(Self {
            value,
            marker: PhantomData,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn into_string(self) -> String {
        self.value
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl<T> AsRef<str> for Id<T> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T> Borrow<str> for Id<T> {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<T> From<String> for Id<T> {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl<T> From<&str> for Id<T> {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl<T> std::fmt::Display for Id<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Issue {}

    #[test]
    fn serializes_as_its_string_value() {
        let id = Id::<Issue>::new("VMX-233");

        assert_eq!(serde_json::to_string(&id).unwrap(), "\"VMX-233\"");
        assert_eq!(
            serde_json::from_str::<Id<Issue>>("\"VMX-233\"").unwrap(),
            id
        );
    }

    #[test]
    fn survives_rkyv_round_trip() {
        let id = Id::<Issue>::new("VMX-233");
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&id).unwrap();
        let decoded = rkyv::from_bytes::<Id<Issue>, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded, id);
    }

    #[test]
    fn rejects_empty_construction_and_deserialization() {
        assert!(Id::<Issue>::try_new(" \n").is_err());
        assert!(serde_json::from_str::<Id<Issue>>("\"\"").is_err());
    }

    #[cfg(feature = "bevy")]
    #[test]
    fn is_an_immutable_component() {
        fn immutable<
            T: bevy_ecs::component::Component<Mutability = bevy_ecs::component::Immutable>,
        >() {
        }

        immutable::<Id<Issue>>();
    }
}
