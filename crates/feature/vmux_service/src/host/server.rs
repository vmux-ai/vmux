use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use vmux_ecs::service::{AbortTask, Executor, Operations, Protocols, Register, Remote, Wake};
use vmux_process::ProcessRuntime;
use vmux_transport::service::RemoteOperationStore;

use super::server_driver::{Connection, ConnectionRuntime, Listener as ListenerDriver};
use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::client_operation::ClientOperations;

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
struct Client;

type DaemonRuntimeState<'a> = (
    &'a Executor,
    &'a Wake,
    &'a ProcessRuntime,
    &'a Protocols,
    &'a Shutdown,
    &'a StartedAt,
);

#[derive(SystemParam)]
struct DaemonRuntime<'w, 's> {
    state: Single<'w, 's, DaemonRuntimeState<'static>, With<Daemon>>,
    inbox: Single<'w, 's, &'static mut ConnectionInbox, With<Daemon>>,
}

impl Plugin for ServiceDaemonPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_process::ProcessPlugin,
            vmux_process::ProcessServicePlugin,
            crate::remote::RemotePlugin,
        ))
        .configure_sets(Update, (Register, Launch).chain())
        .add_systems(Startup, prepare)
        .add_systems(
            Update,
            (
                start.in_set(Launch),
                ApplyDeferred,
                start_clients,
                reap_clients,
            )
                .chain(),
        );
    }
}

fn prepare(daemons: Query<(Entity, &Wake), With<Daemon>>, mut commands: Commands) {
    for (entity, wake) in &daemons {
        let (processes, process_runtime) = ProcessRuntime::new(wake.0.clone());
        let (client_operations, client_operation_runtime) = ClientOperations::new(wake.0.clone());
        let (authorizations, authorization_runtime) = RemoteAuthorizations::new(wake.0.clone());
        let operations: Arc<dyn RemoteOperationStore> = Arc::new(client_operations);
        commands.entity(entity).insert((
            Name::new("vmux service"),
            processes,
            Operations(operations),
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

fn start_clients(mut runtime: DaemonRuntime, mut commands: Commands) {
    let (executor, wake, processes, protocols, shutdown, started_at) = *runtime.state;
    while let Ok(stream) = runtime.inbox.0.try_recv() {
        let client = ConnectionRuntime::new(
            processes.clone(),
            protocols.0.clone(),
            shutdown.0.clone(),
            started_at.0,
        );
        let connection = Connection::new(stream, client);
        let wake = wake.0.clone();
        let task = executor.0.spawn(async move {
            if let Err(error) = connection.run().await {
                tracing::error!(%error, "client error");
            }
            let _ = wake.send(());
        });
        commands.spawn((Name::new("service client"), Client, AbortTask(task)));
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

    use vmux_api::protocol::{ClientMessage, ProcessId};
    use vmux_process::ProcessService;
    use vmux_transport::service::{ServiceCodec, ServiceProtocolDriver};

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
