#![cfg(host)]

use std::sync::Arc;

use bevy::prelude::*;
use tokio::sync::mpsc;
use vmux_api::protocol::{ClientMessage, ServiceMessage};
pub use vmux_api::service::ServiceMessageSet;
use vmux_api::service::ServiceMessageVariant;
use vmux_transport::service::{RemoteDriver, RemoteOperationStore, ServiceProtocolDriver};

pub use crate::shutdown::{Shutdown, ShutdownComplete, ShutdownPending};

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShutdownSet;

#[derive(Resource)]
pub struct Restarting;

#[derive(Component, Clone)]
pub struct Executor(pub tokio::runtime::Handle);

#[derive(Component, Clone)]
pub struct Wake(pub mpsc::UnboundedSender<()>);

#[derive(Component, Clone)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub struct Operations(pub Arc<dyn RemoteOperationStore>);

#[derive(Component, Default)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub struct Protocols(pub Vec<Arc<dyn ServiceProtocolDriver>>);

#[derive(Component, Default)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub struct Remote(pub Option<Arc<dyn RemoteDriver>>);

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub struct Register;

#[derive(Clone, Message)]
pub struct ServiceRequest(pub ClientMessage);

#[derive(Clone, Message)]
pub struct ServiceInbound(pub ServiceMessage);

pub trait ServiceMessageAppExt {
    fn add_service_message<M>(&mut self) -> &mut Self
    where
        M: ServiceMessageVariant;
}

impl ServiceMessageAppExt for App {
    fn add_service_message<M>(&mut self) -> &mut Self
    where
        M: ServiceMessageVariant,
    {
        self.add_message::<ServiceInbound>()
            .add_message::<M>()
            .configure_sets(
                Update,
                (
                    ServiceMessageIngressSet,
                    ServiceMessageDecodeSet,
                    ServiceMessageSet,
                )
                    .chain(),
            )
            .add_systems(Update, route_message::<M>.in_set(ServiceMessageDecodeSet))
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServiceMessageIngressSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ServiceMessageDecodeSet;

fn route_message<M: ServiceMessageVariant>(
    mut inbound: MessageReader<ServiceInbound>,
    mut messages: MessageWriter<M>,
) {
    for inbound in inbound.read() {
        let Some(message) = M::from_service_message(&inbound.0) else {
            continue;
        };
        messages.write(message);
    }
}

#[derive(Component)]
pub struct ServiceConnected;

#[derive(Component, Clone, Debug)]
pub struct ServiceUnavailable(pub String);
