use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bevy_app::{App, AppExit, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{ShutdownPlugin, ShutdownRequest};
use vmux_app::http::{
    HttpRequest, HttpResponder, HttpResponse, HttpRuntime, HttpServer, HttpServerBound,
    HttpServerPlugin, HttpServerSet, Method, StatusCode,
};
use vmux_app::runner::HeadlessTokioRunner;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct HealthBody {
    status: String,
}

#[derive(Resource)]
struct BoundChannel(Option<oneshot::Sender<SocketAddr>>);

fn notify_bound(
    bound: Query<&HttpServerBound, Added<HttpServerBound>>,
    mut channel: ResMut<BoundChannel>,
) {
    for bound in &bound {
        if let Some(tx) = channel.0.take() {
            let _ = tx.send(bound.addr());
        }
    }
}

fn serve_routes(
    mut requests: Query<(&HttpRequest, &mut HttpResponder)>,
    mut shutdown: MessageWriter<ShutdownRequest>,
) {
    for (request, mut responder) in &mut requests {
        match (request.method(), request.path()) {
            (&Method::GET, "/health") => {
                let response = HttpResponse::json(
                    StatusCode::OK,
                    &HealthBody {
                        status: "ok".into(),
                    },
                )
                .unwrap();
                responder.respond(response);
            }
            (&Method::POST, "/stop") => {
                let body = request.body_str().unwrap_or("").to_string();
                let response = HttpResponse::text(StatusCode::OK, format!("bye:{body}"))
                    .with_header("x-consumer", "vmux");
                responder.respond(response);
                shutdown.write(ShutdownRequest::success());
            }
            _ => {
                responder.respond(HttpResponse::not_found());
            }
        }
    }
}

async fn send_http(addr: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn http_server_plugin_composes_with_headless_runner_and_shutdown() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (addr_tx, addr_rx) = oneshot::channel();

    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().jobs(true).build(),
        ShutdownPlugin,
        HttpServerPlugin,
    ))
    .insert_resource(HttpRuntime::new(runtime.handle().clone()))
    .insert_resource(BoundChannel(Some(addr_tx)))
    .add_systems(
        Update,
        (notify_bound, serve_routes).in_set(HttpServerSet::Serve),
    )
    .set_runner(
        HeadlessTokioRunner::new(runtime.handle().clone())
            .with_housekeeping_floor(Duration::from_secs(30))
            .into_runner(),
    );
    app.world_mut()
        .spawn(HttpServer::bind("127.0.0.1:0".parse().unwrap()));

    let client = runtime.spawn(async move {
        let addr = addr_rx.await.unwrap();
        let health = send_http(
            addr,
            "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        let stop = send_http(
            addr,
            "POST /stop HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nConnection: close\r\n\r\ndone",
        )
        .await;
        (addr, health, stop)
    });

    let started = Instant::now();
    assert_eq!(app.run(), AppExit::Success);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "http server should wake headless runner immediately without waiting on housekeeping floor"
    );
    let (addr, health, stop) = runtime.block_on(client).unwrap();
    assert!(health.starts_with("HTTP/1.1 200 OK\r\n"), "{health}");
    assert!(health.ends_with(r#"{"status":"ok"}"#), "{health}");
    assert!(stop.starts_with("HTTP/1.1 200 OK\r\n"), "{stop}");
    assert!(stop.contains("x-consumer: vmux\r\n"), "{stop}");
    assert!(stop.ends_with("\r\n\r\nbye:done"), "{stop}");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_err() {
            break;
        }
        assert!(Instant::now() < deadline, "server port stayed open");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn run_async_and_hardened_http_server_work_inside_tokio_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let handle = tokio::runtime::Handle::current();
        let (addr_tx, addr_rx) = oneshot::channel();
        let mut runner = HeadlessTokioRunner::new(handle.clone())
            .with_housekeeping_floor(Duration::from_secs(30));

        let mut app = App::new();
        app.add_plugins((
            VmuxPlugin::builder().jobs(true).build(),
            ShutdownPlugin,
            HttpServerPlugin,
        ))
        .insert_resource(HttpRuntime::new(handle))
        .insert_resource(BoundChannel(Some(addr_tx)))
        .add_systems(
            Update,
            (notify_bound, serve_routes).in_set(HttpServerSet::Serve),
        );
        app.world_mut().spawn(
            HttpServer::bind("127.0.0.1:0".parse().unwrap())
                .with_max_connections(4)
                .with_request_timeout(Duration::from_millis(150))
                .with_header_read_timeout(Duration::from_millis(150)),
        );

        let client = tokio::spawn(async move {
            let addr = addr_rx.await.unwrap();
            let health = send_http(
                addr,
                "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await;

            let mut stalled_body = TcpStream::connect(addr).await.unwrap();
            stalled_body
                .write_all(
                    b"POST /stop HTTP/1.1\r\nHost: localhost\r\nContent-Length: 64\r\nConnection: close\r\n\r\nhi",
                )
                .await
                .unwrap();
            stalled_body.flush().await.unwrap();
            let mut timeout_bytes = Vec::new();
            stalled_body.read_to_end(&mut timeout_bytes).await.unwrap();
            let timeout_response = String::from_utf8(timeout_bytes).unwrap();

            let stop = send_http(
                addr,
                "POST /stop HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            )
            .await;
            (addr, health, timeout_response, stop)
        });

        let started = Instant::now();
        assert_eq!(runner.run_async(&mut app).await, AppExit::Success);
        assert!(started.elapsed() < Duration::from_secs(5));
        let (addr, health, timeout_response, stop) = client.await.unwrap();
        assert!(health.starts_with("HTTP/1.1 200 OK\r\n"), "{health}");
        assert!(
            timeout_response.starts_with("HTTP/1.1 408 Request Timeout\r\n"),
            "{timeout_response}"
        );
        assert!(stop.starts_with("HTTP/1.1 200 OK\r\n"), "{stop}");
        assert!(stop.ends_with("\r\n\r\nbye:ok"), "{stop}");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_err() {
                break;
            }
            assert!(Instant::now() < deadline, "server port stayed open");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
}
