use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy_ecs::prelude::*;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vmux_app::ecs::{Shutdown, ShutdownBarrier, ShutdownComplete, ShutdownSet};
use vmux_app::http::{
    HttpRequest, HttpResponder, HttpResponse, HttpRuntime, HttpServer, HttpServerBound,
    HttpServerSet, Method, StatusCode,
};
use vmux_app::prelude::*;
use vmux_app::runner::HeadlessTokioRunner;

#[derive(Resource, Default)]
struct ServiceMetrics {
    health_checks: u64,
    drained: bool,
}

#[derive(Component)]
struct DrainBarrier;

#[derive(Serialize)]
struct HealthPayload {
    status: &'static str,
    health_checks: u64,
}

#[derive(Resource, Clone)]
struct BoundSlot(Arc<Mutex<Option<SocketAddr>>>);

#[derive(Resource, Clone)]
struct DrainedFlag(Arc<Mutex<bool>>);

struct DaemonServicePlugin {
    bind_addr: SocketAddr,
    runtime: tokio::runtime::Handle,
    bound: BoundSlot,
    drained: DrainedFlag,
}

impl Plugin for DaemonServicePlugin {
    fn build(&self, app: &mut App) {
        let bind_addr = self.bind_addr;
        app.add_plugins(ServerPlugins)
            .insert_resource(HttpRuntime::new(self.runtime.clone()))
            .insert_resource(self.bound.clone())
            .insert_resource(self.drained.clone())
            .init_resource::<ServiceMetrics>()
            .add_systems(Startup, move |mut commands: Commands| {
                commands.spawn(HttpServer::bind(bind_addr));
            })
            .add_systems(
                Update,
                (record_bound, serve_health)
                    .chain()
                    .in_set(HttpServerSet::Serve),
            )
            .add_systems(Shutdown, begin_drain.in_set(ShutdownSet::Drain))
            .add_systems(Update, finish_drain)
            .add_systems(
                ShutdownComplete,
                mark_complete.in_set(ShutdownSet::Complete),
            );
    }
}

fn record_bound(servers: Query<&HttpServerBound, Added<HttpServerBound>>, slot: Res<BoundSlot>) {
    for bound in &servers {
        if let Ok(mut guard) = slot.0.lock() {
            *guard = Some(bound.addr());
        }
    }
}

fn serve_health(
    mut metrics: ResMut<ServiceMetrics>,
    mut requests: Query<(&HttpRequest, &mut HttpResponder)>,
) {
    for (request, mut responder) in &mut requests {
        if responder.is_responded() {
            continue;
        }
        let response = match (request.method(), request.path()) {
            (&Method::GET, "/healthz") => {
                metrics.health_checks += 1;
                HttpResponse::json(
                    StatusCode::OK,
                    &HealthPayload {
                        status: "ok",
                        health_checks: metrics.health_checks,
                    },
                )
                .unwrap_or_else(|error| HttpResponse::internal_error(error.to_string()))
            }
            _ => HttpResponse::not_found(),
        };
        responder.respond(response);
    }
}

fn begin_drain(mut commands: Commands) {
    commands.spawn((DrainBarrier, ShutdownBarrier, Name::new("flush-metrics")));
}

fn finish_drain(
    barriers: Query<Entity, With<DrainBarrier>>,
    mut metrics: ResMut<ServiceMetrics>,
    mut commands: Commands,
) {
    for entity in &barriers {
        metrics.drained = true;
        commands.entity(entity).despawn();
    }
}

fn mark_complete(metrics: Res<ServiceMetrics>, flag: Res<DrainedFlag>) {
    if let Ok(mut guard) = flag.0.lock() {
        *guard = metrics.drained;
    }
}

async fn http_get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let raw = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    String::from_utf8(bytes).unwrap()
}

fn main() {
    let headless = std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some();
    let bind_addr: SocketAddr = if headless {
        "127.0.0.1:0".parse().unwrap()
    } else {
        std::env::var("VMUX_SERVICE_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:4040".to_string())
            .parse()
            .unwrap()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let bound = BoundSlot(Arc::new(Mutex::new(None)));
    let drained = DrainedFlag(Arc::new(Mutex::new(false)));

    let mut app = App::new();
    app.add_plugins(DaemonServicePlugin {
        bind_addr,
        runtime: runtime.handle().clone(),
        bound: bound.clone(),
        drained: drained.clone(),
    });

    if headless {
        runtime.block_on(async move {
            let (handle, runner) = HeadlessTokioRunner::handles(tokio::runtime::Handle::current());
            let task = tokio::spawn(runner.into_future(app));

            let mut actual_addr = None;
            for _ in 0..100 {
                if let Some(addr) = *bound.0.lock().unwrap() {
                    actual_addr = Some(addr);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let addr = actual_addr.expect("service did not bind");

            let body = http_get(addr, "/healthz").await;
            assert!(body.contains("200 OK"));
            assert!(body.contains("\"status\":\"ok\""));

            handle.shutdown();
            let exit = task.await.unwrap();
            assert_eq!(exit, AppExit::Success);
            assert!(*drained.0.lock().unwrap());
            println!("service daemon smoke passed on http://{addr} (graceful drain verified)");
        });
        return;
    }

    println!("Vmux Service Daemon listening on http://{bind_addr}/healthz");
    let runner = HeadlessTokioRunner::new(runtime.handle().clone()).with_os_signals();
    app.set_runner(runner.into_runner()).run();
}
