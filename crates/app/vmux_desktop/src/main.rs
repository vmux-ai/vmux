use bevy::prelude::*;
#[cfg(not(target_os = "macos"))]
use bevy_cef::prelude::early_exit_if_subprocess;
use vmux_desktop::VmuxPlugin;

use bevy::ecs::schedule::ScheduleLabel;

fn main() {
    #[cfg(target_os = "macos")]
    vmux_simulator::SimulatorPlugin::exit_helper_if_requested();

    vmux_desktop::panic_hook::install();

    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    println!(
        "\n\
         \x1b[36m \x1b[1m\\              /\x1b[0m\x1b[36m  |\\            /|  |        |  \\      /\x1b[0m\n\
         \x1b[36m  \x1b[1m\\            /\x1b[0m\x1b[36m   | \\          / |  |        |   \\    /\x1b[0m\n\
         \x1b[36m   \x1b[1m\\          /\x1b[0m\x1b[36m    |  \\        /  |  |        |    \\  /\x1b[0m\n\
         \x1b[36m    \x1b[1m\\        /\x1b[0m\x1b[36m     |   \\      /   |  |        |     \\/\x1b[0m\n\
         \x1b[36m     \x1b[1m\\      /\x1b[0m\x1b[36m      |    \\    /    |  |        |     /\\\x1b[0m\n\
         \x1b[36m      \x1b[1m\\    /\x1b[0m\x1b[36m       |     \\  /     |  |        |    /  \\\x1b[0m\n\
         \x1b[36m       \x1b[1m\\  /\x1b[0m\x1b[36m        |      \\/      |  |        |   /    \\\x1b[0m\n\
         \x1b[36m        \x1b[1m\\/\x1b[0m\x1b[36m         |              |  |________|  /      \\\x1b[0m\n\
         \n\
         \x1b[2mv{}{}\x1b[0m\n",
        env!("CARGO_PKG_VERSION"),
        match env!("VMUX_BUILD_PROFILE") {
            "release" => String::new(),
            "local" => format!(" ({})", env!("VMUX_GIT_HASH")),
            "dev" => format!(" dev ({})", env!("VMUX_GIT_HASH")),
            other => format!(" ({})", other),
        }
    );

    vmux_ecs::profile::ProfilePaths::current().migrate_legacy_personal_layout();

    let mut app = App::new();
    app.add_plugins(VmuxPlugin);
    run_update_on_one_thread(&mut app);

    unsafe {
        libc::signal(
            libc::SIGINT,
            sigint_handler as *const () as libc::sighandler_t,
        );
    }

    app.run();
}

fn run_update_on_one_thread(app: &mut App) {
    for label in [
        First.intern(),
        PreUpdate.intern(),
        Update.intern(),
        PostUpdate.intern(),
        Last.intern(),
    ] {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
    }
}

extern "C" fn sigint_handler(_: libc::c_int) {
    unsafe {
        let msg = b"\nShutting down...\n";
        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
        libc::_exit(0);
    }
}
