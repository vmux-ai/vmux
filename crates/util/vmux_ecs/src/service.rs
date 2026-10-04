#![cfg(host)]

use bevy::prelude::*;
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_api::service::ServiceMessageVariant;

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
pub struct ServiceMessageSet;

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
