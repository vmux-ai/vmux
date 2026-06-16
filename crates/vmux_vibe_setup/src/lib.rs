pub mod event;
#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant"],
    icon: "sparkles",
    command_bar: true,
};

#[cfg(not(target_arch = "wasm32"))]
pub const VIBE_SETUP_URL: &str = "vmux://agent/vibe/setup";

#[cfg(not(target_arch = "wasm32"))]
pub const VIBE_INSTALL_COMMAND: &str = "curl -LsSf https://mistral.ai/vibe/install.sh | bash";

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
