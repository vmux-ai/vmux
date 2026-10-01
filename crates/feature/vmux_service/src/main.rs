use bevy_app::prelude::*;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, fmt};
use vmux_core::service::ServicePaths;
use vmux_service::DaemonBinary;
use vmux_service::runner::WakeDrivenRunner;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let (wake_tx, wake_rx) = mpsc::unbounded_channel();
    let (signal_tx, signal_rx) = mpsc::channel(1);
    let listener = runtime.block_on(DaemonBootstrap::start(signal_tx.clone()));
    let handle = runtime.handle().clone();
    let daemon = vmux_service::server::ServiceDaemonPlugin::runtime(
        listener,
        wake_tx,
        handle.clone(),
        signal_tx,
    );

    let mut app = App::new();
    app.add_plugins(vmux_service::server::ServiceDaemonPlugin)
        .set_runner(WakeDrivenRunner::new(handle, wake_rx, signal_rx).into_runner());
    app.world_mut().spawn(daemon);
    app.run();
}

struct DaemonBootstrap;

impl DaemonBootstrap {
    async fn start(signal_tx: mpsc::Sender<()>) -> tokio::net::UnixListener {
        let paths = ServicePaths::current();
        let dir = ServicePaths::dir();
        std::fs::create_dir_all(&dir).expect("failed to create service dir");

        DaemonTracing::init();

        let pid = std::process::id();
        std::fs::write(paths.pid(), pid.to_string()).expect("failed to write PID file");
        DaemonBinary::current()
            .and_then(|daemon| daemon.record_identity())
            .expect("failed to write service identity file");

        let socket = paths.socket();
        let _ = std::fs::remove_file(&socket);
        let listener = tokio::net::UnixListener::bind(&socket).expect("failed to bind Unix socket");

        tracing::info!(
            target: "vmux_service::startup",
            version = env!("CARGO_PKG_VERSION"),
            profile = ServicePaths::build_profile(),
            pid,
            socket = %socket.display(),
            "vmux_service started"
        );

        let cleanup_socket = socket.clone();
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            tracing::info!("shutdown signal received, cleaning up");
            let _ = std::fs::remove_file(&cleanup_socket);
            let _ = std::fs::remove_file(paths.pid());
            let _ = std::fs::remove_file(paths.identity());
            let _ = signal_tx.send(()).await;
        });

        listener
    }
}

struct DaemonTracing;

impl DaemonTracing {
    fn init() {
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
}
