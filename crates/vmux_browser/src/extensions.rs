pub(crate) mod bridge;
pub(crate) mod bridge_page;
pub(crate) mod broker;
mod capability;
mod download;
mod install;
pub mod load;
mod manager_page;
pub(crate) mod model;
mod runtime;
mod shim;
mod template;
pub(crate) mod windows;

pub use manager_page::ExtensionsPlugin;
