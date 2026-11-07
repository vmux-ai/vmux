use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use moonshine_save::prelude::Save;
use serde::{Deserialize, Serialize};

#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize,
)]
#[reflect(Component)]
#[require(Save)]
#[type_path = "vmux_acp"]
pub struct AcpSessionId(pub String);
