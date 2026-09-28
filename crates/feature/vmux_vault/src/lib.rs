#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
mod host;
#[cfg(ui)]
mod ui;

#[cfg(host)]
pub use host::VaultPlugin;

pub const VAULT_PAGE_URL: &str = "vmux://vault/";
