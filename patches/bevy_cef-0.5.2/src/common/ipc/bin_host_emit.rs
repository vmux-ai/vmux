use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;

#[derive(Reflect, Debug, Clone, EntityEvent)]
#[reflect(opaque)]
pub struct BinHostEmitEvent {
    #[event_target]
    pub webview: Entity,
    pub id: String,
    pub payload: Vec<u8>,
}

impl BinHostEmitEvent {
    pub fn from_bytes(webview: Entity, id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            webview,
            id: id.into(),
            payload,
        }
    }

    pub fn from_rkyv<T>(webview: Entity, id: impl Into<String>, value: &T) -> Self
    where
        T: for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .map(|b| b.into_vec())
            .unwrap_or_default();
        Self::from_bytes(webview, id, bytes)
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
    webview_debug_log(format!(
        "bin_host_emit entity={:?} id={} payload_len={}",
        trigger.webview,
        trigger.id,
        trigger.payload.len()
    ));
    browsers.emit_event_bytes(&trigger.webview, trigger.id.clone(), &trigger.payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[derive(Debug, Clone, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct TestPayload {
        value: u32,
    }

    #[test]
    fn bin_host_emit_event_from_rkyv_round_trips() {
        let original = TestPayload { value: 42 };
        let event = BinHostEmitEvent::from_rkyv(Entity::PLACEHOLDER, "test-id", &original);
        assert_eq!(event.id, "test-id");
        let recovered =
            rkyv::from_bytes::<TestPayload, rkyv::rancor::Error>(&event.payload).expect("decode");
        assert_eq!(original, recovered);
    }
}
