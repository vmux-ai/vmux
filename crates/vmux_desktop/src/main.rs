use bevy::prelude::*;
#[cfg(not(target_os = "macos"))]
use bevy_cef::prelude::early_exit_if_subprocess;
use vmux_desktop::VmuxPlugin;

fn main() {
    // Check for `service` subcommand before any GUI/Bevy initialization.
    if std::env::args().nth(1).as_deref() == Some("service") {
        run_service();
        return;
    }

    #[cfg(not(target_os = "macos"))]
    early_exit_if_subprocess();

    // Fix up the macOS keychain ACL on the Chromium safe-storage item so
    // future signed builds (manual install, brew upgrade, auto-update)
    // inherit access without prompting the user. No-op on non-macOS, and
    // no-op the very first run when the item does not yet exist.
    #[cfg(target_os = "macos")]
    vmux_desktop::keychain::ensure_chromium_safe_storage_acl();

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
            "dev" => format!(" dev ({})", env!("VMUX_GIT_HASH")),
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

/// Run the vmux service (persistent terminal process manager).
/// Invoked via `vmux service` or `Vmux service`.
fn run_service() {
    use vmux_service::{pid_path, service_dir, socket_path};

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(async {
        let dir = service_dir();
        std::fs::create_dir_all(&dir).expect("failed to create service dir");

        let pid = std::process::id();
        std::fs::write(pid_path(), pid.to_string()).expect("failed to write PID file");

        let sock = socket_path();
        let _ = std::fs::remove_file(&sock);

        let listener = tokio::net::UnixListener::bind(&sock).expect("failed to bind Unix socket");

        eprintln!("vmux-service: listening on {}", sock.display());

        let sock_cleanup = sock.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            let _ = std::fs::remove_file(&sock_cleanup);
            let _ = std::fs::remove_file(pid_path());
            std::process::exit(0);
        });

        vmux_service::server::run_server(listener).await;
    });
}
