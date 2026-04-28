use vmux_daemon::{daemon_dir, pid_path, socket_path};

#[tokio::main]
async fn main() {
    let dir = daemon_dir();
    std::fs::create_dir_all(&dir).expect("failed to create daemon dir");

    // Write PID file
    let pid = std::process::id();
    std::fs::write(pid_path(), pid.to_string()).expect("failed to write PID file");

    // Remove stale socket
    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);

    let listener =
        tokio::net::UnixListener::bind(&sock).expect("failed to bind Unix socket");

    eprintln!("vmux-daemon: listening on {}", sock.display());

    // Clean up on shutdown
    let sock_cleanup = sock.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = std::fs::remove_file(&sock_cleanup);
        let _ = std::fs::remove_file(pid_path());
        std::process::exit(0);
    });

    vmux_daemon::server::run_server(listener).await;
}
