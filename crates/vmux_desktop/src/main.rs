use bevy::prelude::*;
use vmux_desktop::VmuxPlugin;

fn main() {
    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    App::new().add_plugins(VmuxPlugin).run();
}
