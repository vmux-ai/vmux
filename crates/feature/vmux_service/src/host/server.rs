use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use vmux_ecs::service::{AbortTask, Executor, Operations, Protocols, Register, Remote, Wake};
use vmux_process::ProcessRuntime;

pub use super::peer::TrustedDesktop;
use super::peer::{ClientPeer, DesktopPeer, PendingClient};
use super::server_driver::{Connection, ConnectionRuntime, Listener as ListenerDriver};
use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::client_operation::ClientOperations;
use vmux_transport::service_peer::LocalPeer;

pub struct ServiceDaemonPlugin;

#[derive(Component)]
pub struct Daemon;

#[derive(Component)]
pub struct SocketListener(pub Option<UnixListener>);

#[derive(Component)]
pub struct ExitSignal(pub mpsc::Sender<()>);

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
struct Launch;

#[derive(Component, Clone, Copy)]
struct StartedAt(Instant);

#[derive(Component)]
struct ConnectionInbox(mpsc::UnboundedReceiver<tokio::net::UnixStream>);

#[derive(Component, Clone)]
struct Shutdown(mpsc::Sender<()>);

#[derive(Component)]
pub(super) struct Client;

type DaemonRuntimeState<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Executor,
        &'static Wake,
        &'static ProcessRuntime,
        &'static Protocols,
        &'static Shutdown,
        &'static StartedAt,
    ),
    With<Daemon>,
>;

type DaemonState<'w, 's> = Single<
    'w,
    's,
    (
        Entity,
        &'static Executor,
        &'static Wake,
        &'static ProcessRuntime,
        &'static Protocols,
        &'static Shutdown,
        &'static StartedAt,
    ),
    With<Daemon>,
>;

#[derive(SystemParam)]
struct DaemonRuntime<'w, 's> {
    state: DaemonState<'w, 's>,
    inbox: Single<'w, 's, &'static mut ConnectionInbox, With<Daemon>>,
}

impl Plugin for ServiceDaemonPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_process::ProcessPlugin,
            vmux_process::ProcessServicePlugin,
            crate::remote::RemotePlugin,
            super::control::ControlPlugin,
        ))
        .configure_sets(Update, (Register, Launch).chain())
        .add_systems(Startup, prepare)
        .add_systems(
            Update,
            (
                start.in_set(Launch),
                ApplyDeferred,
                start_clients,
                dispatch_clients,
                reap_clients,
            )
                .chain(),
        );
    }
}

fn prepare(
    daemons: Query<(Entity, &Wake, Has<TrustedDesktop>), With<Daemon>>,
    mut commands: Commands,
) {
    for (entity, wake, configured) in &daemons {
        if !configured {
            commands.entity(entity).insert(TrustedDesktop::current());
        }
        let (processes, process_runtime) = ProcessRuntime::new(wake.0.clone());
        let (client_operations, client_operation_runtime) = ClientOperations::new(wake.0.clone());
        let (authorizations, authorization_runtime) = RemoteAuthorizations::new(wake.0.clone());
        commands.entity(entity).insert((
            Name::new("vmux service"),
            processes,
            Operations(Arc::new(client_operations)),
            Protocols::default(),
            Remote::default(),
            authorizations,
            process_runtime,
            client_operation_runtime,
            authorization_runtime,
        ));
    }
}

fn start(
    mut servers: Query<(Entity, &mut SocketListener, &ExitSignal, &Executor, &Wake)>,
    mut commands: Commands,
) {
    for (entity, mut listener, exit, executor, wake) in &mut servers {
        let Some(listener) = listener.0.take() else {
            continue;
        };
        let started_at = StartedAt(Instant::now());
        let (connections, connection_inbox) = mpsc::unbounded_channel();
        let (shutdown, shutdown_inbox) = mpsc::channel(1);
        let listener = ListenerDriver::new(
            listener,
            connections,
            wake.0.clone(),
            shutdown_inbox,
            exit.0.clone(),
        );
        let task = executor.0.spawn(listener.run());
        commands
            .entity(entity)
            .remove::<(SocketListener, ExitSignal)>()
            .insert((
                started_at,
                ConnectionInbox(connection_inbox),
                Shutdown(shutdown),
                AbortTask(task),
            ));
        let _ = wake.0.send(());
    }
}

fn start_clients(
    mut runtime: DaemonRuntime,
    policies: Query<&TrustedDesktop>,
    mut commands: Commands,
) {
    let (root, _, _, _, _, _, _) = *runtime.state;
    let policy = policies.get(root).ok();
    while let Ok(stream) = runtime.inbox.0.try_recv() {
        let requirement = policy.and_then(|policy| policy.requirement.as_ref());
        let peer = match LocalPeer::inspect(&stream, requirement) {
            Ok(peer) => peer,
            Err(error) => {
                tracing::warn!(%error, "local peer identity unavailable");
                continue;
            }
        };
        let admitted = policy.is_some_and(|policy| policy.admits(&peer));
        let mut client = commands.spawn((
            Name::new("service client"),
            Client,
            ChildOf(root),
            ClientPeer(peer),
            PendingClient(Some(stream)),
        ));
        if admitted {
            client.insert(DesktopPeer);
        }
    }
}

fn dispatch_clients(
    mut clients: Query<(
        Entity,
        &ChildOf,
        &ClientPeer,
        &mut PendingClient,
        Has<DesktopPeer>,
    )>,
    runtimes: DaemonRuntimeState,
    observations: Query<&crate::remote::server::ObservationSource>,
    mut commands: Commands,
) {
    for (entity, parent, peer, mut pending, admitted) in &mut clients {
        let Ok((_, executor, wake, processes, protocols, shutdown, started_at)) =
            runtimes.get(parent.parent())
        else {
            continue;
        };
        let Some(stream) = pending.0.take() else {
            continue;
        };
        let mut client = ConnectionRuntime::new(
            processes.clone(),
            Vec::new(),
            shutdown.0.clone(),
            started_at.0,
        )
        .observations(
            observations
                .get(parent.parent())
                .ok()
                .map(|source| source.subscribe()),
        );
        for protocol in &protocols.0 {
            client.add_local(entity.to_bits(), protocol.clone(), peer.0.clone(), admitted);
        }
        let connection = Connection::new(stream, client);
        let wake = wake.0.clone();
        let task = executor.0.spawn(async move {
            if let Err(error) = connection.run().await {
                tracing::error!(%error, "client error");
            }
            let _ = wake.send(());
        });
        commands
            .entity(entity)
            .insert(AbortTask(task))
            .remove::<PendingClient>();
    }
}

fn reap_clients(clients: Query<(Entity, &AbortTask), With<Client>>, mut commands: Commands) {
    for (entity, task) in &clients {
        if task.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    use vmux_api::protocol::ServiceMessage;
    use vmux_api::protocol::{ClientMessage, ProcessId};
    use vmux_process::ProcessService;
    use vmux_transport::service::{ServiceCodec, ServiceProtocolDriver};

    #[cfg(target_os = "macos")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_local_peer_is_materialized_before_dispatch_without_disabling_legacy_protocols()
    {
        for trusted in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("service.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let binary = if trusted {
                std::env::current_exe().unwrap()
            } else {
                std::path::PathBuf::from("/bin/ls")
            };
            let (wake, _wakes) = mpsc::unbounded_channel();
            let (exit, mut exited) = mpsc::channel(1);
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ServiceDaemonPlugin));
            app.world_mut().spawn((
                Daemon,
                SocketListener(Some(listener)),
                ExitSignal(exit),
                Executor(tokio::runtime::Handle::current()),
                Wake(wake),
                TrustedDesktop::for_binary(&binary).unwrap(),
            ));
            app.update();
            let (release, held) = tokio::sync::oneshot::channel();
            let path = socket.clone();
            let client = tokio::spawn(async move {
                let stream = tokio::net::UnixStream::connect(path).await.unwrap();
                let (mut reader, mut writer) = stream.into_split();
                ServiceCodec::write_client(&mut writer, &ClientMessage::Status)
                    .await
                    .unwrap();
                let response = ServiceCodec::read_service(&mut reader)
                    .await
                    .unwrap()
                    .unwrap();
                let _ = held.await;
                response
            });
            let until = Instant::now() + std::time::Duration::from_secs(5);
            loop {
                app.update();
                let world = app.world_mut();
                if let Ok((peer, admitted)) = world
                    .query::<(&ClientPeer, Has<DesktopPeer>)>()
                    .single(world)
                {
                    assert_eq!(peer.0.uid(), unsafe { libc::geteuid() });
                    assert_eq!(peer.0.pid(), Some(std::process::id()));
                    assert_eq!(peer.0.verified_code(), trusted);
                    assert_eq!(admitted, trusted);
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "native peer facts were not materialized"
                );
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            release.send(()).unwrap();
            while !client.is_finished() {
                app.update();
                assert!(Instant::now() < until, "legacy Status did not complete");
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            assert!(matches!(
                client.await.unwrap(),
                ServiceMessage::StatusResponse {
                    process_count: 0,
                    ..
                }
            ));
            let shutdown = tokio::spawn(async move {
                let stream = tokio::net::UnixStream::connect(socket).await.unwrap();
                let (mut reader, mut writer) = stream.into_split();
                ServiceCodec::write_client(&mut writer, &ClientMessage::Shutdown)
                    .await
                    .unwrap();
                ServiceCodec::read_service(&mut reader).await.unwrap()
            });
            let until = Instant::now() + std::time::Duration::from_secs(5);
            loop {
                app.update();
                if exited.try_recv().is_ok() && shutdown.is_finished() {
                    break;
                }
                if shutdown.is_finished()
                    && app
                        .world_mut()
                        .query_filtered::<&AbortTask, With<Daemon>>()
                        .iter(app.world())
                        .all(|task| task.0.is_finished())
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "owned local peer fixture did not shut down"
                );
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            assert!(
                matches!(shutdown.await.unwrap(), Some(ServiceMessage::ProcessList { processes }) if processes.is_empty())
            );
        }
    }

    struct ProcessAppThread {
        stop: Option<std::sync::mpsc::Sender<()>>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl ProcessAppThread {
        fn start(wake: mpsc::UnboundedSender<()>) -> (Self, ProcessRuntime) {
            let (processes, runtime) = ProcessRuntime::new(wake);
            let (stop, stopped) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || {
                let mut app = App::new();
                app.add_plugins((MinimalPlugins, vmux_process::ProcessPlugin));
                app.world_mut()
                    .spawn((Name::new("vmux process runtime"), runtime));
                loop {
                    app.update();
                    match stopped.recv_timeout(std::time::Duration::from_millis(1)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            });
            (
                Self {
                    stop: Some(stop),
                    handle: Some(handle),
                },
                processes,
            )
        }
    }

    impl Drop for ProcessAppThread {
        fn drop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    async fn run_test_server(listener: UnixListener, wake: mpsc::UnboundedSender<()>) {
        let (_process_app, processes) = ProcessAppThread::start(wake.clone());
        let (connections, mut connection_inbox) = mpsc::unbounded_channel();
        let (shutdown, shutdown_inbox) = mpsc::channel(1);
        let (exit, mut exit_inbox) = mpsc::channel(1);
        let process: Arc<dyn ServiceProtocolDriver> =
            Arc::new(ProcessService::new(processes.clone()));
        let runtime = ConnectionRuntime::new(processes, vec![process], shutdown, Instant::now());
        let listener = tokio::spawn(
            ListenerDriver::new(listener, connections, wake, shutdown_inbox, exit).run(),
        );
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                Some(stream) = connection_inbox.recv() => {
                    clients.spawn(Connection::new(stream, runtime.clone()).run());
                }
                _ = exit_inbox.recv() => break,
            }
        }
        clients.abort_all();
        let _ = listener.await;
    }

    #[tokio::test]
    async fn shutdown_message_breaks_run_server() {
        let dir = std::env::temp_dir().join(format!("vmux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("test.sock");
        let _ = std::fs::remove_file(&sock);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (_r, mut w) = stream.into_split();
        let bytes =
            rkyv::to_bytes::<rkyv::rancor::Error>(&ClientMessage::Shutdown).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write shutdown");

        let res = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
        assert!(res.is_ok(), "run_server did not exit after Shutdown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn process_alive(pid: u32, identity: &Option<String>) -> bool {
        if unsafe { libc::kill(pid as i32, 0) } != 0 {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            if linux_proc_state(pid) == Some('Z') {
                return false;
            }
            if linux_proc_starttime(pid) != *identity {
                return false;
            }
        }
        true
    }

    fn proc_identity(pid: u32) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            linux_proc_starttime(pid)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_proc_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?.1.trim_start().chars().next()
    }

    #[cfg(target_os = "linux")]
    fn linux_proc_starttime(pid: u32) -> Option<String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)
            .map(str::to_string)
    }

    fn proc_state_label(pid: u32) -> String {
        #[cfg(target_os = "linux")]
        {
            linux_proc_state(pid)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "gone".to_string())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            "n/a".to_string()
        }
    }

    async fn await_child_pid(pidfile: &std::path::Path) -> Option<u32> {
        for _ in 0..200 {
            if let Ok(s) = std::fs::read_to_string(pidfile)
                && let Ok(pid) = s.trim().parse::<u32>()
                && pid > 0
            {
                return Some(pid);
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        None
    }

    #[tokio::test]
    async fn client_disconnect_reaps_created_processes() {
        let dir = std::env::temp_dir().join(format!("vmux-reap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("reap.sock");
        let pidfile = dir.join("child.pid");
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pidfile);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (r, mut w) = stream.into_split();

        let create = ClientMessage::CreateProcess {
            process_id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("echo $$ > {}; exec sleep 30", pidfile.display()),
            ],
            cwd: dir.display().to_string(),
            env: vec![],
            cols: 80,
            rows: 24,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&create).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write create");

        let pid = await_child_pid(&pidfile)
            .await
            .expect("child process should report its pid");
        let identity = proc_identity(pid);
        assert!(
            process_alive(pid, &identity),
            "child should be alive after CreateProcess"
        );

        drop(w);
        drop(r);

        let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while process_alive(pid, &identity) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        server.abort();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            reaped.is_ok(),
            "child pid {pid} still alive after client disconnect — service did not reap it (state: {})",
            proc_state_label(pid)
        );
    }

    #[tokio::test]
    async fn a_client_that_dies_mid_frame_still_has_its_processes_reaped() {
        let dir = std::env::temp_dir().join(format!("vmux-torn-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("torn.sock");
        let pidfile = dir.join("child.pid");
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pidfile);
        let listener = tokio::net::UnixListener::bind(&sock).unwrap();

        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(run_test_server(listener, wake_tx));

        let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
        let (r, mut w) = stream.into_split();

        let create = ClientMessage::CreateProcess {
            process_id: ProcessId::new(),
            command: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("echo $$ > {}; exec sleep 30", pidfile.display()),
            ],
            cwd: dir.display().to_string(),
            env: vec![],
            cols: 80,
            rows: 24,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&create).expect("serialize");
        ServiceCodec::write_raw(&mut w, &bytes)
            .await
            .expect("write create");

        let pid = await_child_pid(&pidfile)
            .await
            .expect("child process should report its pid");
        let identity = proc_identity(pid);

        tokio::io::AsyncWriteExt::write_all(&mut w, &1024u32.to_le_bytes())
            .await
            .expect("write prefix");
        tokio::io::AsyncWriteExt::write_all(&mut w, b"only a few bytes")
            .await
            .expect("write partial body");
        drop(w);
        drop(r);

        let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while process_alive(pid, &identity) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;

        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        server.abort();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            reaped.is_ok(),
            "child pid {pid} survived a torn frame — the read loop propagated instead of reaping (state: {})",
            proc_state_label(pid)
        );
    }
}
