#[cfg(host)]
use bevy::ecs::reflect::ReflectComponent;
#[cfg(host)]
use bevy::prelude::{Component, Reflect};

#[vmux_api::contract(Copy, Eq)]
#[cfg_attr(host, derive(Component, Reflect))]
#[cfg_attr(host, reflect(Component))]
#[cfg_attr(host, type_path = "vmux_core")]
#[serde(rename_all = "snake_case")]
pub enum SmartBookmarkFolder {
    Projects,
    Knowledge,
    Tools,
}
