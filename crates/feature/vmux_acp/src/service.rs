use std::sync::Arc;

use bevy::prelude::*;
use vmux_ecs::service::{Executor, Operations, Protocols, Register, Remote, Wake};
use vmux_process::ProcessRuntime;

use crate::service_driver::AcpService;

pub struct AcpServicePlugin;

impl Plugin for AcpServicePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::acp::AcpRuntimePlugin,
            crate::remote::RemoteRequestPlugin,
        ))
        .add_systems(Update, (register, ApplyDeferred).chain().in_set(Register));
    }
}

fn register(
    mut servers: Query<
        (
            Entity,
            &Executor,
            &Wake,
            &ProcessRuntime,
            &Operations,
            &mut Protocols,
            &mut Remote,
        ),
        Without<AcpService>,
    >,
    mut commands: Commands,
) {
    for (entity, executor, wake, processes, _operations, mut protocols, mut remote) in &mut servers
    {
        let (agent, runtime) =
            AcpService::new(executor.0.clone(), wake.0.clone(), processes.clone());
        protocols.0.push(Arc::new(agent.clone()));
        let (driver, inbox) = agent.remote_driver(wake.0.clone());
        remote.0 = Some(driver);
        commands.entity(entity).insert((agent, runtime, inbox));
    }
}
