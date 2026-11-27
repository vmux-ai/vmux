use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::thread;
use std::time::Duration;

use bevy::prelude::*;
use tokio::runtime::Runtime;
use tokio::sync::mpsc as tokio_mpsc;
use vmux_api::ProcessId;
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_ecs::service::{
    ServiceConnected, ServiceInbound, ServiceMessageIngressSet, ServiceRequest,
};
use vmux_process::{ProcessLaunch, ProcessPlugin, ProcessRuntime};
use vmux_transport::service::ServiceConnection;

pub struct ServiceBridgePlugin {
    pub prefer_daemon: bool,
}

impl Default for ServiceBridgePlugin {
    fn default() -> Self {
        Self {
            prefer_daemon: true,
        }
    }
}

impl Plugin for ServiceBridgePlugin {
    fn build(&self, app: &mut App) {
        let bridge =
            ServiceBridge::connect(self.prefer_daemon).expect("service bridge must initialize");
        app.insert_resource(bridge)
            .add_message::<ServiceRequest>()
            .add_message::<ServiceInbound>()
            .add_systems(Startup, connect)
            .add_systems(Update, ingress.in_set(ServiceMessageIngressSet))
            .add_systems(Last, egress);
    }
}

#[derive(Resource)]
pub struct ServiceBridge {
    cmd_tx: Mutex<std_mpsc::Sender<ClientMessage>>,
    msg_rx: Mutex<std_mpsc::Receiver<ServiceMessage>>,
    _runtime: Arc<Runtime>,
}

impl ServiceBridge {
    pub fn send(&self, message: ClientMessage) {
        if let Ok(tx) = self.cmd_tx.lock() {
            let _ = tx.send(message);
        }
    }

    fn connect(prefer_daemon: bool) -> Result<Self, String> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?,
        );
        if prefer_daemon && let Some(bridge) = Self::connect_daemon(Arc::clone(&runtime)) {
            return Ok(bridge);
        }
        Self::start_embedded(runtime)
    }

    fn connect_daemon(runtime: Arc<Runtime>) -> Option<Self> {
        let (tx, rx) = std_mpsc::channel();
        let rt = Arc::clone(&runtime);
        let _ = thread::Builder::new()
            .name("vmux-tui-connect".into())
            .spawn(move || {
                let result = rt.block_on(ServiceConnection::connect());
                let _ = tx.send(result);
            });
        let connection = match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(Ok(conn)) => Arc::new(conn),
            _ => return None,
        };
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<ClientMessage>();
        let (msg_tx, msg_rx) = std_mpsc::channel::<ServiceMessage>();
        let read_conn = Arc::clone(&connection);
        let read_rt = Arc::clone(&runtime);
        let _ = thread::Builder::new()
            .name("vmux-tui-read".into())
            .spawn(move || {
                read_rt.block_on(async move {
                    while let Ok(Some(message)) = read_conn.recv().await {
                        if msg_tx.send(message).is_err() {
                            break;
                        }
                    }
                });
            });
        let write_conn = Arc::clone(&connection);
        let write_rt = Arc::clone(&runtime);
        let _ = thread::Builder::new()
            .name("vmux-tui-write".into())
            .spawn(move || {
                write_rt.block_on(async move {
                    while let Ok(message) = cmd_rx.recv() {
                        if write_conn.send(&message).await.is_err() {
                            break;
                        }
                    }
                });
            });
        Some(Self {
            cmd_tx: Mutex::new(cmd_tx),
            msg_rx: Mutex::new(msg_rx),
            _runtime: runtime,
        })
    }

    fn start_embedded(runtime: Arc<Runtime>) -> Result<Self, String> {
        let (wake_tx, mut wake_rx) = tokio_mpsc::unbounded_channel::<()>();
        let (process_runtime, bundle) = ProcessRuntime::new(wake_tx);
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<ClientMessage>();
        let (msg_tx, msg_rx) = std_mpsc::channel::<ServiceMessage>();
        thread::Builder::new()
            .name("vmux-tui-process-ecs".into())
            .spawn(move || {
                let mut app = App::new();
                app.add_plugins(ProcessPlugin);
                app.world_mut().spawn(bundle);
                app.finish();
                app.cleanup();
                loop {
                    app.update();
                    if wake_rx.try_recv().is_ok() {
                        while wake_rx.try_recv().is_ok() {}
                        continue;
                    }
                    thread::sleep(Duration::from_millis(8));
                }
            })
            .map_err(|error| error.to_string())?;
        let driver_rt = Arc::clone(&runtime);
        thread::Builder::new()
            .name("vmux-tui-embedded-bridge".into())
            .spawn(move || {
                Self::run_embedded_bridge(driver_rt, process_runtime, cmd_rx, msg_tx);
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            cmd_tx: Mutex::new(cmd_tx),
            msg_rx: Mutex::new(msg_rx),
            _runtime: runtime,
        })
    }

    fn run_embedded_bridge(
        runtime: Arc<Runtime>,
        processes: ProcessRuntime,
        cmd_rx: std_mpsc::Receiver<ClientMessage>,
        msg_tx: std_mpsc::Sender<ServiceMessage>,
    ) {
        runtime.block_on(async move {
            let mut tasks: HashMap<ProcessId, tokio::task::JoinHandle<()>> = HashMap::new();
            while let Ok(command) = cmd_rx.recv() {
                Self::dispatch_embedded(&processes, &msg_tx, &mut tasks, command).await;
            }
            for (_, handle) in tasks {
                handle.abort();
            }
            let _ = processes.shutdown().await;
        });
    }

    async fn dispatch_embedded(
        processes: &ProcessRuntime,
        msg_tx: &std_mpsc::Sender<ServiceMessage>,
        tasks: &mut HashMap<ProcessId, tokio::task::JoinHandle<()>>,
        command: ClientMessage,
    ) {
        match command {
            ClientMessage::CreateProcess {
                process_id,
                command,
                args,
                cwd,
                env,
                cols,
                rows,
            } => {
                let launch = ProcessLaunch {
                    id: process_id,
                    command,
                    args,
                    cwd,
                    env,
                    cols,
                    rows,
                    keep_after_exit: false,
                };
                match processes.create(launch).await {
                    Ok(created) => {
                        let _ = msg_tx.send(ServiceMessage::ProcessCreated {
                            process_id: created.id,
                            pid: created.pid,
                        });
                        let mut updates = created.updates;
                        let forward = msg_tx.clone();
                        let id = created.id;
                        tasks.insert(
                            id,
                            tokio::spawn(async move {
                                while let Ok(update) = updates.recv().await {
                                    if forward.send(update.into_service_message(id)).is_err() {
                                        break;
                                    }
                                }
                            }),
                        );
                    }
                    Err(reason) => {
                        let _ =
                            msg_tx.send(ServiceMessage::ProcessCreateFailed { process_id, reason });
                    }
                }
            }
            ClientMessage::ProcessInput { process_id, data } => {
                let _ = processes.input(process_id, data).await;
            }
            ClientMessage::MouseWheel {
                process_id,
                up,
                col,
                row,
                modifiers,
            } => {
                let _ = processes
                    .mouse_wheel(process_id, up, col, row, modifiers)
                    .await;
            }
            ClientMessage::ScrollWindow {
                process_id,
                top_row,
                follow,
            } => {
                let _ = processes.scroll_window(process_id, top_row, follow).await;
            }
            ClientMessage::ResizeProcess {
                process_id,
                cols,
                rows,
            } => {
                let _ = processes.resize(process_id, cols, rows).await;
            }
            ClientMessage::KillProcess { process_id } => {
                let _ = processes.remove(process_id).await;
                if let Some(handle) = tasks.remove(&process_id) {
                    handle.abort();
                }
            }
            ClientMessage::ListProcesses => {
                let list = processes.list().await.unwrap_or_default();
                let _ = msg_tx.send(ServiceMessage::ProcessList { processes: list });
            }
            _ => {}
        }
    }
}

fn connect(mut commands: Commands) {
    commands.spawn((Name::new("TUI service bridge"), ServiceConnected));
}

fn ingress(bridge: Res<ServiceBridge>, mut inbound: MessageWriter<ServiceInbound>) {
    let Ok(rx) = bridge.msg_rx.lock() else {
        return;
    };
    while let Ok(message) = rx.try_recv() {
        inbound.write(ServiceInbound(message));
    }
}

fn egress(bridge: Res<ServiceBridge>, mut requests: MessageReader<ServiceRequest>) {
    for request in requests.read() {
        bridge.send(request.0.clone());
    }
}
