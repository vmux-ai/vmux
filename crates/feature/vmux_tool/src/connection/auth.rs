use super::transport::{
    AuthorizationServer, ClientRegistration, Http, McpCallback as CallbackListener,
    ProtectedResource, TokenResponse,
};
use super::{McpAuthorizationUrl, McpCatalogServer, McpCompletion, PendingMcpOperation};
use crate::storage::{ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult};
use crate::{ToolStore, ToolsManifest};
use bevy::prelude::*;
use ring::digest::{SHA256, digest};
use std::time::{Duration, Instant};
use vmux_ecs::profile::McpCredentials;
use vmux_ecs::{EntityTarget, IoTask, IoTaskPlugin, IoTaskSet, OperationCompleted, TaskError};
use vmux_profile::mcp_credentials::{McpCredentialTransaction, McpOauthCredentials};

pub(super) struct AuthPlugin;

impl Plugin for AuthPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(IoTaskPlugin::<(
            StoreLease,
            CallbackListener,
            Http,
            ProtectedResource,
            AuthorizationServer,
            ClientRegistration,
            Code,
            TokenResponse,
            CredentialLease,
            OriginalCredential,
            CredentialWritten,
            CredentialRestored,
        )>::default())
            .configure_sets(Update, IoTaskSet::all().before(AuthSet))
            .add_systems(Update, begin.before(AuthSet))
            .add_systems(
                Update,
                (
                    inspect,
                    bind,
                    client,
                    protected,
                    authorization,
                    register,
                    consent,
                    callback,
                    exchange,
                    store_lock,
                    credential_lock,
                    original_manifest,
                    original_credential,
                    prepare_commit,
                    credential_write,
                    manifest_write,
                    rollback,
                    finish,
                )
                    .chain()
                    .in_set(AuthSet),
            );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct AuthSet;

#[derive(Component)]
pub(super) struct Active;

#[derive(Component, Clone)]
pub(super) struct Credentials(pub(super) McpCredentials);

#[derive(Component, Clone)]
pub(super) struct Source {
    pub(super) generation: u64,
    pub(super) id: String,
    pub(super) owner: Entity,
    pub(super) disconnect: bool,
}

#[derive(Component)]
struct PhaseState(Phase);

enum Phase {
    Inspect,
    Bind,
    Client,
    Protected,
    Authorization,
    Register,
    Consent,
    Callback,
    Exchange,
    StoreLock,
    CredentialLock,
    OriginalManifest,
    OriginalCredential,
    Prepare,
    CredentialWrite,
    ManifestWrite,
    CredentialRollback,
    ManifestRollback,
    Complete,
}

type Idle = (
    (
        Without<IoTask<CallbackListener>>,
        Without<IoTask<Http>>,
        Without<IoTask<ProtectedResource>>,
        Without<IoTask<AuthorizationServer>>,
        Without<IoTask<ClientRegistration>>,
        Without<IoTask<Code>>,
    ),
    (
        Without<IoTask<TokenResponse>>,
        Without<IoTask<StoreLease>>,
        Without<IoTask<CredentialLease>>,
        Without<IoTask<OriginalCredential>>,
        Without<IoTask<CredentialWritten>>,
        Without<IoTask<CredentialRestored>>,
    ),
);

#[derive(Component)]
struct PrimaryFailure(String);

type BeginRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PendingMcpOperation,
        &'static EntityTarget<McpCatalogServer>,
    ),
    Without<Active>,
>;

fn begin(
    requests: BeginRequests<'_, '_>,
    servers: Query<&McpCatalogServer>,
    credentials: Query<(
        Entity,
        &McpCredentials,
        Option<&vmux_ecs::profile::ProfileDirectories>,
    )>,
    mut commands: Commands,
) {
    for (entity, request, server) in &requests {
        if request.disconnect {
            continue;
        }
        let current = credentials.single().ok();
        let selected = servers.get(server.entity()).ok();
        let error = if current.is_none_or(|(owner, _, _)| request.owner != owner) {
            Some("MCP source Profile changed")
        } else if selected.is_none() {
            Some("MCP server is no longer available")
        } else {
            None
        };
        if let Some(error) = error {
            commands
                .entity(entity)
                .remove::<PendingMcpOperation>()
                .insert(McpCompletion {
                    id: request.id.clone(),
                    disconnect: request.disconnect,
                    success: false,
                    message: error.into(),
                });
            commands.trigger(OperationCompleted {
                entity,
                result: Err(error.into()),
            });
            continue;
        }
        let (owner, credentials, directories) = current.unwrap();
        let server = selected.unwrap();
        let store = match directories {
            Some(paths) => ToolStore::new(paths.0.config(), paths.0.home()),
            None => ToolStore::current(),
        };
        commands
            .entity(entity)
            .remove::<PendingMcpOperation>()
            .insert((
                Active,
                Source {
                    generation: request.generation,
                    id: request.id.clone(),
                    owner,
                    disconnect: request.disconnect,
                },
                server.clone(),
                Credentials(credentials.clone()),
                store,
                PhaseState(Phase::Inspect),
            ));
    }
}

fn inspect(
    requests: Query<
        (
            Entity,
            &PhaseState,
            &McpCatalogServer,
            Option<&ReadResult>,
            Option<&ReadRequest>,
        ),
        (With<Active>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, phase, server, result, pending) in &requests {
        if !matches!(phase.0, Phase::Inspect) {
            continue;
        }
        if let Some(result) = result {
            let error = match &result.0 {
                Err(error) => Some(error.clone()),
                Ok(manifest)
                    if manifest
                        .mcp
                        .servers
                        .get(&server.id)
                        .is_some_and(|configured| !server.owns(configured)) =>
                {
                    Some(format!(
                        "MCP server ID is already managed by tools.toml: {}",
                        server.id
                    ))
                }
                Ok(_) => None,
            };
            commands.entity(entity).remove::<ReadResult>();
            if let Some(error) = error {
                commands
                    .entity(entity)
                    .insert((PrimaryFailure(error), PhaseState(Phase::Complete)));
            } else {
                commands.entity(entity).insert(PhaseState(Phase::Bind));
            }
        } else if pending.is_none() {
            commands.entity(entity).insert(ReadRequest {
                retain_lease: false,
            });
        }
    }
}

type BindRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        Option<&'static CallbackListener>,
        Option<&'static TaskError<CallbackListener>>,
    ),
    (With<Active>, Idle),
>;

fn bind(requests: BindRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, result, error) in &requests {
        if !matches!(phase.0, Phase::Bind) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Client));
        } else {
            commands
                .entity(entity)
                .insert(IoTask::spawn(async { CallbackListener::bind() }));
        }
    }
}

type ClientRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        Option<&'static Http>,
        Option<&'static TaskError<Http>>,
    ),
    (With<Active>, Idle),
>;

fn client(requests: ClientRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, result, error) in &requests {
        if !matches!(phase.0, Phase::Client) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Protected));
        } else {
            commands
                .entity(entity)
                .insert(IoTask::spawn(async { Http::new() }));
        }
    }
}

type ProtectedRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Http,
        &'static McpCatalogServer,
        Option<&'static ProtectedResource>,
        Option<&'static TaskError<ProtectedResource>>,
    ),
    (With<Active>, Idle),
>;

fn protected(requests: ProtectedRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, http, server, result, error) in &requests {
        if !matches!(phase.0, Phase::Protected) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::Authorization));
        } else {
            let http = http.clone();
            let resource = server.url.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                http.protected_metadata(&resource)
            }));
        }
    }
}

type AuthorizationRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Http,
        &'static ProtectedResource,
        Option<&'static AuthorizationServer>,
        Option<&'static TaskError<AuthorizationServer>>,
    ),
    (With<Active>, Idle),
>;

fn authorization(requests: AuthorizationRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, http, protected, result, error) in &requests {
        if !matches!(phase.0, Phase::Authorization) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
            continue;
        }
        if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Register));
            continue;
        }
        let Some(issuer) = protected.authorization_servers.first() else {
            commands.entity(entity).insert((
                PrimaryFailure("MCP server did not advertise an authorization server".into()),
                PhaseState(Phase::Complete),
            ));
            continue;
        };
        let issuer = issuer.clone();
        let http = http.clone();
        commands.entity(entity).insert(IoTask::spawn(async move {
            http.authorization_metadata(&issuer)
        }));
    }
}

type RegisterRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Http,
        &'static AuthorizationServer,
        &'static CallbackListener,
        Option<&'static ClientRegistration>,
        Option<&'static TaskError<ClientRegistration>>,
    ),
    (With<Active>, Idle),
>;

fn register(requests: RegisterRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, http, authorization, callback, result, error) in &requests {
        if !matches!(phase.0, Phase::Register) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Consent));
        } else {
            let http = http.clone();
            let authorization = authorization.clone();
            let redirect = callback.redirect.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                http.register(&authorization, &redirect)
            }));
        }
    }
}

#[derive(Component)]
struct Pkce {
    verifier: String,
    deadline: Instant,
}

#[derive(Component, Clone)]
pub(super) struct CallbackState(pub(super) String);

impl Pkce {
    fn encode_challenge(bytes: &[u8]) -> String {
        let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let first = chunk[0];
            let second = chunk.get(1).copied().unwrap_or_default();
            let third = chunk.get(2).copied().unwrap_or_default();
            encoded.push(table[(first >> 2) as usize] as char);
            encoded.push(table[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
            if chunk.len() > 1 {
                encoded.push(table[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
            }
            if chunk.len() > 2 {
                encoded.push(table[(third & 0b111111) as usize] as char);
            }
        }
        encoded
    }
}

type ConsentRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static McpCatalogServer,
        &'static AuthorizationServer,
        &'static ClientRegistration,
        &'static CallbackListener,
    ),
    (With<Active>, Idle),
>;

fn consent(requests: ConsentRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, server, authorization, registration, callback) in &requests {
        if !matches!(phase.0, Phase::Consent) {
            continue;
        }
        let verifier = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let challenge = Pkce::encode_challenge(digest(&SHA256, verifier.as_bytes()).as_ref());
        let state = uuid::Uuid::new_v4().simple().to_string();
        let mut url = match Http::https_url(
            &authorization.authorization_endpoint,
            "OAuth authorization endpoint",
        ) {
            Ok(url) => url,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((PrimaryFailure(error), PhaseState(Phase::Complete)));
                continue;
            }
        };
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &registration.client_id)
            .append_pair("redirect_uri", &callback.redirect)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &server.url)
            .append_pair("scope", &server.scopes.join(" "));
        commands.entity(entity).insert((
            Pkce {
                verifier,
                deadline: Instant::now() + Duration::from_secs(300),
            },
            CallbackState(state),
            McpAuthorizationUrl {
                generation: source.generation,
                url: url.to_string(),
            },
            PhaseState(Phase::Callback),
        ));
    }
}

#[derive(Component)]
struct Code(Option<String>);

type CallbackRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static CallbackListener,
        &'static Pkce,
        &'static CallbackState,
        Option<&'static Code>,
        Option<&'static TaskError<Code>>,
    ),
    (With<Active>, Idle),
>;

fn callback(requests: CallbackRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, callback, pkce, state, result, error) in &requests {
        if !matches!(phase.0, Phase::Callback) {
            continue;
        }
        if Instant::now() >= pkce.deadline {
            commands.entity(entity).insert((
                PrimaryFailure("OAuth authorization timed out".into()),
                PhaseState(Phase::Complete),
            ));
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some_and(|result| result.0.is_some()) {
            commands.entity(entity).insert(PhaseState(Phase::Exchange));
        } else {
            let delay = if result.is_some() {
                Duration::from_millis(50)
            } else {
                Duration::ZERO
            };
            let listener = match callback.try_clone() {
                Ok(listener) => listener,
                Err(error) => {
                    commands.entity(entity).insert((
                        PrimaryFailure(error.to_string()),
                        PhaseState(Phase::Complete),
                    ));
                    continue;
                }
            };
            let state = state.clone();
            commands
                .entity(entity)
                .remove::<Code>()
                .insert(IoTask::spawn(async move {
                    std::thread::sleep(delay);
                    listener.receive_once(&state).map(Code)
                }));
        }
    }
}

type ExchangeRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Http,
        &'static AuthorizationServer,
        &'static ClientRegistration,
        &'static CallbackListener,
        &'static Pkce,
        &'static Code,
        &'static McpCatalogServer,
        Option<&'static TokenResponse>,
        Option<&'static TaskError<TokenResponse>>,
    ),
    (With<Active>, Idle),
>;

fn exchange(requests: ExchangeRequests<'_, '_>, mut commands: Commands) {
    for (
        entity,
        phase,
        http,
        authorization,
        registration,
        callback,
        pkce,
        code,
        server,
        result,
        error,
    ) in &requests
    {
        if !matches!(phase.0, Phase::Exchange) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::StoreLock));
        } else {
            let http = http.clone();
            let authorization = authorization.clone();
            let registration = registration.clone();
            let redirect = callback.redirect.clone();
            let verifier = pkce.verifier.clone();
            let code = code.0.as_ref().unwrap().clone();
            let resource = server.url.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                http.exchange(
                    &authorization.token_endpoint,
                    &registration,
                    &code,
                    &verifier,
                    &redirect,
                    &resource,
                )
            }));
        }
    }
}

#[derive(Component)]
struct CredentialLease {
    _lease: McpCredentialTransaction,
}
#[derive(Component)]
struct OriginalManifest(Option<ToolsManifest>);
#[derive(Component)]
struct OriginalCredential(Option<McpOauthCredentials>);
#[derive(Component)]
struct CredentialWritten;
#[derive(Component)]
struct CredentialRestored;

#[derive(Component, Clone)]
struct ProposedCredential(McpOauthCredentials);

type StoreLockRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static ToolStore,
        Option<&'static StoreLease>,
        Option<&'static TaskError<StoreLease>>,
    ),
    (With<Active>, Idle),
>;

fn store_lock(requests: StoreLockRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, store, result, error) in &requests {
        if !matches!(phase.0, Phase::StoreLock) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::CredentialLock));
        } else {
            let store = store.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                Ok::<_, String>(StoreLease {
                    _lease: store.lease_async().await,
                })
            }));
        }
    }
}

type CredentialLockRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Credentials,
        Option<&'static CredentialLease>,
        Option<&'static TaskError<CredentialLease>>,
    ),
    (With<Active>, Idle),
>;

fn credential_lock(requests: CredentialLockRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, credentials, result, error) in &requests {
        if !matches!(phase.0, Phase::CredentialLock) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::OriginalManifest));
        } else {
            let access = credentials.0.access.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                access
                    .transaction()
                    .await
                    .map(|lease| CredentialLease { _lease: lease })
            }));
        }
    }
}

fn original_manifest(
    requests: Query<
        (
            Entity,
            &PhaseState,
            Option<&ReadResult>,
            Option<&ReadRequest>,
        ),
        (With<Active>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, phase, result, pending) in &requests {
        if !matches!(phase.0, Phase::OriginalManifest) {
            continue;
        }
        if let Some(result) = result {
            commands.entity(entity).remove::<ReadResult>();
            match &result.0 {
                Ok(manifest) => {
                    commands.entity(entity).insert((
                        OriginalManifest(Some(manifest.clone())),
                        PhaseState(Phase::OriginalCredential),
                    ));
                }
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((PrimaryFailure(error.clone()), PhaseState(Phase::Complete)));
                }
            }
        } else if pending.is_none() {
            commands
                .entity(entity)
                .insert(ReadRequest { retain_lease: true });
        }
    }
}

type OriginalCredentialRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static Credentials,
        Option<&'static OriginalCredential>,
        Option<&'static TaskError<OriginalCredential>>,
    ),
    (With<Active>, Idle),
>;

fn original_credential(requests: OriginalCredentialRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, credentials, result, error) in &requests {
        if !matches!(phase.0, Phase::OriginalCredential) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::Complete),
            ));
        } else if result.is_some() {
            commands.entity(entity).insert(PhaseState(Phase::Prepare));
        } else {
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                storage.load(&id).map(OriginalCredential)
            }));
        }
    }
}

type PrepareCommitRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static McpCatalogServer,
        &'static OriginalManifest,
        Option<&'static AuthorizationServer>,
        Option<&'static ClientRegistration>,
        Option<&'static TokenResponse>,
    ),
    (With<Active>, Idle),
>;

fn prepare_commit(
    requests: PrepareCommitRequests<'_, '_>,
    current: Query<Entity, With<McpCredentials>>,
    mut commands: Commands,
) {
    for (entity, phase, source, server, original, authorization, registration, token) in &requests {
        if !matches!(phase.0, Phase::Prepare) {
            continue;
        }
        if current.single().ok() != Some(source.owner) {
            commands.entity(entity).insert((
                PrimaryFailure("MCP source Profile changed".into()),
                PhaseState(Phase::Complete),
            ));
            continue;
        }
        let original = original.0.as_ref().unwrap();
        if original
            .mcp
            .servers
            .get(&source.id)
            .is_some_and(|configured| !server.owns(configured))
        {
            commands.entity(entity).insert((
                PrimaryFailure(format!(
                    "MCP server ID is already managed by tools.toml: {}",
                    source.id
                )),
                PhaseState(Phase::Complete),
            ));
            continue;
        }
        let (Some(authorization), Some(registration), Some(token)) =
            (authorization, registration, token)
        else {
            continue;
        };
        commands.entity(entity).insert((
            ProposedCredential(McpOauthCredentials {
                token_endpoint: authorization.token_endpoint.clone(),
                client_id: registration.client_id.clone(),
                client_secret: registration.client_secret.clone(),
                access_token: token.access_token.clone(),
                refresh_token: token.refresh_token.clone(),
                expires_at: McpOauthCredentials::expires_at(token.expires_in),
                scope: token
                    .scope
                    .clone()
                    .unwrap_or_else(|| server.scopes.join(" ")),
                resource: server.url.clone(),
            }),
            PhaseState(Phase::CredentialWrite),
        ));
    }
}

type CredentialWriteRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        &'static Credentials,
        &'static ProposedCredential,
        Option<&'static CredentialWritten>,
        Option<&'static TaskError<CredentialWritten>>,
    ),
    (With<Active>, Idle),
>;

fn credential_write(requests: CredentialWriteRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, credentials, proposed, result, error) in &requests {
        if !matches!(phase.0, Phase::CredentialWrite) {
            continue;
        }
        if let Some(error) = error {
            commands.entity(entity).insert((
                PrimaryFailure(error.to_string()),
                PhaseState(Phase::CredentialRollback),
            ));
        } else if result.is_some() {
            commands
                .entity(entity)
                .insert(PhaseState(Phase::ManifestWrite));
        } else {
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            let proposed = proposed.0.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                storage.store(&id, &proposed).map(|_| CredentialWritten)
            }));
        }
    }
}

fn manifest_write(
    requests: Query<
        (
            Entity,
            &PhaseState,
            &Source,
            &McpCatalogServer,
            &OriginalManifest,
            Option<&WriteResult>,
            Option<&WriteRequest>,
        ),
        (With<Active>, Idle),
    >,
    mut commands: Commands,
) {
    for (entity, phase, source, server, original, result, pending) in &requests {
        if !matches!(phase.0, Phase::ManifestWrite) {
            continue;
        }
        if let Some(result) = result {
            commands.entity(entity).remove::<WriteResult>();
            match &result.0 {
                Ok(()) => {
                    commands.entity(entity).insert(PhaseState(Phase::Complete));
                }
                Err(error) => {
                    commands.entity(entity).insert((
                        PrimaryFailure(error.clone()),
                        PhaseState(Phase::ManifestRollback),
                    ));
                }
            }
        } else if pending.is_none() {
            let mut manifest = original.0.as_ref().unwrap().clone();
            manifest
                .mcp
                .servers
                .insert(source.id.clone(), server.manifest());
            commands.entity(entity).insert(WriteRequest {
                manifest,
                retain_lease: true,
            });
        }
    }
}

fn rollback(
    requests: Query<
        (
            Entity,
            &PhaseState,
            &Source,
            &Credentials,
            &OriginalManifest,
            &OriginalCredential,
            Option<&WriteResult>,
            Option<&WriteRequest>,
            Option<&CredentialRestored>,
            Option<&TaskError<CredentialRestored>>,
        ),
        (With<Active>, Idle),
    >,
    mut failures: Query<&mut PrimaryFailure>,
    mut commands: Commands,
) {
    for (
        entity,
        phase,
        source,
        credentials,
        original,
        old,
        manifest,
        pending,
        credential_done,
        credential_error,
    ) in &requests
    {
        if matches!(phase.0, Phase::ManifestRollback) {
            if let Some(result) = manifest {
                commands.entity(entity).remove::<WriteResult>();
                if let Err(error) = &result.0
                    && let Ok(mut failure) = failures.get_mut(entity)
                {
                    failure
                        .0
                        .push_str(&format!("; manifest rollback failed: {error}"));
                }
                commands
                    .entity(entity)
                    .insert(PhaseState(Phase::CredentialRollback));
            } else if pending.is_none() {
                commands.entity(entity).insert(WriteRequest {
                    manifest: original.0.as_ref().unwrap().clone(),
                    retain_lease: true,
                });
            }
        } else if matches!(phase.0, Phase::CredentialRollback) {
            if credential_done.is_some() || credential_error.is_some() {
                if let Some(error) = credential_error
                    && let Ok(mut failure) = failures.get_mut(entity)
                {
                    failure
                        .0
                        .push_str(&format!("; credential rollback failed: {error}"));
                }
                commands.entity(entity).insert(PhaseState(Phase::Complete));
                continue;
            }
            let id = source.id.clone();
            let storage = credentials.0.storage.clone();
            let old = old.0.clone();
            commands.entity(entity).insert(IoTask::spawn(async move {
                let result = match old {
                    Some(old) => storage.store(&id, &old),
                    None => storage.remove(&id),
                };
                result.map(|_| CredentialRestored)
            }));
        }
    }
}

type FinishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PhaseState,
        &'static Source,
        Option<&'static PrimaryFailure>,
    ),
    (With<Active>, Idle),
>;

fn finish(requests: FinishRequests<'_, '_>, mut commands: Commands) {
    for (entity, phase, source, failure) in &requests {
        if !matches!(phase.0, Phase::Complete) {
            continue;
        }
        let message = failure.map(|failure| failure.0.clone()).unwrap_or_default();
        let success = failure.is_none();
        commands
            .entity(entity)
            .insert(McpCompletion {
                id: source.id.clone(),
                disconnect: source.disconnect,
                success,
                message: message.clone(),
            })
            .remove::<(
                Active,
                PhaseState,
                CallbackListener,
                CallbackState,
                Http,
                Pkce,
                Code,
            )>();
        commands.entity(entity).remove::<(
            ProtectedResource,
            AuthorizationServer,
            ClientRegistration,
            TokenResponse,
            ProposedCredential,
        )>();
        commands.entity(entity).remove::<(
            OriginalManifest,
            OriginalCredential,
            StoreLease,
            CredentialLease,
        )>();
        commands
            .entity(entity)
            .remove::<(ToolStore, Credentials, McpCatalogServer)>();
        commands.trigger(OperationCompleted {
            entity,
            result: if success { Ok(()) } else { Err(message) },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::{IoTaskPool, TaskPoolBuilder, futures_lite::future};
    use std::io::{Read, Write};
    use vmux_ecs::profile::mcp_credentials::McpCredentialAccess;
    use vmux_ecs::{Operation, OperationRetained, OperationState, OperationTarget};

    #[test]
    fn pkce_challenge_matches_the_rfc7636_example_without_base64_padding() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            Pkce::encode_challenge(digest(&SHA256, verifier.as_bytes()).as_ref()),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    struct CompletionWake(std::sync::mpsc::Sender<()>);

    impl CompletionWake {
        fn install(self, app: &mut App) {
            app.insert_resource(vmux_ecs::JobWake(std::sync::Arc::new(self).into()));
        }
    }

    impl std::task::Wake for CompletionWake {
        fn wake(self: std::sync::Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    #[test]
    fn pending_io_wakes_the_reactive_host_before_the_next_input() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let (notify, notified) = std::sync::mpsc::channel();
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        CompletionWake(notify).install(&mut app);
        let (release, wait) = std::sync::mpsc::channel();
        let operation = app
            .world_mut()
            .spawn(IoTask::spawn(async move {
                wait.recv().unwrap();
                Ok::<_, String>(CredentialWritten)
            }))
            .id();
        notified
            .recv_timeout(Duration::from_secs(3))
            .expect("registered IO did not wake the idle host");
        app.update();
        assert!(notified.try_recv().is_err());
        release.send(()).unwrap();
        notified
            .recv_timeout(Duration::from_secs(3))
            .expect("completed IO did not wake the idle host");
        app.update();
        assert!(app.world().get::<CredentialWritten>(operation).is_some());
        assert!(
            app.world()
                .get::<IoTask<CredentialWritten>>(operation)
                .is_none()
        );
    }

    #[test]
    fn profile_removal_refuses_pending_connect_and_disconnect_and_releases_claims() {
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        for disconnect in [false, true] {
            let owner = app.world_mut().spawn_empty().id();
            let page = app
                .world_mut()
                .spawn(super::super::McpPageState {
                    pending: true,
                    owner: Some(owner),
                    ..Default::default()
                })
                .id();
            let server = app
                .world_mut()
                .spawn(McpCatalogServer {
                    id: "linear".into(),
                    url: "https://mcp.linear.app/mcp".into(),
                    scopes: Vec::new(),
                })
                .id();
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle(if disconnect {
                        "mcp.disconnect"
                    } else {
                        "mcp.connect"
                    })
                    .unwrap()
                    .running(),
                    OperationRetained,
                    OperationTarget::new(page),
                    EntityTarget::<McpCatalogServer>::new(server),
                    super::super::McpConnectionOperation,
                    PendingMcpOperation {
                        generation: 1,
                        owner,
                        id: "linear".into(),
                        disconnect,
                    },
                ))
                .id();
            app.world_mut().entity_mut(page).insert(EntityTarget::<
                super::super::McpConnectionOperation,
            >::new(operation));
            app.world_mut()
                .entity_mut(server)
                .insert(super::super::McpServerClaim {
                    operation: Some(operation),
                });
            app.world_mut().despawn(owner);
            app.update();
            assert_eq!(
                app.world().get::<OperationState>(operation),
                Some(&OperationState::Failed)
            );
            assert!(app.world().get::<OperationRetained>(operation).is_none());
            assert!(app.world().get::<PendingMcpOperation>(operation).is_none());
            assert_eq!(
                app.world()
                    .get::<super::super::McpServerClaim>(server)
                    .unwrap()
                    .operation,
                None
            );
            let state = app.world().get::<super::super::McpPageState>(page).unwrap();
            assert!(!state.pending && !state.loading);
            let result = app.world().get::<McpCompletion>(page).unwrap();
            assert!(!result.success);
            assert_eq!(result.disconnect, disconnect);
        }
    }

    #[test]
    fn retained_credentials_do_not_replace_the_profile_during_commit() {
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let credentials = McpCredentials {
            access: McpCredentialAccess::default(),
            storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                vmux_profile::Profile::named("native-commit-contract"),
                vmux_profile::ProfilePaths::current(),
            ),
        };
        let owner = app.world_mut().spawn(credentials.clone()).id();
        let server = McpCatalogServer {
            id: "linear".into(),
            url: "https://mcp.linear.app/mcp".into(),
            scopes: Vec::new(),
        };
        let catalog = app.world_mut().spawn(server.clone()).id();
        let directory = tempfile::tempdir().unwrap();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                EntityTarget::<McpCatalogServer>::new(catalog),
                super::super::McpConnectionOperation,
                Active,
                PhaseState(Phase::Prepare),
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: false,
                },
                Credentials(credentials),
                server,
                ToolStore::new(
                    directory.path().join("config"),
                    directory.path().join("home"),
                ),
                OriginalManifest(Some(ToolsManifest::default())),
                AuthorizationServer {
                    authorization_endpoint: "https://auth.example/authorize".into(),
                    token_endpoint: "https://auth.example/token".into(),
                    registration_endpoint: None,
                },
                ClientRegistration {
                    client_id: "fixture-client".into(),
                    client_secret: None,
                },
                TokenResponse {
                    access_token: "fixture-token".into(),
                    refresh_token: None,
                    expires_in: None,
                    scope: None,
                },
                (CredentialWritten, WriteResult(Ok(()))),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Succeeded)
        );
        assert!(app.world().get::<McpCredentials>(owner).is_some());
        assert!(app.world().get::<Credentials>(operation).is_none());
        assert!(!directory.path().join("config").exists());
    }

    #[test]
    fn callback_denial_after_an_empty_poll_retires_authentication_without_exchange() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let listener = CallbackListener::bind().unwrap();
        let address = listener
            .redirect
            .strip_prefix("http://")
            .unwrap()
            .strip_suffix("/callback")
            .unwrap()
            .to_string();
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let owner = app.world_mut().spawn_empty().id();
        let server = app
            .world_mut()
            .spawn(McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                EntityTarget::<McpCatalogServer>::new(server),
                super::super::McpConnectionOperation,
                Active,
                PhaseState(Phase::Callback),
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: false,
                },
                listener,
                Pkce {
                    verifier: "unused".into(),
                    deadline: Instant::now() + Duration::from_secs(3),
                },
                CallbackState("expected".into()),
                Code(None),
            ))
            .id();
        app.update();
        let sender = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .write_all(b"GET /callback?state=expected&error=access_denied HTTP/1.1\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.world().get::<Active>(operation).is_some() {
            app.update();
            assert!(Instant::now() < deadline, "OAuth denial did not drain");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(sender.join().unwrap().starts_with("HTTP/1.1 400"));
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert!(
            app.world()
                .get::<IoTask<TokenResponse>>(operation)
                .is_none()
        );
        assert!(app.world().get::<CallbackListener>(operation).is_none());
    }

    #[test]
    fn accepted_io_retains_credential_lease_after_page_removal_until_completion() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let access = McpCredentialAccess::default();
        let before = access.stable_revision().unwrap();
        let transaction = future::block_on(access.transaction()).unwrap();
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let owner = app.world_mut().spawn_empty().id();
        let page = app
            .world_mut()
            .spawn(super::super::McpPageState::default())
            .id();
        let server = app
            .world_mut()
            .spawn(McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let (release, wait) = std::sync::mpsc::channel();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                OperationTarget::new(page),
                EntityTarget::<McpCatalogServer>::new(server),
                super::super::McpConnectionOperation,
                Active,
                PhaseState(Phase::Complete),
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: false,
                },
                CredentialLease {
                    _lease: transaction,
                },
                IoTask::spawn(async move {
                    wait.recv().unwrap();
                    Ok::<_, String>(CredentialWritten)
                }),
            ))
            .id();
        app.world_mut()
            .entity_mut(server)
            .insert(super::super::McpServerClaim {
                operation: Some(operation),
            });
        app.world_mut().despawn(page);
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().get::<CredentialLease>(operation).is_some());
        assert!(app.world().get::<OperationRetained>(operation).is_some());
        assert!(
            app.world()
                .get::<IoTask<CredentialWritten>>(operation)
                .is_some()
        );
        assert_eq!(access.with_revision(before, || "stale").unwrap(), None);
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.world().get::<Active>(operation).is_some() {
            app.update();
            assert!(
                Instant::now() < deadline,
                "credential lease or accepted IO did not retire"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.world().get::<CredentialLease>(operation).is_none());
        assert!(app.world().get::<OperationRetained>(operation).is_none());
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Succeeded)
        );
        let after = access.stable_revision().unwrap();
        assert!(after > before);
        assert_eq!(
            access.with_revision(after, || "current").unwrap(),
            Some("current")
        );
    }

    #[test]
    fn missing_native_issuer_refuses_authentication_before_http_registration() {
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let owner = app.world_mut().spawn_empty().id();
        let server = app
            .world_mut()
            .spawn(McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                EntityTarget::<McpCatalogServer>::new(server),
                super::super::McpConnectionOperation,
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: false,
                },
                Active,
                PhaseState(Phase::Authorization),
                Http::new().unwrap(),
                ProtectedResource {
                    authorization_servers: Vec::new(),
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert!(
            app.world()
                .get::<IoTask<AuthorizationServer>>(operation)
                .is_none()
        );
        assert!(
            app.world()
                .get::<IoTask<ClientRegistration>>(operation)
                .is_none()
        );
    }
    #[test]
    fn failed_manifest_write_restores_the_original_before_releasing_the_transaction() {
        IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(2).build());
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(
            directory.path().join("config"),
            directory.path().join("home"),
        );
        let mut original = ToolsManifest::default();
        original.mcp.servers.insert(
            "preserved".into(),
            super::super::McpServerManifest {
                transport: super::super::McpTransport::Http,
                command: None,
                args: Vec::new(),
                env: Default::default(),
                cwd: None,
                url: Some("https://preserved.example/mcp".into()),
                headers: Default::default(),
                header_env: Default::default(),
                bearer_token_env_var: None,
            },
        );
        original.write_to(&store.manifest_path()).unwrap();
        let access = McpCredentialAccess::default();
        let transaction = future::block_on(access.transaction()).unwrap();
        let store_lease = future::block_on(store.lease_async());
        let mut app = App::new();
        app.add_plugins(super::super::McpConnectionPlugin);
        let profile = vmux_profile::Profile::named("native-auth-contract");
        let credentials = McpCredentials {
            access: access.clone(),
            storage: vmux_profile::mcp_credentials::McpCredentialStorage::at(
                profile,
                vmux_profile::ProfilePaths::current(),
            ),
        };
        let owner = app.world_mut().spawn_empty().id();
        let server = app
            .world_mut()
            .spawn(McpCatalogServer {
                id: "linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
                scopes: Vec::new(),
            })
            .id();
        let operation = app
            .world_mut()
            .spawn((
                Operation::bundle("mcp.connect").unwrap().running(),
                OperationRetained,
                EntityTarget::<McpCatalogServer>::new(server),
                super::super::McpConnectionOperation,
                Active,
                Source {
                    generation: 1,
                    id: "linear".into(),
                    owner,
                    disconnect: false,
                },
                PhaseState(Phase::ManifestRollback),
                PrimaryFailure("mock partial config write".into()),
                store.clone(),
                Credentials(credentials),
                OriginalManifest(Some(original.clone())),
                OriginalCredential(None),
                StoreLease {
                    _lease: store_lease,
                },
                CredentialLease {
                    _lease: transaction,
                },
                CredentialRestored,
            ))
            .id();
        let mut corrupted = original.clone();
        corrupted.mcp.servers.clear();
        corrupted.write_to(&store.manifest_path()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.world().get::<Active>(operation).is_some() {
            app.update();
            assert!(Instant::now() < deadline, "manifest rollback did not drain");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .mcp
                .servers,
            original.mcp.servers
        );
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert!(app.world().get::<CredentialLease>(operation).is_none());
        assert!(app.world().get::<StoreLease>(operation).is_none());
        assert!(access.stable_revision().unwrap().is_multiple_of(2));
    }
}
