#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;
#[cfg(feature = "bevy")]
use bevy_reflect::std_traits::ReflectDefault;
use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::prelude::Component, bevy_reflect::Reflect)
)]
#[cfg_attr(feature = "bevy", reflect(Component, Default))]
#[cfg_attr(feature = "bevy", type_path = "vmux_api")]
pub struct Project;

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::prelude::Component, bevy_reflect::Reflect)
)]
#[cfg_attr(feature = "bevy", component(immutable))]
#[cfg_attr(feature = "bevy", reflect(Component, no_field_bounds))]
#[cfg_attr(feature = "bevy", type_path = "vmux_api")]
#[serde(transparent)]
pub struct ProjectRef(Id<Project>);

impl ProjectRef {
    pub fn new(id: Id<Project>) -> Self {
        Self(id)
    }

    pub fn id(&self) -> &Id<Project> {
        &self.0
    }
}
