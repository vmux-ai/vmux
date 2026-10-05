use std::sync::Arc;

use bevy::prelude::*;
use vmux_ecs::service::{Executor, Operations, Protocols, Register, Remote, Wake};
use vmux_process::ProcessRuntime;
use vmux_transport::service::ServiceProtocolDriver;

use crate::service_driver::AgentService;

pub struct AgentServicePlugin;

impl Plugin for AgentServicePlugin {
    fn build(&self, app: &mut App) {
        crate::acp::add(app);
        app.add_systems(Update, (register, ApplyDeferred).chain().in_set(Register));
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
        Without<AgentService>,
    >,
    mut commands: Commands,
) {
    for (entity, executor, wake, processes, operations, mut protocols, mut remote) in &mut servers {
        let (agent, runtime) =
            AgentService::new(executor.0.clone(), wake.0.clone(), processes.clone());
        protocols
            .0
            .push(Arc::new(agent.clone()) as Arc<dyn ServiceProtocolDriver>);
        remote.0 = Some(agent.remote_driver(operations.0.clone()));
        commands.entity(entity).insert((agent, runtime));
    }
}
