pub use vmux_api::process_id::*;

#[cfg(all(host, feature = "host"))]
#[derive(bevy_ecs::prelude::EntityEvent, Clone, Copy, Debug)]
pub struct ProcessPresentationRequest {
    #[event_target]
    pub owner: bevy_ecs::prelude::Entity,
    pub process_id: ProcessId,
}
