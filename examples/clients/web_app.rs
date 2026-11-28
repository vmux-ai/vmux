use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vmux_app::http::{
    HttpRequest, HttpResponder, HttpResponse, HttpRuntime, HttpServer, HttpServerBound,
    HttpServerSet, Method, StatusCode,
};
use vmux_app::prelude::*;
use vmux_app::runner::HeadlessTokioRunner;

#[derive(Resource, Default)]
struct WebCounter {
    count: i64,
    requests: u64,
}

#[derive(Serialize)]
struct CounterJson {
    count: i64,
    requests: u64,
}

#[derive(Resource, Clone)]
struct BoundAddrSlot(Arc<Mutex<Option<SocketAddr>>>);

#[component]
fn WebAppPage(count: i64, requests: u64) -> Element {
    rsx! {
        main {
            h1 { "Vmux Framework Web App" }
            p { "Canonical ECS count: {count}" }
            p { "Total HTTP requests served: {requests}" }
            form { method: "post", action: "/inc", button { r#type: "submit", "+1" } }
            form { method: "post", action: "/dec", button { r#type: "submit", "-1" } }
        }
    }
}

struct WebAppPlugin {
    bind_addr: SocketAddr,
    runtime: tokio::runtime::Handle,
    bound: BoundAddrSlot,
}

impl Plugin for WebAppPlugin {
    fn build(&self, app: &mut App) {
        let bind_addr = self.bind_addr;
        app.add_plugins(ServerPlugins)
            .insert_resource(HttpRuntime::new(self.runtime.clone()))
            .insert_resource(self.bound.clone())
            .init_resource::<WebCounter>()
            .add_systems(Startup, move |mut commands: Commands| {
                commands.spawn(HttpServer::bind(bind_addr));
            })
            .add_systems(
                Update,
                (record_bound_addr, handle_http_requests)
                    .chain()
                    .in_set(HttpServerSet::Serve),
            );
    }
}

fn record_bound_addr(
    servers: Query<&HttpServerBound, Added<HttpServerBound>>,
    slot: Res<BoundAddrSlot>,
) {
    for bound in &servers {
        if let Ok(mut guard) = slot.0.lock() {
            *guard = Some(bound.addr());
        }
    }
}

fn handle_http_requests(
    mut counter: ResMut<WebCounter>,
    mut requests: Query<(&HttpRequest, &mut HttpResponder)>,
) {
    for (request, mut responder) in &mut requests {
        if responder.is_responded() {
            continue;
        }
        counter.requests += 1;
        let response = match (request.method(), request.path()) {
            (&Method::GET, "/") => render_html(counter.count, counter.requests),
            (&Method::POST, "/inc") => {
                counter.count += 1;
                render_html(counter.count, counter.requests)
            }
            (&Method::POST, "/dec") => {
                counter.count -= 1;
                render_html(counter.count, counter.requests)
            }
            (&Method::GET, "/api/state") => HttpResponse::json(
                StatusCode::OK,
                &CounterJson {
                    count: counter.count,
                    requests: counter.requests,
                },
            )
            .unwrap_or_else(|error| HttpResponse::internal_error(error.to_string())),
            _ => HttpResponse::not_found(),
        };
        responder.respond(response);
    }
}

fn render_html(count: i64, requests: u64) -> HttpResponse {
    let body = dioxus_ssr::render_element(rsx! {
        WebAppPage { count, requests }
    });
    HttpResponse::ok(format!(
        "<!doctype html><html><head><title>Vmux Web Client</title></head><body>{body}</body></html>"
    ))
    .with_header("content-type", "text/html; charset=utf-8")
}

async fn send_raw_http(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    String::from_utf8(bytes).unwrap()
}

fn main() {
    let headless = std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some();
    let bind_addr: SocketAddr = if headless {
        "127.0.0.1:0".parse().unwrap()
    } else {
        std::env::var("VMUX_WEB_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
            .parse()
            .unwrap()
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let bound = BoundAddrSlot(Arc::new(Mutex::new(None)));

    let mut app = App::new();
    app.add_plugins(WebAppPlugin {
        bind_addr,
        runtime: runtime.handle().clone(),
        bound: bound.clone(),
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
            let addr = actual_addr.expect("web server did not bind");

            let home = send_raw_http(
                addr,
                "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await;
            assert!(home.contains("200 OK"));
            assert!(home.contains("Vmux Framework Web App"));

            let inc = send_raw_http(
                addr,
                "POST /inc HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
            assert!(inc.contains("Canonical ECS count: 1"));

            let state = send_raw_http(
                addr,
                "GET /api/state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await;
            assert!(state.contains("\"count\":1"));

            handle.shutdown();
            let exit = task.await.unwrap();
            assert_eq!(exit, AppExit::Success);
            println!("web client smoke passed on http://{addr}");
        });
        return;
    }

    println!("Vmux Web Client listening on http://{bind_addr}");
    let runner = HeadlessTokioRunner::new(runtime.handle().clone()).with_os_signals();
    app.set_runner(runner.into_runner()).run();
}
