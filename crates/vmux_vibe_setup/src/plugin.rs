use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

use crate::event::VibeInstallRunRequest;

pub struct VibeSetupPlugin;

impl Plugin for VibeSetupPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(VibeInstallRunRequest,)>::default())
            .add_observer(on_vibe_install_run);
    }
}

/// "Want me to run?" button → spawn a terminal in the focused pane running the Vibe install script.
fn on_vibe_install_run(
    _trigger: On<BinReceive<VibeInstallRunRequest>>,
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
) {
    run.write(vmux_terminal::RunShellRequest {
        command: crate::VIBE_INSTALL_COMMAND.to_string(),
        cwd: String::new(),
        mode: vmux_terminal::ShellMode::NewTab,
    });
}
