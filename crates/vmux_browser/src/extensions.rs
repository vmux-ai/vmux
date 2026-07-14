pub(crate) mod bridge;
pub(crate) mod bridge_page;
pub(crate) mod broker;
mod capability;
mod download;
mod install;
pub mod load;
mod manager_page;
mod runtime;
mod shim;

pub use manager_page::ExtensionsPlugin;
