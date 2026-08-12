use bevy_app::prelude::*;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, fmt};

use crate::runner::{ServiceHostPlugin, wake_driven_runner};
use crate::{DaemonBinary, ServicePaths};

/// Daemon entry point. Initializes logging, writes pid/identity, binds the socket, installs
/// SIGTERM/SIGINT handlers, and drives the headless app until shutdown.
///
/// The Bevy runner owns the main thread and the IPC server runs as a Tokio task, rather than the
/// other way round: a runner is a loop that must be free to park, and `run_server` awaits until
/// the daemon is done.
pub fn run() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let (wake_tx, wake_rx) = mpsc::unbounded_channel();
    let (signal_tx, signal_rx) = mpsc::channel(1);

    let listener = rt.block_on(bootstrap(signal_tx));
    rt.spawn(crate::server::run_server(listener, wake_tx));

    let handle = rt.handle().clone();
    App::new()
        .add_plugins(ServiceHostPlugin {
            runtime: handle.clone(),
        })
        .set_runner(wake_driven_runner(handle, wake_rx, signal_rx))
        .run();
}

/// Everything that has to happen before the app can run, and the listener it produces.
async fn bootstrap(signal_tx: mpsc::Sender<()>) -> tokio::net::UnixListener {
    let paths = ServicePaths::current();
    let dir = ServicePaths::dir();
    std::fs::create_dir_all(&dir).expect("failed to create service dir");

    init_tracing();

    let pid = std::process::id();
    std::fs::write(paths.pid(), pid.to_string()).expect("failed to write PID file");
    DaemonBinary::current()
        .and_then(|daemon| daemon.record_identity())
        .expect("failed to write service identity file");

    let sock = paths.socket();
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).expect("failed to bind Unix socket");

    tracing::info!(
        target: "vmux_service::startup",
        version = env!("CARGO_PKG_VERSION"),
        profile = ServicePaths::build_profile(),
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
        let _ = std::fs::remove_file(paths.pid());
        let _ = std::fs::remove_file(paths.identity());
        // Ask the app to stop rather than calling process::exit, which would leave AppExit
        // unobserved and give later stages nowhere to hang shutdown work.
        let _ = signal_tx.send(()).await;
    });

    listener
}

fn init_tracing() {
    let dir = ServicePaths::log_dir();
    std::fs::create_dir_all(&dir).expect("failed to create log dir");
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(format!("vmux-{}", ServicePaths::build_profile()))
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
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
