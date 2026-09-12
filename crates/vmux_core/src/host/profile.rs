use bevy::prelude::*;

pub use vmux_profile::*;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ProfileId(pub String);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProfileLabel;
