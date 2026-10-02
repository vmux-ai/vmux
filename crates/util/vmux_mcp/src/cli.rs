use std::io::{self, Write};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use vmux_ecs::ProcessId;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::host::manifest::FeaturePlugin;

use crate::protocol::{McpConfig, McpInput, McpOutput, McpPlugin, McpRuntime, McpServer, McpSet};

pub struct McpCliPlugin;

impl Plugin for McpCliPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            Name::new("MCP async runtime"),
            McpRuntime(tokio::runtime::Handle::current()),
        ));
        app.add_plugins((FeaturePlugin::<crate::Feature>::default(), McpPlugin))
            .add_systems(
                Update,
                (start_stdio, receive_stdio).chain().in_set(McpSet::Input),
            )
            .add_systems(
                Update,
                (write_stdio, finish_stdio).chain().in_set(McpSet::Output),
            );
    }
}

struct McpCliOptions {
    anchor: Option<ProcessId>,
    profile: Option<String>,
    acp_session: bool,
    acp_terminals: bool,
    run_block_timeout: Duration,
    shell: String,
}

impl TryFrom<&CliInvocation> for McpCliOptions {
    type Error = String;

    fn try_from(invocation: &CliInvocation) -> Result<Self, Self::Error> {
        let anchor = invocation
            .value("anchor")
            .and_then(|value| value.parse::<ProcessId>().ok());
        let profile = invocation
            .value("profile")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let run_timeout_secs = invocation
            .value("run_timeout_secs")
            .unwrap_or("50")
            .parse::<u64>()
            .map_err(|error| format!("invalid --run-timeout-secs: {error}"))?;
        Ok(Self {
            anchor,
            profile,
            acp_session: invocation.flag("acp_session"),
            acp_terminals: invocation.flag("acp_terminals"),
            run_block_timeout: Duration::from_secs(run_timeout_secs),
            shell: invocation.value("shell").unwrap_or_default().to_string(),
        })
    }
}

#[derive(Component)]
struct McpStdio {
    invocation: Entity,
    receiver: Mutex<Receiver<McpStdin>>,
    pending: usize,
    eof: bool,
    failed: bool,
}

#[derive(Component)]
struct McpStdinWorker(Option<std::thread::JoinHandle<()>>);

enum McpStdin {
    Input(serde_json::Value),
    Eof,
    Error(String),
}

fn start_stdio(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if !invocation.is("mcp") {
            continue;
        }
        let options = match McpCliOptions::try_from(invocation) {
            Ok(options) => options,
            Err(error) => {
                commands.entity(entity).insert(CliResult(Err(error)));
                continue;
            }
        };
        if let Some(profile) = options.profile {
            unsafe { std::env::set_var("VMUX_PROFILE", profile) };
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(64);
        let worker = std::thread::Builder::new()
            .name("mcp-stdin".into())
            .spawn(move || {
                let stdin = io::stdin();
                let mut reader = stdin.lock();
                loop {
                    match McpInput::read(&mut reader) {
                        Ok(Some(McpInput(value))) => {
                            if sender.send(McpStdin::Input(value)).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {
                            let _ = sender.send(McpStdin::Eof);
                            return;
                        }
                        Err(error) => {
                            let _ = sender.send(McpStdin::Error(error.to_string()));
                            return;
                        }
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                commands.entity(entity).insert(CliResult(Err(format!(
                    "failed to start MCP stdin worker: {error}"
                ))));
                continue;
            }
        };
        commands.spawn((
            Name::new("MCP protocol runtime"),
            McpServer::default(),
            McpConfig {
                anchor: options.anchor,
                acp_session: options.acp_session,
                acp_terminals: options.acp_terminals,
                run_block_timeout: options.run_block_timeout,
                shell: options.shell,
            },
            McpStdio {
                invocation: entity,
                receiver: Mutex::new(receiver),
                pending: 0,
                eof: false,
                failed: false,
            },
            McpStdinWorker(Some(worker)),
        ));
    }
}

fn receive_stdio(
    mut servers: Query<&mut McpStdio>,
    mut input: MessageWriter<McpInput>,
    mut commands: Commands,
) {
    for mut stdio in &mut servers {
        loop {
            let received = stdio.receiver.get_mut().unwrap().try_recv();
            match received {
                Ok(McpStdin::Input(value)) => {
                    if value.get("id").is_some() {
                        stdio.pending += 1;
                    }
                    input.write(McpInput(value));
                }
                Ok(McpStdin::Eof) | Err(TryRecvError::Disconnected) => {
                    stdio.eof = true;
                    break;
                }
                Ok(McpStdin::Error(error)) => {
                    stdio.eof = true;
                    stdio.failed = true;
                    commands
                        .entity(stdio.invocation)
                        .insert(CliResult(Err(error)));
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
    }
}

fn write_stdio(
    mut output: MessageReader<McpOutput>,
    mut servers: Query<&mut McpStdio>,
    mut commands: Commands,
) {
    let Ok(mut stdio) = servers.single_mut() else {
        return;
    };
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    for McpOutput(value) in output.read() {
        let result = serde_json::to_writer(&mut writer, value)
            .and_then(|()| writer.write_all(b"\n").map_err(serde_json::Error::io))
            .and_then(|()| writer.flush().map_err(serde_json::Error::io));
        if let Err(error) = result {
            stdio.eof = true;
            stdio.failed = true;
            commands
                .entity(stdio.invocation)
                .insert(CliResult(Err(error.to_string())));
        }
        stdio.pending = stdio.pending.saturating_sub(1);
    }
}

fn finish_stdio(
    mut servers: Query<(Entity, &McpStdio, &mut McpStdinWorker)>,
    mut commands: Commands,
) {
    for (server, stdio, mut worker) in &mut servers {
        if !stdio.eof || stdio.pending != 0 {
            continue;
        }
        if let Some(worker) = worker.0.take() {
            let _ = worker.join();
        }
        if !stdio.failed {
            commands
                .entity(stdio.invocation)
                .insert(CliResult::success());
        }
        commands.entity(server).despawn();
    }
}
