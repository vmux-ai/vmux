use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileLabel;
