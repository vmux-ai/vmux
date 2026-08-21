use bevy::prelude::*;
#[cfg(not(target_os = "macos"))]
use bevy_cef::prelude::early_exit_if_subprocess;
use vmux_desktop::VmuxPlugin;

fn main() {
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

    vmux_core::profile::migrate_legacy_personal_layout();

    let mut app = App::new();
    app.add_plugins(VmuxPlugin);
    run_update_on_one_thread(&mut app);

    // Override Bevy's Ctrl+C handler with a synchronous signal handler.
    // Bevy's handler fires asynchronously via a pipe, giving macOS AppKit
    // time to call applicationWillTerminate: which panics inside winit's
    // re-entrant event handler. A raw sigaction handler runs synchronously
    // on the interrupted thread, calling _exit before AppKit can react.
    unsafe {
        libc::signal(
            libc::SIGINT,
            sigint_handler as *const () as libc::sighandler_t,
        );
    }

    app.run();
}

/// Run `Update` on the calling thread rather than across the task pool.
///
/// Bevy's multi-threaded executor pays a fixed cost per turn — spawning a task per system, a scope
/// to join them, and the locks that coordinate it — and this app has nothing to spend it on. Under
/// a keystroke, sampling put roughly 1% of the process in vmux's own systems and the rest in that
/// coordination: ~1250 samples waiting on mutexes and ~290 in the executor itself.
///
/// A browser shell is not a game. Its systems are short, and what makes typing feel expensive is
/// how often a turn runs, not how much work one turn holds — so paying to spread that work over
/// eight threads buys nothing and costs the handshake every time.
fn run_update_on_one_thread(app: &mut App) {
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
}

extern "C" fn sigint_handler(_: libc::c_int) {
    unsafe {
        let msg = b"\nShutting down...\n";
        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
        libc::_exit(0);
    }
}
