//! vmux_desktop binary entrypoint.

use bevy::prelude::*;

use vmux_desktop::VmuxPlugin;

fn main() {
    #[cfg(not(target_os = "macos"))]
    bevy_cef::prelude::early_exit_if_subprocess();

    App::new().add_plugins(VmuxPlugin).run();
}
