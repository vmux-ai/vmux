use std::sync::Arc;

use bevy::prelude::*;
use vmux_ecs::service::{Protocols, Register};
use vmux_transport::service::ServiceProtocolDriver;

use crate::ProcessRuntime;
use crate::service_driver::ProcessService;

pub struct ProcessServicePlugin;

impl Plugin for ProcessServicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, register.in_set(Register));
    }
}

#[derive(Component)]
struct Registered;

fn register(
    mut servers: Query<(Entity, &ProcessRuntime, &mut Protocols), Without<Registered>>,
    mut commands: Commands,
) {
    for (entity, processes, mut protocols) in &mut servers {
        protocols.0.push(
            Arc::new(ProcessService::new(processes.clone())) as Arc<dyn ServiceProtocolDriver>
        );
        commands.entity(entity).insert(Registered);
    }
}
