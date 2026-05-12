use crate::{identity_path, pid_path, service_dir, socket_path, write_service_identity};
use tracing_subscriber::{EnvFilter, fmt};

/// Daemon entry point. Initializes logging, writes pid/identity, binds the socket,
/// installs SIGTERM/SIGINT handlers, and runs the IPC server until shutdown.
pub fn run() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(run_async());
}

async fn run_async() {
    let dir = service_dir();
    std::fs::create_dir_all(&dir).expect("failed to create service dir");

    init_tracing();

    let pid = std::process::id();
    std::fs::write(pid_path(), pid.to_string()).expect("failed to write PID file");
    write_service_identity().expect("failed to write service identity file");

    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).expect("failed to bind Unix socket");

    tracing::info!(
        target: "vmux_service::startup",
        version = env!("CARGO_PKG_VERSION"),
        profile = crate::current_profile(),
        pid = pid,
        socket = %sock.display(),
        "vmux_service started"
    );

    let sock_cleanup = sock.clone();
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        tracing::info!("shutdown signal received, cleaning up");
        let _ = std::fs::remove_file(&sock_cleanup);
        let _ = std::fs::remove_file(pid_path());
        let _ = std::fs::remove_file(identity_path());
        std::process::exit(0);
    });

    crate::server::run_server(listener).await;
}

fn init_tracing() {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("vmux-{}", crate::current_profile()))
        .filename_suffix("log")
        .max_log_files(7)
        .build(service_dir())
        .expect("build rolling log appender");

    let (writer, guard) = tracing_appender::non_blocking(appender);
    Box::leak(Box::new(guard));

    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_env("VMUX_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_target(false)
        .try_init();
}
