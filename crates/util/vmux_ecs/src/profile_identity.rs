#[cfg(feature = "records")]
use crate::record::{Format, ReflectStored};
use bevy_ecs::prelude::*;
use bevy_ecs::reflect::ReflectComponent;
use bevy_reflect::{Reflect, ReflectDeserialize, ReflectSerialize};
use serde::{Deserialize, Serialize};

#[derive(Component, Reflect, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
#[cfg_attr(feature = "records", reflect(Stored, @Format::new("directory", 1)))]
pub struct ProfileId(pub String);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileLabel;
