use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::component::Component;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, NonSendMut, Query, Single};
use bevy_tasks::{IoTaskPool, Task, futures_lite::future};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_api::room::{RemoteAgent, RemoteSession};
use vmux_transport::{ClientCredential, DeviceId};
use vmux_ui::i18n::translate;

use crate::credentials::StoredCredentials;
use crate::qr_scanner;
use crate::remote::{Api, ApiError};
use crate::runtime::RuntimeHandle;

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

pub(crate) struct PairingPlugin;

impl Plugin for PairingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PairLinkChanged>()
            .add_message::<PairRequest>()
            .add_message::<PairingFailure>()
            .add_message::<DisconnectRequest>()
            .insert_non_send(ConnectionSubscribers::default())
            .add_systems(
                Startup,
                (spawn_connection_state, restore_connection).chain(),
            )
            .add_systems(
                Update,
                (
                    change_pair_link,
                    begin_pairing,
                    show_pairing_failures,
                    disconnect,
                    poll_connection_attempts,
                    begin_refresh,
                    poll_refreshes,
                    publish_connection,
                )
                    .chain(),
            );
    }
}

#[derive(Clone)]
struct ConnectionSnapshot {
    view: ConnectionView,
    api: Option<Api>,
    api_generation: u64,
}

#[derive(Default)]
struct ConnectionSubscribers(Vec<Box<dyn FnMut(ConnectionSnapshot)>>);

fn spawn_connection_state(mut commands: Commands) {
    commands.spawn(ConnectionState::default());
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AuthState {
    #[default]
    Loading,
    Paired,
    Unpaired,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct ConnectionView {
    pub(super) auth: AuthState,
    pub(super) pair_url: String,
    pub(super) error: String,
    pub(super) sessions: Vec<RemoteSession>,
    pub(super) agents: Vec<RemoteAgent>,
    pub(super) reachable: bool,
    pub(super) pairing: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ConnectionProjection {
    pub(super) view: Signal<ConnectionView>,
    pub(super) api: Signal<Option<Api>>,
    pub(super) sessions: Signal<Vec<RemoteSession>>,
    pub(super) agents: Signal<Vec<RemoteAgent>>,
}

pub(super) fn use_connection(runtime: RuntimeHandle) -> ConnectionProjection {
    let mut view = use_signal(ConnectionView::default);
    let mut api = use_signal(|| None);
    let mut sessions = use_signal(Vec::new);
    let mut agents = use_signal(Vec::new);
    let mut api_generation = use_signal(|| u64::MAX);

    use_hook(move || {
        runtime.configure_non_send(|subscribers: &mut ConnectionSubscribers| {
            subscribers.0.push(Box::new(move |snapshot| {
                if *view.peek() != snapshot.view {
                    sessions.set(snapshot.view.sessions.clone());
                    agents.set(snapshot.view.agents.clone());
                    view.set(snapshot.view);
                }
                if *api_generation.peek() != snapshot.api_generation {
                    api.set(snapshot.api);
                    api_generation.set(snapshot.api_generation);
                }
            }));
        });
    });

    ConnectionProjection {
        view,
        api,
        sessions,
        agents,
    }
}

#[derive(Message)]
pub(crate) struct PairLinkChanged(pub(crate) String);

#[derive(Message)]
pub(crate) struct PairRequest(pub(crate) String);

#[derive(Message)]
pub(crate) struct PairingFailure(pub(crate) String);

#[derive(Message)]
pub(crate) struct DisconnectRequest;

#[derive(Component)]
pub(super) struct ConnectionState {
    view: ConnectionView,
    api: Option<Api>,
    api_generation: u64,
    operation_generation: u64,
    refresh_at: Instant,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            view: ConnectionView::default(),
            api: None,
            api_generation: 0,
            operation_generation: 0,
            refresh_at: Instant::now() + REFRESH_INTERVAL,
        }
    }
}

impl ConnectionState {
    pub(super) fn api(&self) -> Option<Api> {
        self.api.clone()
    }

    pub(super) fn sessions(&self) -> &[RemoteSession] {
        &self.view.sessions
    }

    pub(super) fn set_sessions(&mut self, sessions: Vec<RemoteSession>) {
        self.view.sessions = sessions;
        self.view.reachable = true;
        self.view.error.clear();
    }

    fn projected(&self) -> ConnectionSnapshot {
        ConnectionSnapshot {
            view: self.view.clone(),
            api: self.api.clone(),
            api_generation: self.api_generation,
        }
    }

    fn replace_api(&mut self, api: Option<Api>) {
        if let Some(displaced) = std::mem::replace(&mut self.api, api) {
            displaced.close();
        }
        self.api_generation = self.api_generation.wrapping_add(1);
    }

    fn clear(&mut self) {
        self.replace_api(None);
        self.operation_generation = self.operation_generation.wrapping_add(1);
        self.view = ConnectionView {
            auth: AuthState::Unpaired,
            ..ConnectionView::default()
        };
    }
}

fn publish_connection(
    state: Single<Ref<ConnectionState>>,
    mut subscribers: NonSendMut<ConnectionSubscribers>,
) {
    if !state.is_changed() {
        return;
    }
    let snapshot = state.projected();
    for subscriber in &mut subscribers.0 {
        subscriber(snapshot.clone());
    }
}

#[derive(Clone, Copy)]
enum ConnectionSource {
    Restore,
    Pair,
}

#[derive(Component)]
struct ConnectionAttempt {
    target: bevy_ecs::entity::Entity,
    generation: u64,
    source: ConnectionSource,
    task: Task<Result<ConnectionAttemptOutput, ApiError>>,
}

struct ConnectionAttemptOutput {
    api: Api,
    sessions: Result<Vec<RemoteSession>, ApiError>,
    agents: Vec<RemoteAgent>,
    credentials: Option<Credentials>,
}

impl ConnectionAttempt {
    fn spawn(
        commands: &mut Commands,
        target: bevy_ecs::entity::Entity,
        generation: u64,
        source: ConnectionSource,
        credentials: Credentials,
    ) {
        let task = IoTaskPool::get().spawn(async move {
            let api = Api::new(credentials)?;
            let sessions = api.sessions().await;
            let agents = if sessions.is_ok() {
                api.agents().await.unwrap_or_default()
            } else {
                Vec::new()
            };
            let credentials = api.paired_credentials().await;
            Ok(ConnectionAttemptOutput {
                api,
                sessions,
                agents,
                credentials,
            })
        });
        commands.spawn(Self {
            target,
            generation,
            source,
            task,
        });
    }
}

#[derive(Component)]
struct ConnectionRefresh {
    target: bevy_ecs::entity::Entity,
    api_generation: u64,
    task: Task<Result<Vec<RemoteSession>, ApiError>>,
}

fn restore_connection(
    mut states: Query<(bevy_ecs::entity::Entity, &mut ConnectionState)>,
    mut commands: Commands,
) {
    let Ok((entity, mut state)) = states.single_mut() else {
        return;
    };
    let Some(credentials) = StoredCredentials::load() else {
        state.view.auth = AuthState::Unpaired;
        return;
    };
    state.view.pair_url = credentials.pairing_url();
    state.operation_generation = state.operation_generation.wrapping_add(1);
    ConnectionAttempt::spawn(
        &mut commands,
        entity,
        state.operation_generation,
        ConnectionSource::Restore,
        credentials,
    );
}

fn change_pair_link(
    mut requests: MessageReader<PairLinkChanged>,
    mut states: Query<&mut ConnectionState>,
) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    for request in requests.read() {
        state.view.pair_url.clone_from(&request.0);
        state.view.error.clear();
    }
}

fn begin_pairing(
    mut requests: MessageReader<PairRequest>,
    mut states: Query<(bevy_ecs::entity::Entity, &mut ConnectionState)>,
    mut leaves: MessageWriter<crate::session::LeaveSession>,
    mut commands: Commands,
) {
    let Ok((entity, mut state)) = states.single_mut() else {
        return;
    };
    for request in requests.read() {
        leaves.write(crate::session::LeaveSession);
        state.replace_api(None);
        state.view.auth = AuthState::Unpaired;
        state.view.pair_url.clone_from(&request.0);
        state.view.error.clear();
        state.view.sessions.clear();
        state.view.agents.clear();
        state.view.reachable = false;
        let credentials = match Credentials::parse(&request.0) {
            Ok(credentials) => credentials,
            Err(message) => {
                state.view.pairing = false;
                state.view.error = message;
                continue;
            }
        };
        state.view.pairing = true;
        state.operation_generation = state.operation_generation.wrapping_add(1);
        ConnectionAttempt::spawn(
            &mut commands,
            entity,
            state.operation_generation,
            ConnectionSource::Pair,
            credentials,
        );
    }
}

fn show_pairing_failures(
    mut failures: MessageReader<PairingFailure>,
    mut states: Query<&mut ConnectionState>,
) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    for failure in failures.read() {
        state.view.error.clone_from(&failure.0);
        state.view.pairing = false;
    }
}

fn disconnect(
    mut requests: MessageReader<DisconnectRequest>,
    mut states: Query<&mut ConnectionState>,
    mut leaves: MessageWriter<crate::session::LeaveSession>,
) {
    let Ok(mut state) = states.single_mut() else {
        return;
    };
    for _ in requests.read() {
        leaves.write(crate::session::LeaveSession);
        StoredCredentials::clear();
        state.clear();
    }
}

fn poll_connection_attempts(
    mut attempts: Query<(bevy_ecs::entity::Entity, &mut ConnectionAttempt)>,
    mut states: Query<&mut ConnectionState>,
    mut commands: Commands,
) {
    for (entity, mut attempt) in &mut attempts {
        let Some(result) = future::block_on(future::poll_once(&mut attempt.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Ok(mut state) = states.get_mut(attempt.target) else {
            continue;
        };
        if attempt.generation != state.operation_generation {
            if let Ok(output) = result {
                output.api.close();
            }
            continue;
        }
        state.view.pairing = false;
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                StoredCredentials::clear();
                state.view.auth = AuthState::Unpaired;
                state.view.error = error.to_string();
                continue;
            }
        };
        match output.sessions {
            Ok(sessions) => {
                if let Some(credentials) = output.credentials {
                    StoredCredentials::save(&credentials);
                }
                state.replace_api(Some(output.api));
                state.view.auth = AuthState::Paired;
                state.view.pair_url.clear();
                state.view.error.clear();
                state.view.sessions = sessions;
                state.view.agents = output.agents;
                state.view.reachable = true;
                state.refresh_at = Instant::now() + REFRESH_INTERVAL;
            }
            Err(ApiError::Unauthorized) => {
                output.api.close();
                StoredCredentials::clear();
                state.view.auth = AuthState::Unpaired;
                state.view.error = match attempt.source {
                    ConnectionSource::Restore => translate("mobile-error-pairing-expired"),
                    ConnectionSource::Pair => translate("mobile-error-token-rejected"),
                };
            }
            Err(error) => match attempt.source {
                ConnectionSource::Restore => {
                    state.replace_api(Some(output.api));
                    state.view.auth = AuthState::Paired;
                    state.view.reachable = false;
                    state.view.error = error.to_string();
                    state.refresh_at = Instant::now() + REFRESH_INTERVAL;
                }
                ConnectionSource::Pair => {
                    output.api.close();
                    state.view.auth = AuthState::Unpaired;
                    state.view.error = error.to_string();
                }
            },
        }
    }
}

fn begin_refresh(
    states: Query<(bevy_ecs::entity::Entity, &ConnectionState)>,
    refreshes: Query<&ConnectionRefresh>,
    mut commands: Commands,
) {
    let Ok((entity, state)) = states.single() else {
        return;
    };
    if state.view.auth != AuthState::Paired || Instant::now() < state.refresh_at {
        return;
    }
    if refreshes.iter().any(|refresh| refresh.target == entity) {
        return;
    }
    let Some(api) = state.api.clone() else {
        return;
    };
    let task = IoTaskPool::get().spawn(async move { api.sessions().await });
    commands.spawn(ConnectionRefresh {
        target: entity,
        api_generation: state.api_generation,
        task,
    });
}

fn poll_refreshes(
    mut refreshes: Query<(bevy_ecs::entity::Entity, &mut ConnectionRefresh)>,
    mut states: Query<&mut ConnectionState>,
    mut leaves: MessageWriter<crate::session::LeaveSession>,
    mut commands: Commands,
) {
    for (entity, mut refresh) in &mut refreshes {
        let Some(result) = future::block_on(future::poll_once(&mut refresh.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Ok(mut state) = states.get_mut(refresh.target) else {
            continue;
        };
        if refresh.api_generation != state.api_generation {
            continue;
        }
        state.refresh_at = Instant::now() + REFRESH_INTERVAL;
        match result {
            Ok(sessions) => {
                state.view.sessions = sessions;
                state.view.reachable = true;
                state.view.error.clear();
            }
            Err(ApiError::Unauthorized) => {
                leaves.write(crate::session::LeaveSession);
                StoredCredentials::clear();
                state.clear();
                state.view.error = translate("mobile-error-pairing-expired");
            }
            Err(error) => {
                state.view.reachable = false;
                state.view.error = error.to_string();
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct Credentials {
    pub(crate) base_url: String,
    #[serde(alias = "token")]
    pub(crate) relay_token: String,
    #[serde(default)]
    pub(crate) credential: Option<ClientCredential>,
    #[serde(default)]
    pub(crate) client_id: DeviceId,
    #[serde(default)]
    pub(crate) fingerprint: String,
    #[serde(default)]
    pub(crate) device: String,
}

impl Credentials {
    pub(crate) fn endpoint(&self) -> Option<crate::quic::Endpoint> {
        if self.fingerprint.is_empty()
            || self.device.is_empty()
            || self.client_id.as_str().is_empty()
        {
            return None;
        }
        let credential = self.credential.clone()?;
        let parsed = Url::parse(&self.base_url).ok()?;
        let host = parsed.host_str()?;
        let port = parsed.port().unwrap_or(443);
        Some(crate::quic::Endpoint {
            address: format!("{host}:{port}"),
            relay_token: self.relay_token.clone(),
            credential,
            client_id: self.client_id.clone(),
            fingerprint: self.fingerprint.clone(),
            desktop: vmux_transport::DeviceId::new(&self.device),
        })
    }

    pub(crate) fn parse(input: &str) -> Result<Credentials, String> {
        let input = input.trim();
        if input.starts_with("vmux://") {
            let parsed = Url::parse(input).map_err(|_| translate("mobile-url-invalid"))?;
            if parsed.scheme() != "vmux" || parsed.host_str() != Some("pair") {
                return Err(translate("mobile-url-invalid"));
            }
            let params = parsed
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            let base_url = params
                .get("base")
                .map(|value| value.to_string())
                .ok_or_else(|| translate("mobile-url-no-address"))?;
            let relay_token = params
                .get("relay_token")
                .or_else(|| params.get("token"))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| translate("mobile-url-no-token"))?;
            let pairing_token = params
                .get("pairing_token")
                .or_else(|| params.get("token"))
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| translate("mobile-url-no-token"))?;
            let base = Url::parse(&base_url).map_err(|_| translate("mobile-url-bad-address"))?;
            if !matches!(base.scheme(), "http" | "https") {
                return Err(translate("mobile-url-scheme"));
            }
            let fingerprint = params
                .get("fp")
                .map(|value| value.to_string())
                .unwrap_or_default();
            let device = params
                .get("device")
                .map(|value| value.to_string())
                .unwrap_or_default();
            let base_url = normalized_pairing_base(base)?;
            if base_url.is_empty() {
                return Err(translate("mobile-url-no-address"));
            }
            return Ok(Credentials {
                base_url,
                relay_token,
                credential: Some(ClientCredential::Pairing(pairing_token)),
                client_id: DeviceId::new(uuid::Uuid::new_v4().simple().to_string()),
                fingerprint,
                device,
            });
        }
        let start = input
            .find("https://")
            .or_else(|| input.find("http://"))
            .ok_or_else(|| translate("mobile-url-paste-full"))?;
        let candidate = input[start..].split_whitespace().next().unwrap_or_default();
        let parsed = Url::parse(candidate).map_err(|_| translate("mobile-url-invalid"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(translate("mobile-url-scheme"));
        }
        let relay_token = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "relay_token" || name == "token")
                    .map(|(_, value)| value.into_owned())
            })
            .filter(|token| !token.is_empty())
            .ok_or_else(|| translate("mobile-url-no-token"))?;
        let pairing_token = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "pairing_token" || name == "token")
                    .map(|(_, value)| value.into_owned())
            })
            .filter(|token| !token.is_empty())
            .ok_or_else(|| translate("mobile-url-no-token"))?;
        let fingerprint = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "fp")
                    .map(|(_, value)| value.into_owned())
            })
            .unwrap_or_default();
        let device = parsed
            .fragment()
            .and_then(|fragment| {
                url::form_urlencoded::parse(fragment.as_bytes())
                    .find(|(name, _)| name == "device")
                    .map(|(_, value)| value.into_owned())
            })
            .unwrap_or_default();
        let base_url = normalized_pairing_base(parsed)?;
        if base_url.is_empty() {
            return Err(translate("mobile-url-no-address"));
        }
        Ok(Credentials {
            base_url,
            relay_token,
            credential: Some(ClientCredential::Pairing(pairing_token)),
            client_id: DeviceId::new(uuid::Uuid::new_v4().simple().to_string()),
            fingerprint,
            device,
        })
    }

    pub(crate) fn pairing_url(&self) -> String {
        let Some(ClientCredential::Pairing(pairing_token)) = &self.credential else {
            return String::new();
        };
        let fragment = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("relay_token", &self.relay_token)
            .append_pair("pairing_token", pairing_token)
            .append_pair("fp", &self.fingerprint)
            .append_pair("device", &self.device)
            .finish();
        format!("{base}/#{fragment}", base = self.base_url)
    }
}

fn normalized_pairing_base(mut url: Url) -> Result<String, String> {
    url.set_fragment(None);
    url.set_query(None);
    if url.origin().ascii_serialization() == "null" {
        return Ok(String::new());
    }
    let mut value = url.to_string();
    while value.ends_with('/') {
        value.pop();
    }
    Ok(value)
}

#[derive(Props, Clone, PartialEq)]
pub(super) struct PairCardProps {
    pub(super) value: String,
    pub(super) error: String,
    pub(super) pairing: bool,
    pub(super) on_value: EventHandler<String>,
    pub(super) on_pair: EventHandler<()>,
    pub(super) on_scan: EventHandler<()>,
}

#[component]
pub(super) fn PairCard(props: PairCardProps) -> Element {
    let unavailable = use_hook(|| qr_scanner::ScannerSupport::detect().unavailable());
    let mut show_link = use_signal(|| unavailable.is_some() || !props.value.trim().is_empty());

    rsx! {
        div { class: "w-full",
            div { class: "mb-5 text-center",
                h2 { class: "text-base font-semibold text-foreground", {translate("mobile-pair-title")} }
                p { class: "mt-1 text-xs leading-5 text-muted-foreground", {translate("mobile-pair-subtitle")} }
            }
            button {
                class: "flex h-14 w-full items-center justify-center gap-2.5 rounded-2xl bg-primary text-sm font-semibold text-primary-foreground shadow-xl shadow-black/20 disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none active:scale-[0.99] active:bg-primary/90",
                r#type: "button",
                disabled: unavailable.is_some(),
                onclick: move |_| props.on_scan.call(()),
                svg {
                    class: "h-5 w-5",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M3 5a2 2 0 0 1 2-2h2" }
                    path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                    path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                    path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                    rect { width: "5", height: "5", x: "7", y: "7", rx: "1" }
                    path { d: "M17 7v.01" }
                    path { d: "M17 12v5" }
                    path { d: "M12 17h5" }
                }
                {translate("mobile-pair-scan")}
            }
            button {
                class: "mx-auto mt-4 block rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground active:bg-accent active:text-accent-foreground",
                r#type: "button",
                onclick: move |_| show_link.set(!show_link()),
                {if show_link() { translate("mobile-pair-hide-link") } else { translate("mobile-pair-show-link") }}
            }
            if let Some(reason) = unavailable.clone() {
                p { class: "mt-3 text-center text-xs leading-5 text-muted-foreground", "{reason}" }
            }
            if show_link() {
                form {
                    class: "mt-2 flex items-center gap-2 rounded-2xl border border-border bg-muted p-1.5",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-10 min-w-0 flex-1 bg-transparent px-3 font-mono text-base text-foreground outline-none placeholder:text-muted-foreground",
                        r#type: "url",
                        autofocus: unavailable.is_some(),
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: translate("mobile-pair-link-placeholder"),
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    button {
                        class: "flex h-10 shrink-0 items-center gap-2 rounded-xl bg-secondary px-4 text-xs font-semibold text-secondary-foreground disabled:opacity-70 active:bg-secondary/80",
                        r#type: "submit",
                        disabled: props.pairing,
                        if props.pairing {
                            span { class: "size-3.5 animate-spin rounded-full border-2 border-secondary-foreground/25 border-t-secondary-foreground motion-reduce:animate-none" }
                        }
                        {if props.pairing { translate("mobile-pair-connecting") } else { translate("mobile-pair-connect") }}
                    }
                }
            }
            if !props.error.is_empty() {
                p { class: "mt-3 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs leading-5 text-destructive", "{props.error}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::Api;

    #[test]
    fn a_pairing_link_carries_the_certificate_fingerprint() {
        let expected = "c620a502885ddf230420184cc3a1b190792c14c1049ab76a6a63596054a1025e";

        let pasted = Credentials::parse(&format!(
            "https://mac.example.ts.net/#token=secret&fp={expected}"
        ))
        .unwrap();
        let deep_link = Credentials::parse(&format!(
            "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net&token=secret&fp={expected}"
        ))
        .unwrap();

        assert_eq!(pasted.fingerprint, expected);
        assert_eq!(deep_link.fingerprint, expected);
        assert_eq!(pasted.relay_token, "secret");
        assert_eq!(
            pasted.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
    }

    #[test]
    fn a_link_without_a_fingerprint_parses_but_cannot_be_dialled() {
        let credentials = Credentials::parse("https://mac.example.ts.net/#token=secret").unwrap();

        assert!(credentials.fingerprint.is_empty());
        assert_eq!(credentials.relay_token, "secret");
        assert!(
            Api::new(credentials).is_err(),
            "an unpinned pairing must be refused, not silently downgraded"
        );
    }

    #[test]
    fn a_written_pairing_preserves_transport_credentials() {
        let original = Credentials {
            base_url: "https://mac.example.ts.net".to_string(),
            relay_token: "relay-secret".to_string(),
            credential: Some(ClientCredential::Pairing("pairing-secret".to_string())),
            client_id: DeviceId::new("phone-a"),
            fingerprint: "c620a502885ddf230420184cc3a1b190".to_string(),
            device: "device-1".to_string(),
        };
        let parsed = Credentials::parse(&original.pairing_url()).unwrap();

        assert_eq!(parsed.base_url, original.base_url);
        assert_eq!(parsed.relay_token, original.relay_token);
        assert_eq!(parsed.credential, original.credential);
        assert_eq!(parsed.fingerprint, original.fingerprint);
        assert_eq!(parsed.device, original.device);
        assert!(!parsed.client_id.as_str().is_empty());
        assert_ne!(parsed.client_id, original.client_id);
    }

    #[test]
    fn parses_pairing_url() {
        let credentials =
            Credentials::parse("paste into Vmux: https://mac.example.ts.net/#token=secret")
                .unwrap();

        assert_eq!(credentials.base_url, "https://mac.example.ts.net");
        assert_eq!(credentials.relay_token, "secret");
        assert_eq!(
            credentials.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
        assert!(credentials.fingerprint.is_empty());
        assert!(credentials.device.is_empty());
        assert!(!credentials.client_id.as_str().is_empty());
    }

    #[test]
    fn parses_pairing_deep_link() {
        let credentials = Credentials::parse(
            "vmux://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret",
        )
        .unwrap();

        assert_eq!(credentials.base_url, "https://mac.example.ts.net:54821");
        assert_eq!(credentials.relay_token, "secret");
        assert_eq!(
            credentials.credential,
            Some(ClientCredential::Pairing("secret".to_string()))
        );
        assert!(credentials.fingerprint.is_empty());
        assert!(credentials.device.is_empty());
    }

    #[test]
    fn pairing_url_preserves_relay_path() {
        let credentials =
            Credentials::parse("http://localhost:8787/r/device-1/#token=secret").unwrap();

        assert_eq!(credentials.base_url, "http://localhost:8787/r/device-1");
        assert_eq!(credentials.relay_token, "secret");
    }
}
