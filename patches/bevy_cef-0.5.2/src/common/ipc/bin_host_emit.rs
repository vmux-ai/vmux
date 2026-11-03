use bevy::prelude::*;
use bevy_cef_core::prelude::*;
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use vmux_api::{HostEvent, UiEventPermissions};

#[derive(Clone, Debug, Default)]
pub struct BinHostEmitReceipt(Arc<AtomicBool>);

impl BinHostEmitReceipt {
    pub fn delivered(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn mark_delivered(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Reflect, Debug, Clone, EntityEvent)]
#[reflect(opaque)]
pub struct BinHostEmitEvent {
    #[event_target]
    webview: Entity,
    id: &'static str,
    permission: &'static str,
    payload: Arc<[u8]>,
    surface_generation: u64,
    navigation_revision: u64,
    ipc_epoch: u64,
    #[reflect(ignore)]
    receipt: BinHostEmitReceipt,
}

impl BinHostEmitEvent {
    fn from_bytes<T>(webview: Entity, payload: Vec<u8>, authority: Option<(u64, u64, u64)>) -> Self
    where
        T: HostEvent,
    {
        let (surface_generation, navigation_revision, ipc_epoch) = authority.unwrap_or_default();
        Self {
            webview,
            id: T::id(),
            permission: T::PERMISSION,
            payload: payload.into(),
            surface_generation,
            navigation_revision,
            ipc_epoch,
            receipt: BinHostEmitReceipt::default(),
        }
    }

    pub fn from_event<T>(webview: Entity, value: &T) -> Self
    where
        T: HostEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        Self::from_payload::<T, T>(webview, value)
    }

    pub fn from_event_at<T>(webview: Entity, authority: (u64, u64, u64), value: &T) -> Self
    where
        T: HostEvent
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        Self::from_payload_at::<T, T>(webview, authority, value)
    }

    pub fn from_payload<C, P>(webview: Entity, value: &P) -> Self
    where
        C: HostEvent,
        P: for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        Self::serialize::<C, P>(webview, None, value)
    }

    pub fn from_payload_at<C, P>(webview: Entity, authority: (u64, u64, u64), value: &P) -> Self
    where
        C: HostEvent,
        P: for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        Self::serialize::<C, P>(webview, Some(authority), value)
    }

    fn serialize<C, P>(webview: Entity, authority: Option<(u64, u64, u64)>, value: &P) -> Self
    where
        C: HostEvent,
        P: for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        let payload = rkyv::to_bytes::<rkyv::rancor::Error>(value)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to serialize binary host event {}: {error:?}",
                    C::id()
                )
            })
            .into_vec();
        Self::from_bytes::<C>(webview, payload, authority)
    }

    pub const fn webview(&self) -> Entity {
        self.webview
    }

    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn permission(&self) -> &'static str {
        self.permission
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub const fn authority(&self) -> (u64, u64, u64) {
        (
            self.surface_generation,
            self.navigation_revision,
            self.ipc_epoch,
        )
    }

    pub fn receipt(&self) -> BinHostEmitReceipt {
        self.receipt.clone()
    }

    pub fn mark_delivered(&self) {
        self.receipt.mark_delivered();
    }

    pub const fn matches_authority(
        &self,
        surface_generation: u64,
        navigation_revision: u64,
        ipc_epoch: u64,
    ) -> bool {
        self.surface_generation == surface_generation
            && self.navigation_revision == navigation_revision
            && self.ipc_epoch == ipc_epoch
            && surface_generation != 0
            && navigation_revision != 0
            && ipc_epoch != 0
    }
}

pub(super) struct BinHostEmitPlugin;

impl Plugin for BinHostEmitPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<BinHostEmitEvent>()
            .add_observer(bin_host_emit);
    }
}

fn bin_host_emit(
    trigger: On<BinHostEmitEvent>,
    browsers: NonSend<Browsers>,
    blocked: Query<(), With<BinIpcBlocked>>,
    authority: Query<(
        &BinIpcSurfaceGeneration,
        &BinIpcNavigationRevision,
        &BinIpcEpoch,
    )>,
    permissions: Query<&UiEventPermissions>,
) {
    if blocked.contains(trigger.webview()) {
        return;
    }
    let Ok((surface_generation, navigation_revision, ipc_epoch)) = authority.get(trigger.webview())
    else {
        return;
    };
    if !trigger.matches_authority(surface_generation.0, navigation_revision.0, ipc_epoch.0) {
        return;
    }
    let Some(page_url) = browsers.page_url(&trigger.webview()) else {
        return;
    };
    if !UiEventPermissions::allows_page(permissions.iter(), &page_url, trigger.permission()) {
        warn!(
            "blocked binary host event {} for unexpected page URL {page_url}",
            trigger.id()
        );
        return;
    }
    if browsers.emit_event_bytes(
        &trigger.webview(),
        trigger.id(),
        trigger.payload(),
        trigger.authority(),
    ) {
        trigger.mark_delivered();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[vmux_api::host_event]
    struct TestPayload {
        value: u32,
    }

    #[test]
    fn bin_host_emit_event_from_event_round_trips() {
        let original = TestPayload { value: 42 };
        let event = BinHostEmitEvent::from_event_at(Entity::PLACEHOLDER, (3, 5, 7), &original);
        assert_eq!(event.id(), "test_payload@1");
        assert_eq!(event.permission(), "TestPayload");
        assert_eq!(event.authority(), (3, 5, 7));
        assert!(event.matches_authority(3, 5, 7));
        assert!(!event.matches_authority(3, 6, 7));
        let recovered =
            rkyv::from_bytes::<TestPayload, rkyv::rancor::Error>(event.payload()).expect("decode");
        assert_eq!(original, recovered);
    }
}
