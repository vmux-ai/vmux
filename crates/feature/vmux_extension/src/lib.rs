#![allow(clippy::type_complexity)]

#[cfg(all(host, feature = "host"))]
pub use catalog::OpenManagerRequest;
#[cfg(all(host, feature = "host"))]
pub use host::ExtensionPlugin as ExtensionHostPlugin;
#[cfg(all(host, feature = "host"))]
pub use install::{ExtensionInstallCompleted, ExtensionInstallRequest};

pub use core::CorePlugin as ExtensionPlugin;

pub use core::{
    Association, AutoSave, Catalog, Changed, CleanupCompleted, CleanupRequest, CorePlugin, Enabled,
    Evidence, ExtensionSchedule, ExtensionSet, Grants, HostPermissions, Icon, Id, InstallOwner,
    InstallRequest, Installed, LoadCompleted, LoadRequest, Loaded, Mutation,
    OptionalHostPermissions, OptionalPermissions, Package, Permissions, Pinned, Popup,
    PreparedEvidence, ProfileId, PublicKey, Request, Root, Seed, SourceHash, State, Ticket,
    Version, WriteCompleted, WriteError, WriteTaskRequest,
};

#[cfg(all(host, feature = "host"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "host"))]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(all(host, feature = "host"))]
mod catalog;
#[cfg(all(host, feature = "host"))]
pub mod crx;
#[cfg(all(host, feature = "host"))]
mod download;
#[cfg(all(host, feature = "host"))]
pub mod driver;
#[cfg(all(host, feature = "host"))]
mod host;
#[cfg(all(host, feature = "host"))]
mod install;
#[cfg(all(host, feature = "host"))]
pub mod match_pattern;
#[cfg(all(host, feature = "host"))]
pub mod protocol;
#[cfg(all(host, feature = "host"))]
pub mod store;
#[cfg(all(host, feature = "host"))]
pub mod webstore;

pub mod snapshot;

mod core;

#[cfg(feature = "filesystem")]
pub mod filesystem;
