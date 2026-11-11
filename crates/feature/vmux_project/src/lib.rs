use bevy_ecs::reflect::ReflectComponent;

pub use registration::{
    Archived, Error, Hidden, LoadCompleted, LoadError, LoadRequest, Loaded, Mutation, Project,
    ProjectChanged, ProjectPlugin, ProjectSaved, Record, Registered, Registrations, Repository,
    Root, Snapshot, StoreSet, Ticket, WriteCompleted, WriteRequest, WriteResult, WriteTaskRequest,
};
pub use route::ProjectRoute;
pub use vmux_ecs::Revision;

#[cfg(feature = "contracts")]
pub mod contract;
#[cfg(feature = "filesystem")]
pub mod filesystem;
mod registration;
mod route;

#[derive(
    bevy_ecs::prelude::Component,
    bevy_reflect::Reflect,
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(
    feature = "contracts",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[reflect(Component)]
pub struct ProjectId(pub String);
