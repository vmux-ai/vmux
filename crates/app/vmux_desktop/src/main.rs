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

    let _lease = match vmux_service::RuntimeHandoff::acquire() {
        Ok(lease) => lease,
        Err(error) => {
            eprintln!("Cannot open Vmux: {error}");
            std::process::exit(1);
        }
    };
    let mut app = App::new();
    app.add_plugins(VmuxPlugin);
    run_update_on_one_thread(&mut app);

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
