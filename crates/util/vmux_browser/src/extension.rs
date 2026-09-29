pub(crate) mod bridge;
pub(crate) mod bridge_page;
pub(crate) mod broker;
mod capability;
pub mod load;
mod manager_page;
pub(crate) mod model;
pub(crate) mod project;
mod runtime;
mod service_worker_cache;
mod shim;
mod tabs;
mod template;
pub(crate) mod windows;

pub use manager_page::ExtensionBrowserPlugin;
pub(crate) use manager_page::{ExtensionPopup, ExtensionPopupBounds, ExtensionPopupPresented};

#[derive(bevy::prelude::SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum ExtensionSystemSet {
    DrainBridge,
    SyncWindows,
}
