//! Embedded loopback HTTP servers (Axum [`ServeDir`]) for static web apps loaded in CEF webviews.
//!
//! Push [`EmbeddedServeDirRequest`] values into [`PendingEmbeddedServeDir`] from systems in
//! [`EmbeddedServeDirStartup::FillPending`]
//! (before [`VmuxServerPlugin`]’s [`spawn_embedded_serve_dir_system`] on [`Startup`]). Shutdown flags
//! are registered automatically; [`VmuxServerPlugin`] stops all servers on
//! [`AppExit`](bevy::app::AppExit).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use bevy::app::AppExit;
use bevy::prelude::*;
use crossbeam_channel::Sender;

fn embedded_http_runtime() -> Arc<tokio::runtime::Runtime> {
    static RT: OnceLock<Arc<tokio::runtime::Runtime>> = OnceLock::new();
    RT.get_or_init(|| {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .thread_name("vmux-embedded-http")
                // Status + history `ServeDir` tasks run concurrently; one pool avoids a second
                // multi-thread runtime + OS thread per server (was noticeably slowing startup).
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("vmux_server: tokio runtime"),
        )
    })
    .clone()
}

/// Registry of shutdown flags for embedded HTTP tasks.
#[derive(Resource, Default)]
pub struct VmuxServerShutdownRegistry(pub Vec<Arc<Mutex<bool>>>);

/// Register a shutdown flag so [`VmuxServerPlugin`] can signal graceful shutdown on app exit.
pub fn register_shutdown_flag(registry: &mut VmuxServerShutdownRegistry, flag: Arc<Mutex<bool>>) {
    registry.0.push(flag);
}

/// One embedded [`ServeDir`] request: insert into [`PendingEmbeddedServeDir`] before
/// [`spawn_embedded_serve_dir_system`].
pub struct EmbeddedServeDirRequest {
    pub root: PathBuf,
    pub tx: Sender<String>,
    pub shutdown: Arc<Mutex<bool>>,
}

/// Queued embedded servers; each entry is spawned on the next [`spawn_embedded_serve_dir_system`] run.
#[derive(Resource, Default)]
pub struct PendingEmbeddedServeDir(pub Vec<EmbeddedServeDirRequest>);

/// Startup ordering: fill [`PendingEmbeddedServeDir`], then [`spawn_embedded_serve_dir_system`],
/// then drain `tx` URLs so pane setup can use loopback bases immediately (same frame as startup).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum EmbeddedServeDirStartup {
    /// Systems that set [`PendingEmbeddedServeDir`] (e.g. status UI dist + channel).
    FillPending,
    /// Runs [`spawn_embedded_serve_dir_system`].
    SpawnEmbedded,
    /// Blocking/short-wait recv from embedded-server channels into `*UiBaseUrl` resources.
    DrainChannels,
}

/// Startup system: registers shutdown flags and spawns each [`ServeDir`] on the shared Tokio runtime.
pub fn spawn_embedded_serve_dir_system(
    mut pending: ResMut<PendingEmbeddedServeDir>,
    mut registry: ResMut<VmuxServerShutdownRegistry>,
) {
    let batch = std::mem::take(&mut pending.0);
    if batch.is_empty() {
        return;
    }
    let rt = embedded_http_runtime();
    for inner in batch {
        registry.0.push(Arc::clone(&inner.shutdown));
        let EmbeddedServeDirRequest { root, tx, shutdown } = inner;
        rt.spawn(async move {
            run_embedded_serve_dir(root, tx, shutdown, Instant::now()).await;
        });
    }
}

async fn run_embedded_serve_dir(
    root: PathBuf,
    tx: Sender<String>,
    shutdown: Arc<Mutex<bool>>,
    spawned_at: Instant,
) {
    let root = if root.is_dir() {
        root
    } else {
        bevy::log::error!("vmux_server: serve dir missing: {}", root.display());
        return;
    };

    use axum::Router;
    use tower_http::services::ServeDir;

    let app = Router::new().nest_service("/", ServeDir::new(root));
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => {
            bevy::log::error!("vmux_server: bind: {e}");
            return;
        }
    };
    let port = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(e) => {
            bevy::log::error!("vmux_server: local_addr: {e}");
            return;
        }
    };
    let base = format!("http://127.0.0.1:{port}/");
    bevy::log::info!(
        "vmux_server: embedded HTTP listening at {} (task ready in {:?})",
        base,
        spawned_at.elapsed()
    );
    if tx.send(base).is_err() {
        return;
    }

    let shutdown = Arc::clone(&shutdown);
    let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
        loop {
            let stop = match shutdown.lock() {
                Ok(g) => *g,
                Err(e) => *e.into_inner(),
            };
            if stop {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    if let Err(e) = serve.await {
        bevy::log::error!("vmux_server: server error: {e}");
    }
}

fn shutdown_registered_servers(
    mut reader: MessageReader<AppExit>,
    registry: ResMut<VmuxServerShutdownRegistry>,
) {
    if reader.read().next().is_none() {
        return;
    }
    for f in &registry.0 {
        if let Ok(mut g) = f.lock() {
            *g = true;
        }
    }
}

/// Registers [`VmuxServerShutdownRegistry`] and stops all registered servers on [`AppExit`].
#[derive(Default)]
pub struct VmuxServerPlugin;

impl Plugin for VmuxServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingEmbeddedServeDir>()
            .init_resource::<VmuxServerShutdownRegistry>()
            .configure_sets(
                Startup,
                EmbeddedServeDirStartup::SpawnEmbedded.after(EmbeddedServeDirStartup::FillPending),
            )
            .configure_sets(
                Startup,
                EmbeddedServeDirStartup::DrainChannels.after(EmbeddedServeDirStartup::SpawnEmbedded),
            )
            .add_systems(
                Startup,
                spawn_embedded_serve_dir_system.in_set(EmbeddedServeDirStartup::SpawnEmbedded),
            )
            .add_systems(Last, shutdown_registered_servers);
    }
}
