#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
#[cfg(host)]
pub(crate) type Feature = VaultToolPlugin;

#[cfg(host)]
mod agent;
#[cfg(host)]
mod host;
#[cfg(ui)]
mod ui;

#[cfg(host)]
pub use agent::VaultToolPlugin;
#[cfg(host)]
pub use host::VaultPlugin;

pub const VAULT_PAGE_URL: &str = "vmux://vault/";
