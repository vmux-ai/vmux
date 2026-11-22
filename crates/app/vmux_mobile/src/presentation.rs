use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::Query;
use vmux_api::page::UiStateEmit;

use crate::ProjectSet;
use crate::connection::{
    AuthStage, ConnectionNotice, ConnectionPairing, ConnectionReachable, ConnectionSource,
    MobileRoot, PairDraft,
};
use crate::session::{MobileRoute, SelectedSession, StreamConnected};

pub(crate) struct MobilePresentationPlugin;

impl BevyPlugin for MobilePresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RepublishMobileConnection>()
            .add_message::<RepublishMobileSession>()
            .add_systems(
                PreUpdate,
                (project_connection, project_session).in_set(ProjectSet),
            );
    }
}

#[vmux_api::contract(Copy, Default, Eq)]
pub(crate) enum MobileAuthStage {
    #[default]
    Loading,
    Paired,
    Unpaired,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub(crate) enum MobileScreenRoute {
    #[default]
    Start,
    Session,
    Team,
}

#[vmux_api::ui_state(Default, Eq)]
pub(crate) struct MobileConnectionUiState {
    pub(crate) auth: MobileAuthStage,
    pub(crate) pair_url: String,
    pub(crate) error: String,
    pub(crate) reachable: bool,
    pub(crate) pairing: bool,
}

#[vmux_api::ui_state(Default, Eq)]
pub(crate) struct MobileSessionUiState {
    pub(crate) route: MobileScreenRoute,
    pub(crate) selected_sid: String,
    pub(crate) connected: bool,
}

#[derive(Message)]
pub(crate) struct RepublishMobileConnection;

#[derive(Message)]
pub(crate) struct RepublishMobileSession;

#[derive(Component, Default)]
pub(crate) struct PublishedConnectionState(Option<u64>);

impl PublishedConnectionState {
    fn should_emit(&mut self, bytes: &[u8], force: bool) -> bool {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let digest = hasher.finish();
        if !force && self.0 == Some(digest) {
            return false;
        }
        self.0 = Some(digest);
        true
    }
}

#[derive(Component, Default)]
pub(crate) struct PublishedSessionState(Option<u64>);

impl PublishedSessionState {
    fn should_emit(&mut self, bytes: &[u8], force: bool) -> bool {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let digest = hasher.finish();
        if !force && self.0 == Some(digest) {
            return false;
        }
        self.0 = Some(digest);
        true
    }
}

fn project_connection(
    mut republishes: MessageReader<RepublishMobileConnection>,
    sources: Query<
        (
            &AuthStage,
            &PairDraft,
            &ConnectionNotice,
            &ConnectionReachable,
            &ConnectionPairing,
        ),
        With<ConnectionSource>,
    >,
    mut roots: Query<&mut PublishedConnectionState, With<MobileRoot>>,
    mut emits: MessageWriter<UiStateEmit>,
) {
    let force = republishes.read().count() > 0;
    let Ok((auth, draft, notice, reachable, pairing)) = sources.single() else {
        return;
    };
    let Ok(mut published) = roots.single_mut() else {
        return;
    };
    let next = MobileConnectionUiState {
        auth: match auth {
            AuthStage::Loading => MobileAuthStage::Loading,
            AuthStage::Paired => MobileAuthStage::Paired,
            AuthStage::Unpaired => MobileAuthStage::Unpaired,
        },
        pair_url: draft.0.clone(),
        error: notice.0.clone(),
        reachable: reachable.0,
        pairing: pairing.0,
    };
    let Some(emit) = UiStateEmit::from_state(&next) else {
        return;
    };
    if published.should_emit(&emit.bytes, force) {
        emits.write(emit);
    }
}

fn project_session(
    mut republishes: MessageReader<RepublishMobileSession>,
    mut roots: Query<
        (
            &MobileRoute,
            &StreamConnected,
            Option<&SelectedSession>,
            &mut PublishedSessionState,
        ),
        With<MobileRoot>,
    >,
    mut emits: MessageWriter<UiStateEmit>,
) {
    let force = republishes.read().count() > 0;
    let Ok((route, connected, selected, mut published)) = roots.single_mut() else {
        return;
    };
    let next = MobileSessionUiState {
        route: match route {
            MobileRoute::Start => MobileScreenRoute::Start,
            MobileRoute::Session => MobileScreenRoute::Session,
            MobileRoute::Team => MobileScreenRoute::Team,
        },
        selected_sid: selected
            .map(|selected| selected.session_id.0.clone())
            .unwrap_or_default(),
        connected: connected.0,
    };
    let Some(emit) = UiStateEmit::from_state(&next) else {
        return;
    };
    if published.should_emit(&emit.bytes, force) {
        emits.write(emit);
    }
}
