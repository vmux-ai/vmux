use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bevy_app::{App, AppExit, Update};
use bevy_ecs::prelude::*;
use tokio::sync::oneshot;
use vmux_app::VmuxPlugin;
use vmux_app::ecs::{ShutdownPlugin, ShutdownRequest};
use vmux_app::quic::{
    Bytes, LengthPrefixed, QuicFrameReceived, QuicPeer, QuicPeerConnected, QuicPeerDisconnected,
    QuicRuntime, QuicServer, QuicServerBound, QuicServerPlugin, QuicServerSet, RateLimitPolicy,
    SelfSignedIdentity, Trust,
};
use vmux_app::runner::HeadlessTokioRunner;

#[derive(Resource)]
struct BoundChannel(Option<oneshot::Sender<SocketAddr>>);

#[derive(Resource, Default)]
struct PeerLog {
    connected: usize,
    disconnected: usize,
}

fn notify_bound(
    bound: Query<&QuicServerBound, Added<QuicServerBound>>,
    mut channel: ResMut<BoundChannel>,
) {
    for bound in &bound {
        if let Some(tx) = channel.0.take() {
            let _ = tx.send(bound.addr());
        }
    }
}

fn serve_frames(
    mut connected: MessageReader<QuicPeerConnected>,
    mut frames: MessageReader<QuicFrameReceived>,
    mut disconnected: MessageReader<QuicPeerDisconnected>,
    peers: Query<&QuicPeer>,
    mut log: ResMut<PeerLog>,
    mut shutdown: MessageWriter<ShutdownRequest>,
) {
    for _ in connected.read() {
        log.connected += 1;
    }
    for frame in frames.read() {
        let Ok(peer) = peers.get(frame.peer) else {
            continue;
        };
        if frame.payload.as_ref() == b"stop" {
            assert!(peer.send_frame_on(frame.stream_id, Bytes::from_static(b"bye")));
            shutdown.write(ShutdownRequest::success());
        } else {
            let mut body = b"ok:".to_vec();
            body.extend_from_slice(&frame.payload);
            assert!(peer.send_frame(Bytes::from(body)));
        }
    }
    for _ in disconnected.read() {
        log.disconnected += 1;
    }
}

#[test]
fn quic_server_plugin_composes_with_headless_runner_and_shutdown() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let identity = SelfSignedIdentity::generate(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
    let server_config = identity.server_config().unwrap();
    let (addr_tx, addr_rx) = oneshot::channel();

    let mut app = App::new();
    app.add_plugins((
        VmuxPlugin::builder().jobs(true).build(),
        ShutdownPlugin,
        QuicServerPlugin,
    ))
    .insert_resource(QuicRuntime::new(runtime.handle().clone()))
    .insert_resource(BoundChannel(Some(addr_tx)))
    .init_resource::<PeerLog>()
    .add_systems(Update, notify_bound.in_set(QuicServerSet::Receive))
    .add_systems(
        Update,
        serve_frames
            .after(QuicServerSet::Receive)
            .before(QuicServerSet::Reap),
    )
    .set_runner(
        HeadlessTokioRunner::new(runtime.handle().clone())
            .with_housekeeping_floor(Duration::from_secs(30))
            .into_runner(),
    );
    app.world_mut().spawn(
        QuicServer::bind("127.0.0.1:0".parse().unwrap(), server_config)
            .with_max_peers(8)
            .with_rate_limit(RateLimitPolicy::per_second(16, 16)),
    );

    let client_identity = identity.clone();
    let client = runtime.spawn(async move {
        let addr = addr_rx.await.unwrap();
        let endpoint = Trust::Desktop {
            fingerprint: client_identity.fingerprint.clone(),
        }
        .endpoint(addr)
        .unwrap();
        let conn = endpoint.connect(addr, "localhost").unwrap().await.unwrap();
        let codec = LengthPrefixed::new(1024 * 1024);
        let (mut send, mut recv) = conn.open_bi().await.unwrap();

        codec.write(&mut send, b"ping").await.unwrap();
        let ping = codec.read(&mut recv).await.unwrap().unwrap();
        codec.write(&mut send, b"stop").await.unwrap();
        let stop = codec.read(&mut recv).await.unwrap().unwrap();

        conn.close(0u32.into(), b"done");
        endpoint.wait_idle().await;
        (ping, stop)
    });

    let started = Instant::now();
    assert_eq!(app.run(), AppExit::Success);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "quic server should wake headless runner immediately without waiting on housekeeping floor"
    );
    let (ping, stop) = runtime.block_on(client).unwrap();
    assert_eq!(ping, b"ok:ping");
    assert_eq!(stop, b"bye");
}
