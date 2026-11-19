extern crate self as vmux_feature;

#[cfg(feature = "manifest")]
pub use binding::{CommandBindingRegistration, FeaturePageBinding, PageBindingRegistration};

#[cfg(feature = "ecs")]
pub use runtime::{
    Changed, Current, Decided, Definition, DevelopmentOverride, EvaluationFailure, Failure,
    FeaturePlugin, Generation, InstallationScope, OverridePolicy, Provider, Receive, RefreshFailed,
    ReleaseProvider, Set, SnapshotAccepted, Tick,
};

#[cfg(all(feature = "page", host, ui))]
mod access;
pub mod administration;
#[cfg(feature = "manifest")]
mod binding;
#[cfg(feature = "cache")]
pub mod cache;
#[cfg(feature = "cache")]
mod cache_document;
#[cfg(feature = "cache")]
mod cache_io;
#[cfg(feature = "manifest")]
pub mod cli;
#[cfg(feature = "ecs")]
pub mod clock;
#[cfg(feature = "ecs")]
mod composition;
#[cfg(feature = "cache")]
pub mod experiment;
pub mod flag;
#[cfg(all(feature = "page", host, ui))]
mod host;
#[cfg(feature = "hosting")]
pub mod hosting;
#[cfg(feature = "http")]
pub mod http;
#[cfg(all(feature = "page", host, ui))]
mod inspection;
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
