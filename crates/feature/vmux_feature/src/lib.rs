#[cfg(feature = "ecs")]
pub use runtime::{
    Changed, Current, Definition, Failure, FeaturePlugin, Generation, Provider, Receive,
    RefreshFailed, Set, Tick,
};

pub mod administration;
#[cfg(feature = "manifest")]
mod binding;
#[cfg(feature = "manifest")]
pub mod cli;
#[cfg(feature = "ecs")]
pub mod clock;
#[cfg(feature = "ecs")]
mod composition;
pub mod flag;
#[cfg(all(feature = "page", host, ui))]
mod host;
#[cfg(feature = "hosting")]
pub mod hosting;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "manifest")]
pub mod manifest;
#[cfg(feature = "manifest")]
mod metadata;
#[cfg(all(feature = "page", host, ui))]
mod native;
#[cfg(feature = "manifest")]
mod page_binding;
#[cfg(feature = "page")]
pub mod registration;
#[cfg(feature = "ecs")]
mod registry;
#[cfg(feature = "ecs")]
mod runtime;
pub mod snapshot;
#[cfg(feature = "manifest")]
pub mod __private {
    pub use inventory;
}
#[cfg(feature = "page")]
pub mod state;
#[cfg(all(feature = "page", ui))]
mod ui;
extern crate self as vmux_feature;

#[cfg(feature = "manifest")]
pub use binding::{CommandBindingRegistration, FeaturePageBinding, PageBindingRegistration};
