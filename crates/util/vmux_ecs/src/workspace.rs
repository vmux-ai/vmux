use bevy_ecs::prelude::{Component, Event, ReflectComponent, SystemSet};
use bevy_reflect::Reflect;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_ecs::workspace"]
pub enum WorkspaceKind {
    #[default]
    Editor,
    Knowledge,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TabCommandSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StackCommandSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeFocusSet;

#[derive(Event, Clone, Copy)]
pub struct ProjectContextOpenRequest {
    pub force_new: bool,
    pub open: Option<vmux_api::open_target::OpenTarget>,
}
