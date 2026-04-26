use bevy::prelude::*;
use vmux_desktop::VmuxPlugin;

fn main() {
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
        match env!("VMUX_PROFILE") {
            "release" => String::new(),
            "local" => format!(" ({})", env!("VMUX_GIT_HASH")),
            "dev" => " (dev)".to_string(),
            other => format!(" ({})", other),
        }
    );

    let mut app = App::new();
    app.add_plugins(VmuxPlugin);

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

extern "C" fn sigint_handler(_: libc::c_int) {
    unsafe {
        let msg = b"\nShutting down...\n";
        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
        libc::_exit(0);
    }
}
