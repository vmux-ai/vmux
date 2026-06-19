pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
pub mod agent;
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_broker;
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_events;
#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod cleanup;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod framing;
#[cfg(not(target_arch = "wasm32"))]
pub mod http;
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod launchd;
#[cfg(not(target_arch = "wasm32"))]
pub mod message;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod process;
#[cfg(not(target_arch = "wasm32"))]
pub mod protocol;
#[cfg(not(target_arch = "wasm32"))]
pub mod providers;
#[cfg(not(target_arch = "wasm32"))]
pub mod registry;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod sm_app_service;
#[cfg(not(target_arch = "wasm32"))]
pub mod stream;
#[cfg(not(target_arch = "wasm32"))]
pub mod supervisor;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "services",
    title: "Services",
    keywords: &["processes", "monitor"],
    icon: "activity",
    command_bar: true,
};

#[cfg(not(target_arch = "wasm32"))]
mod paths;
#[cfg(not(target_arch = "wasm32"))]
pub use paths::*;
