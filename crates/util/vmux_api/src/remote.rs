#[cfg(feature = "bevy")]
#[derive(bevy_ecs::prelude::Component)]
pub struct Live;

#[cfg(feature = "bevy")]
#[derive(bevy_ecs::schedule::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LiveDecisionSet;

#[cfg(feature = "bevy")]
#[derive(bevy_ecs::schedule::SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RemoteProjectionSet;

#[cfg_attr(feature = "bevy", derive(bevy_ecs::prelude::Component))]
#[vmux_api::contract(Copy, Default, Eq)]
pub enum RemotePhase {
    #[default]
    Disabled,
    Starting,
    Stopping,
    Enabled,
    Error,
}

#[vmux_api::contract(Default, Eq)]
pub struct Observation {
    pub epoch: uuid::Uuid,
    pub revision: u64,
    pub phase: RemotePhase,
}

#[cfg_attr(feature = "bevy", derive(bevy_ecs::prelude::Component))]
#[vmux_api::contract(Default, Eq)]
pub struct UiRemote {
    pub available: bool,
    pub requested: bool,
    pub phase: RemotePhase,
    pub pairing_url: String,
    pub pairing_deep_link: String,
    pub paired: bool,
    #[serde(default)]
    pub pairing_visible: bool,
    pub devices: Vec<RemoteDevice>,
    pub error: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct RemoteDevice {
    pub id: String,
}

#[vmux_api::ui_event(Copy, Default, Eq)]
pub struct RemoteRequest {
    pub enabled: bool,
}

#[vmux_api::ui_event]
pub struct RemotePairingShowRequest;

#[vmux_api::ui_event]
pub struct RemotePairingDismissRequest;

#[vmux_api::ui_event]
pub struct RemoteCopyEvent;

#[vmux_api::ui_event(Default, Eq)]
pub struct RemoteRevokeRequest {
    pub client_id: String,
}
