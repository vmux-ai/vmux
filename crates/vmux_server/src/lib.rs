//! Embedded loopback HTTP servers (Axum [`ServeDir`]) for static web apps loaded in CEF webviews.
//!
//! Insert [`PendingEmbeddedServeDir`] in a system that runs in [`EmbeddedServeDirStartup::FillPending`]
//! (before [`VmuxServerPlugin`]’s [`spawn_embedded_serve_dir_system`] on [`Startup`]). Shutdown flags
//! are registered automatically; [`VmuxServerPlugin`] stops all servers on
//! [`AppExit`](bevy::app::AppExit).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::app::AppExit;
use bevy::prelude::*;
use crossbeam_channel::Sender;

/// Registry of shutdown flags for embedded HTTP threads.
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

/// Set [`PendingEmbeddedServeDir::0`](PendingEmbeddedServeDir) to [`Some`] to spawn a server on the next
/// [`spawn_embedded_serve_dir_system`] run.
#[derive(Resource, Default)]
pub struct PendingEmbeddedServeDir(pub Option<EmbeddedServeDirRequest>);

/// Startup ordering: fill [`PendingEmbeddedServeDir`], then [`spawn_embedded_serve_dir_system`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum EmbeddedServeDirStartup {
    /// Systems that set [`PendingEmbeddedServeDir`] (e.g. status UI dist + channel).
    FillPending,
    /// Runs [`spawn_embedded_serve_dir_system`].
    SpawnEmbedded,
}

/// Startup system: if [`PendingEmbeddedServeDir`] is [`Some`], registers shutdown and spawns the
/// background HTTP thread (then clears the pending slot).
pub fn spawn_embedded_serve_dir_system(
    mut pending: ResMut<PendingEmbeddedServeDir>,
    mut registry: ResMut<VmuxServerShutdownRegistry>,
) {
    let Some(inner) = pending.0.take() else {
        return;
    };
    registry.0.push(Arc::clone(&inner.shutdown));

    let EmbeddedServeDirRequest { root, tx, shutdown } = inner;
    let join = std::thread::Builder::new()
        .name("vmux-embedded-http".into())
        .spawn(move || run_embedded_serve_dir(root, tx, shutdown))
        .expect("vmux embedded http thread");
    std::mem::forget(join);
}

fn run_embedded_serve_dir(root: PathBuf, tx: Sender<String>, shutdown: Arc<Mutex<bool>>) {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            bevy::log::error!("vmux_server: tokio runtime: {e}");
            return;
        }
    };

    rt.block_on(async move {
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
    });
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
            .add_systems(
                Startup,
                spawn_embedded_serve_dir_system.in_set(EmbeddedServeDirStartup::SpawnEmbedded),
            )
            .add_systems(Last, shutdown_registered_servers);
    }
}
