pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

#[cfg(not(target_arch = "wasm32"))]
use crate::vibe::setup::event::VibeInstallRunRequest;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant"],
    icon: "sparkles",
    command_bar: false,
};

#[cfg(not(target_arch = "wasm32"))]
pub const VIBE_SETUP_URL: &str = "vmux://agent/vibe/setup";

#[cfg(not(target_arch = "wasm32"))]
pub const VIBE_INSTALL_COMMAND: &str = "curl -LsSf https://mistral.ai/vibe/install.sh | bash";

#[cfg(not(target_arch = "wasm32"))]
pub struct VibeSetupPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for VibeSetupPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(VibeInstallRunRequest,)>::for_hosts(&["agent"]))
            .add_observer(on_vibe_install_run);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_vibe_install_run(
    _trigger: On<BinReceive<VibeInstallRunRequest>>,
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
) {
    run.write(vmux_terminal::RunShellRequest {
        command: VIBE_INSTALL_COMMAND.to_string(),
        cwd: String::new(),
        mode: vmux_terminal::ShellMode::NewTab,
    });
}
