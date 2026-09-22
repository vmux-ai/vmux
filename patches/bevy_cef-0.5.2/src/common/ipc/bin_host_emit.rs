use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::{BinEventTarget, HostEvent};

#[derive(Reflect, Debug, Clone, EntityEvent)]
#[reflect(opaque)]
pub struct BinHostEmitEvent {
    #[event_target]
    webview: Entity,
    id: &'static str,
    target: BinEventTarget,
    payload: Vec<u8>,
}

impl BinHostEmitEvent {
    fn from_bytes<T>(webview: Entity, payload: Vec<u8>) -> Self
    where
        T: HostEvent,
    {
        Self {
            webview,
            id: T::id(),
            target: T::TARGET,
            payload,
        }
    }

    pub fn from_event<T>(webview: Entity, value: &T) -> Self
    where
        T: HostEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to serialize binary host event {}: {error:?}",
                    T::id()
                )
            })
            .into_vec();
        Self::from_bytes::<T>(webview, payload)
    }

    pub const fn webview(&self) -> Entity {
        self.webview
    }

    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn target(&self) -> BinEventTarget {
        self.target
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

pub(super) struct BinHostEmitPlugin;

impl Plugin for BinHostEmitPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<BinHostEmitEvent>()
            .add_observer(bin_host_emit);
    }
}

fn bin_host_emit(trigger: On<BinHostEmitEvent>, browsers: NonSend<Browsers>) {
    let Some(host) = browsers.page_host(&trigger.webview()) else {
        return;
    };
    if !trigger.target().accepts(&host) {
        warn!(
            "blocked binary host event {} for unexpected page host {host}",
            trigger.id()
        );
        return;
    }
    browsers.emit_event_bytes(&trigger.webview(), trigger.id(), trigger.payload());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    #[vmux_api::host_event(target = "test-host")]
    struct TestPayload {
        value: u32,
    }

    #[test]
    fn bin_host_emit_event_from_event_round_trips() {
        let original = TestPayload { value: 42 };
        let event = BinHostEmitEvent::from_event(Entity::PLACEHOLDER, &original);
        assert_eq!(event.id(), "test_payload@1");
        assert_eq!(event.target(), BinEventTarget::Host("test-host"));
        let recovered =
            rkyv::from_bytes::<TestPayload, rkyv::rancor::Error>(event.payload()).expect("decode");
        assert_eq!(original, recovered);
    }
}
