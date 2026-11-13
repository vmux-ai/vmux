use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use lsp_types::ServerCapabilities;
use serde_json::Value;
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision,
};

pub use server::{
    LaunchServer, RestartServer, ServerCompleted, ServerFailure, StartServer, StopServer, Stopped,
};

mod server;

pub struct LspPlugin;

impl Plugin for LspPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_plugins(server::ServersPlugin)
            .init_resource::<RequestPolicy>()
            .configure_sets(
                Update,
                (
                    Set::Servers,
                    Set::Documents,
                    Set::Diagnostics,
                    Set::Requests,
                )
                    .chain(),
            )
            .add_systems(Update, (cancel, collect).chain().in_set(Set::Requests));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    Servers,
    Documents,
    Requests,
    Diagnostics,
}

#[derive(Component)]
pub struct Root;

#[derive(Component)]
pub struct Server;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct ServerKey {
    root: PathBuf,
    command: String,
    arguments: Vec<String>,
}

impl ServerKey {
    pub fn new(root: impl Into<PathBuf>, command: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            command: command.into(),
            arguments: Vec::new(),
        }
    }

    pub fn with_arguments(mut self, arguments: Vec<String>) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

#[derive(Component)]
pub struct Document {
    pub version: i32,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DocumentPath(pub PathBuf);

#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = DocumentViews)]
pub struct DocumentBinding(Entity);

impl DocumentBinding {
    pub const fn new(document: Entity) -> Self {
        Self(document)
    }

    pub const fn document(&self) -> Entity {
        self.0
    }
}

#[derive(Component)]
#[relationship_target(relationship = DocumentBinding)]
pub struct DocumentViews(Vec<Entity>);

#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = ServerUsers)]
pub struct ServerBinding(Entity);

impl ServerBinding {
    pub const fn new(server: Entity) -> Self {
        Self(server)
    }

    pub const fn server(&self) -> Entity {
        self.0
    }
}

#[derive(Component)]
#[relationship_target(relationship = ServerBinding)]
pub struct ServerUsers(Vec<Entity>);

pub struct WireReply {
    id: i64,
    receiver: crossbeam_channel::Receiver<Value>,
}

impl WireReply {
    pub fn new(id: i64, receiver: crossbeam_channel::Receiver<Value>) -> Self {
        Self { id, receiver }
    }
}

pub trait TransportIo: Send + Sync + 'static {
    fn request(&self, method: &str, params: Value) -> WireReply;
    fn notify(&self, method: &str, params: Value);
    fn cancel(&self, id: i64);
    fn capabilities(&self) -> &ServerCapabilities;
    fn exited(&self) -> bool;
}

#[derive(Component, Clone)]
pub struct Transport {
    identity: uuid::Uuid,
    io: Arc<dyn TransportIo>,
}

impl Transport {
    pub fn new(transport: impl TransportIo) -> Self {
        Self::with_identity(transport, uuid::Uuid::new_v4())
    }

    pub fn with_identity(transport: impl TransportIo, identity: uuid::Uuid) -> Self {
        Self {
            identity,
            io: Arc::new(transport),
        }
    }

    pub fn identity(&self) -> uuid::Uuid {
        self.identity
    }

    pub fn capabilities(&self) -> &ServerCapabilities {
        self.io.capabilities()
    }

    pub fn request(&self, method: &str, params: Value) -> WireReply {
        self.io.request(method, params)
    }

    pub fn provides(&self, method: &str) -> bool {
        Capabilities::new(self.capabilities()).provides(method)
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.io.notify(method, params);
    }

    pub fn exited(&self) -> bool {
        self.io.exited()
    }

    pub fn did_open(&self, uri: &str, language: &str, version: i32, text: &str) {
        self.notify("textDocument/didOpen", serde_json::json!({ "textDocument": { "uri": uri, "languageId": language, "version": version, "text": text } }));
    }

    pub fn did_change(&self, uri: &str, version: i32, text: &str) {
        self.notify("textDocument/didChange", serde_json::json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }));
    }

    pub fn did_close(&self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        );
    }
}

pub struct Capabilities<'a>(&'a ServerCapabilities);

impl<'a> Capabilities<'a> {
    pub fn new(capabilities: &'a ServerCapabilities) -> Self {
        Self(capabilities)
    }

    pub fn provides(&self, method: &str) -> bool {
        let caps = self.0;
        match method {
            "textDocument/hover" => match &caps.hover_provider {
                Some(lsp_types::HoverProviderCapability::Simple(yes)) => *yes,
                Some(_) => true,
                None => false,
            },
            "textDocument/foldingRange" => match &caps.folding_range_provider {
                Some(lsp_types::FoldingRangeProviderCapability::Simple(yes)) => *yes,
                Some(_) => true,
                None => false,
            },
            "textDocument/codeAction" => match &caps.code_action_provider {
                Some(lsp_types::CodeActionProviderCapability::Simple(yes)) => *yes,
                Some(_) => true,
                None => false,
            },
            "textDocument/semanticTokens/full" => {
                let full = match &caps.semantic_tokens_provider {
                    Some(lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options)) => &options.full,
                    Some(lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(options)) => &options.semantic_tokens_options.full,
                    None => return false,
                };
                matches!(
                    full,
                    Some(
                        lsp_types::SemanticTokensFullOptions::Bool(true)
                            | lsp_types::SemanticTokensFullOptions::Delta { .. }
                    )
                )
            }
            "textDocument/completion" => caps.completion_provider.is_some(),
            "textDocument/definition" => Self::offered(&caps.definition_provider),
            "textDocument/declaration" => match &caps.declaration_provider {
                Some(lsp_types::DeclarationCapability::Simple(enabled)) => *enabled,
                Some(_) => true,
                None => false,
            },
            "textDocument/typeDefinition" => match &caps.type_definition_provider {
                Some(lsp_types::TypeDefinitionProviderCapability::Simple(enabled)) => *enabled,
                Some(_) => true,
                None => false,
            },
            "textDocument/implementation" => match &caps.implementation_provider {
                Some(lsp_types::ImplementationProviderCapability::Simple(enabled)) => *enabled,
                Some(_) => true,
                None => false,
            },
            "textDocument/references" => Self::offered(&caps.references_provider),
            "textDocument/documentSymbol" => Self::offered(&caps.document_symbol_provider),
            "textDocument/rename" => Self::offered(&caps.rename_provider),
            "textDocument/formatting" => Self::offered(&caps.document_formatting_provider),
            "textDocument/rangeFormatting" => {
                Self::offered(&caps.document_range_formatting_provider)
            }
            _ => true,
        }
    }

    fn offered<T>(provider: &Option<lsp_types::OneOf<bool, T>>) -> bool {
        match provider {
            Some(lsp_types::OneOf::Left(yes)) => *yes,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub document: Entity,
    pub server: Entity,
    pub version: i32,
    pub transport: uuid::Uuid,
}

impl Stamp {
    pub fn current(&self, owner: Entity, lookup: &Lookup) -> bool {
        let Ok(binding) = lookup.bindings.get(owner) else {
            return false;
        };
        let Ok((_, _, document, parent)) = lookup.documents.get(self.document) else {
            return false;
        };
        binding.document() == self.document
            && document.version == self.version
            && parent.parent() == self.server
            && lookup
                .transports
                .get(self.server)
                .is_ok_and(|transport| transport.identity == self.transport)
            && lookup.servers.contains(self.server)
    }
}

#[derive(SystemParam)]
pub struct Lookup<'w, 's> {
    servers: Query<'w, 's, (Entity, &'static ServerKey), With<Server>>,
    transports: Query<'w, 's, &'static Transport>,
    documents: Query<
        'w,
        's,
        (
            Entity,
            &'static DocumentPath,
            &'static Document,
            &'static ChildOf,
        ),
    >,
    bindings: Query<'w, 's, &'static DocumentBinding>,
}

impl Lookup<'_, '_> {
    pub fn server_current(&self, server: Entity) -> bool {
        self.servers.contains(server) && self.transports.contains(server)
    }

    pub fn server(&self, key: &ServerKey) -> Option<Entity> {
        let mut found = None;
        for (entity, candidate) in &self.servers {
            if candidate != key {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(entity);
        }
        found
    }

    pub fn document(&self, owner: Entity, path: &Path) -> Option<Stamp> {
        let entity = self.bindings.get(owner).ok()?.document();
        let (_, candidate, document, parent) = self.documents.get(entity).ok()?;
        if candidate.0 != path || !self.server_current(parent.parent()) {
            return None;
        }
        Some(Stamp {
            document: entity,
            server: parent.parent(),
            version: document.version,
            transport: self.transport(parent.parent())?.identity,
        })
    }

    pub fn key(&self, server: Entity) -> Option<&ServerKey> {
        Some(self.servers.get(server).ok()?.1)
    }

    pub fn key_for(&self, owner: Entity, path: &Path) -> Option<&ServerKey> {
        self.key(self.document(owner, path)?.server)
    }

    pub fn transport(&self, server: Entity) -> Option<&Transport> {
        self.transports.get(server).ok()
    }

    pub fn transport_current(&self, server: Entity, identity: uuid::Uuid) -> bool {
        self.server_current(server)
            && self
                .transport(server)
                .is_some_and(|transport| transport.identity() == identity)
    }

    pub fn request(
        &self,
        owner: Entity,
        path: &Path,
        method: &str,
        params: Value,
    ) -> Option<Request> {
        let stamp = self.document(owner, path)?;
        let transport = self.transport(stamp.server)?.clone();
        if !transport.provides(method) {
            return None;
        }
        let wire = transport.io.request(method, params);
        Some(Request {
            owner,
            stamp,
            transport,
            wire,
            since: web_time::Instant::now(),
            finished: false,
        })
    }
}

#[derive(Component)]
pub struct Request {
    owner: Entity,
    stamp: Stamp,
    transport: Transport,
    wire: WireReply,
    since: web_time::Instant,
    finished: bool,
}

#[derive(Component)]
struct RequestOwner(Entity);

#[derive(Resource)]
pub struct RequestPolicy {
    pub timeout: Duration,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
        }
    }
}

impl Request {
    pub fn bundle(self) -> impl Bundle {
        (
            Operation::bundle("lsp.request").unwrap(),
            OperationTarget::new(self.stamp.document),
            OperationTargetRevision(self.stamp.version as u64),
            self.stamp,
            RequestOwner(self.owner),
            self,
        )
    }

    fn cancel(&mut self) {
        if !self.finished {
            self.transport.io.cancel(self.wire.id);
            self.finished = true;
        }
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Component)]
pub struct Response(pub Value);

fn cancel(
    lookup: Lookup,
    policy: Res<RequestPolicy>,
    mut requests: Query<(Entity, &mut Request, &OperationState)>,
    responses: Query<(Entity, &Stamp, &RequestOwner), With<Response>>,
    mut commands: Commands,
) {
    for (entity, mut request, state) in &mut requests {
        let current = request.stamp.current(request.owner, &lookup);
        let expired = request.since.elapsed() >= policy.timeout;
        let active = matches!(state, OperationState::Pending | OperationState::Running);
        if current && !expired && active {
            continue;
        }
        request.cancel();
        if active {
            if current && expired {
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err("language server request timed out".into()),
                });
            } else {
                commands.entity(entity).insert(OperationState::Canceled);
            }
        }
        commands.entity(entity).remove::<Request>();
    }
    for (entity, stamp, owner) in &responses {
        if !stamp.current(owner.0, &lookup) {
            commands.entity(entity).remove::<Response>();
        }
    }
}

fn collect(mut requests: Query<(Entity, &mut Request)>, mut commands: Commands) {
    for (entity, mut request) in &mut requests {
        let response = match request.wire.receiver.try_recv() {
            Ok(response) => response,
            Err(crossbeam_channel::TryRecvError::Empty) => continue,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                request.cancel();
                commands.trigger(OperationCompleted {
                    entity,
                    result: Err("language server response channel closed".into()),
                });
                commands.entity(entity).remove::<Request>();
                continue;
            }
        };
        request.finished = true;
        let result = match response.get("error") {
            Some(error) => Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("language server rejected the request")
                .to_string()),
            None => Ok(()),
        };
        if result.is_ok() {
            commands.entity(entity).insert(Response(response));
        }
        commands.trigger(OperationCompleted { entity, result });
        commands.entity(entity).remove::<Request>();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone, Debug, Default)]
    struct Probe {
        pending: Arc<Mutex<Vec<crossbeam_channel::Sender<Value>>>>,
        canceled: Arc<Mutex<Vec<i64>>>,
        capabilities: ServerCapabilities,
    }

    impl Probe {
        fn reply(&self, value: Value) {
            self.pending.lock().unwrap()[0].send(value).unwrap();
        }
    }

    impl TransportIo for Probe {
        fn request(&self, _method: &str, _params: Value) -> WireReply {
            let (sender, receiver) = crossbeam_channel::unbounded();
            let mut pending = self.pending.lock().unwrap();
            pending.push(sender);
            WireReply::new(pending.len() as i64, receiver)
        }

        fn notify(&self, _method: &str, _params: Value) {}

        fn cancel(&self, id: i64) {
            self.canceled.lock().unwrap().push(id);
        }

        fn capabilities(&self) -> &ServerCapabilities {
            &self.capabilities
        }

        fn exited(&self) -> bool {
            false
        }
    }

    struct Harness {
        app: App,
        roots: [Entity; 2],
        servers: [Entity; 2],
        owners: [Entity; 2],
        probes: [Probe; 2],
    }

    impl Harness {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(LspPlugin);
            let mut roots = Vec::new();
            let mut servers = Vec::new();
            let mut owners = Vec::new();
            let mut probes = Vec::new();
            for directory in ["/one", "/two"] {
                let root = app
                    .world_mut()
                    .spawn((Root, vmux_ecs::Cwd(directory.into())))
                    .id();
                let probe = Probe {
                    capabilities: ServerCapabilities {
                        definition_provider: Some(lsp_types::OneOf::Left(true)),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let server = app
                    .world_mut()
                    .spawn((
                        Server,
                        ServerKey::new(directory, "server"),
                        Transport::new(probe.clone()),
                        ChildOf(root),
                    ))
                    .id();
                let document = app
                    .world_mut()
                    .spawn((
                        Document { version: 1 },
                        DocumentPath("/shared.rs".into()),
                        ChildOf(server),
                    ))
                    .id();
                let owner = app.world_mut().spawn(DocumentBinding::new(document)).id();
                roots.push(root);
                servers.push(server);
                owners.push(owner);
                probes.push(probe);
            }
            Self {
                app,
                roots: roots.try_into().unwrap(),
                servers: servers.try_into().unwrap(),
                owners: owners.try_into().unwrap(),
                probes: probes.try_into().unwrap(),
            }
        }

        fn request(&mut self, root: usize) -> Entity {
            let mut state = bevy_ecs::system::SystemState::<Lookup>::new(self.app.world_mut());
            let request = state
                .get_mut(self.app.world_mut())
                .unwrap()
                .request(
                    self.owners[root],
                    Path::new("/shared.rs"),
                    "textDocument/definition",
                    serde_json::json!({}),
                )
                .unwrap();
            self.app.world_mut().spawn(request.bundle()).id()
        }
    }

    #[test]
    fn removing_one_root_cancels_its_wire_work_and_preserves_another_root() {
        let mut harness = Harness::new();
        let first = harness.request(0);
        let second = harness.request(1);
        harness
            .app
            .world_mut()
            .entity_mut(harness.roots[0])
            .despawn();
        harness.probes[1].reply(serde_json::json!({ "result": "other root" }));
        harness.app.update();
        assert_eq!(
            harness.app.world().get::<OperationState>(first),
            Some(&OperationState::Canceled)
        );
        assert_eq!(
            harness.app.world().get::<OperationState>(second),
            Some(&OperationState::Succeeded)
        );
        assert_eq!(*harness.probes[0].canceled.lock().unwrap(), vec![1]);
        assert!(harness.probes[1].canceled.lock().unwrap().is_empty());
        assert!(harness.app.world().get::<Response>(first).is_none());
        assert_eq!(
            harness.app.world().get::<Response>(second).unwrap().0["result"],
            "other root"
        );
    }

    #[test]
    fn server_start_completion_and_stop_use_current_launch_identity() {
        let mut app = App::new();
        app.add_plugins(LspPlugin);
        let root = app
            .world_mut()
            .spawn((Root, vmux_ecs::Cwd("/one".into())))
            .id();
        let server = app
            .world_mut()
            .spawn((
                Server,
                ServerKey::new("/one", "server"),
                StartServer,
                ChildOf(root),
            ))
            .id();
        app.update();
        let launches = app
            .world_mut()
            .resource_mut::<Messages<LaunchServer>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(launches.len(), 1);
        let launch = &launches[0];
        app.world_mut()
            .entity_mut(launch.operation)
            .insert(ServerCompleted::new(
                launch.clone(),
                Ok(Transport::new(Probe::default())),
            ));
        app.update();
        assert!(app.world().get::<Transport>(server).is_some());
        assert_eq!(
            app.world().get::<OperationState>(launch.operation),
            Some(&OperationState::Succeeded)
        );
        app.world_mut().entity_mut(server).insert(StartServer);
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<LaunchServer>>()
                .drain()
                .next()
                .is_none()
        );
        let document = app
            .world_mut()
            .spawn((
                Document { version: 1 },
                DocumentPath("/one/file.rs".into()),
                ChildOf(server),
            ))
            .id();
        app.world_mut().write_message(StopServer { server });
        app.update();
        assert!(app.world().get::<Transport>(server).is_none());
        assert!(app.world().get::<Stopped>(server).is_some());
        assert!(app.world().get_entity(document).is_err());
        assert_eq!(
            app.world().get::<OperationState>(launch.operation),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn root_change_discards_startup_and_keeps_another_root_running() {
        let mut harness = Harness::new();
        harness
            .app
            .world_mut()
            .entity_mut(harness.servers[0])
            .remove::<Transport>()
            .insert(StartServer);
        harness.app.update();
        let launches = harness
            .app
            .world_mut()
            .resource_mut::<Messages<LaunchServer>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(launches.len(), 1);
        harness
            .app
            .world_mut()
            .entity_mut(launches[0].operation)
            .insert(ServerCompleted::new(
                launches[0].clone(),
                Ok(Transport::new(Probe::default())),
            ));
        harness
            .app
            .world_mut()
            .get_mut::<vmux_ecs::Cwd>(harness.roots[0])
            .unwrap()
            .0 = "/changed".into();
        harness.app.update();
        assert!(harness.app.world().get_entity(harness.servers[0]).is_err());
        assert!(
            harness
                .app
                .world()
                .get::<Transport>(harness.servers[1])
                .is_some()
        );
    }

    #[test]
    fn canceled_startup_can_retry_without_completion_and_cannot_replace_the_new_start() {
        for stopped in [false, true] {
            let mut app = App::new();
            app.add_plugins(LspPlugin);
            let root = app
                .world_mut()
                .spawn((Root, vmux_ecs::Cwd("/one".into())))
                .id();
            let server = app
                .world_mut()
                .spawn((
                    Server,
                    ServerKey::new("/one", "server"),
                    StartServer,
                    ChildOf(root),
                ))
                .id();
            app.update();
            let old = app
                .world_mut()
                .resource_mut::<Messages<LaunchServer>>()
                .drain()
                .next()
                .unwrap();
            if stopped {
                app.world_mut().write_message(StopServer { server });
            } else {
                app.world_mut()
                    .entity_mut(old.operation)
                    .insert(OperationState::Canceled);
            }
            app.update();
            app.world_mut().entity_mut(server).insert(StartServer);
            app.update();
            let new = app
                .world_mut()
                .resource_mut::<Messages<LaunchServer>>()
                .drain()
                .next()
                .unwrap();
            let old_operation = old.operation;
            let new_operation = new.operation;
            app.world_mut()
                .entity_mut(new_operation)
                .insert(ServerCompleted::new(
                    old.clone(),
                    Ok(Transport::new(Probe::default())),
                ));
            app.update();
            assert!(app.world().get::<Transport>(server).is_none());
            assert_eq!(
                app.world().get::<OperationState>(new_operation),
                Some(&OperationState::Pending)
            );
            let transport = Transport::new(Probe::default());
            let identity = transport.identity();
            app.world_mut()
                .entity_mut(new_operation)
                .insert(ServerCompleted::new(new, Ok(transport)));
            app.world_mut()
                .entity_mut(old_operation)
                .insert(ServerCompleted::new(
                    old,
                    Ok(Transport::new(Probe::default())),
                ));
            app.update();
            assert_eq!(
                app.world().get::<Transport>(server).unwrap().identity(),
                identity
            );
            assert_eq!(
                app.world().get::<OperationState>(old_operation),
                Some(&OperationState::Canceled)
            );
            assert_eq!(
                app.world().get::<OperationState>(new_operation),
                Some(&OperationState::Succeeded)
            );
        }
    }

    #[test]
    fn full_semantic_requests_require_full_support_in_both_provider_variants() {
        for registration in [false, true] {
            for (full, enabled) in [
                (None, false),
                (
                    Some(lsp_types::SemanticTokensFullOptions::Bool(false)),
                    false,
                ),
                (Some(lsp_types::SemanticTokensFullOptions::Bool(true)), true),
                (
                    Some(lsp_types::SemanticTokensFullOptions::Delta { delta: Some(true) }),
                    true,
                ),
                (
                    Some(lsp_types::SemanticTokensFullOptions::Delta { delta: Some(false) }),
                    true,
                ),
                (
                    Some(lsp_types::SemanticTokensFullOptions::Delta { delta: None }),
                    true,
                ),
            ] {
                let options = lsp_types::SemanticTokensOptions {
                    range: Some(true),
                    full,
                    ..Default::default()
                };
                let provider = if registration {
                    lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        lsp_types::SemanticTokensRegistrationOptions {
                            semantic_tokens_options: options,
                            text_document_registration_options: Default::default(),
                            static_registration_options: Default::default(),
                        },
                    )
                } else {
                    lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options)
                };
                let mut harness = Harness::new();
                let probe = Probe {
                    capabilities: ServerCapabilities {
                        semantic_tokens_provider: Some(provider),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                harness
                    .app
                    .world_mut()
                    .entity_mut(harness.servers[0])
                    .insert(Transport::new(probe.clone()));
                let mut state =
                    bevy_ecs::system::SystemState::<Lookup>::new(harness.app.world_mut());
                let request = state.get_mut(harness.app.world_mut()).unwrap().request(
                    harness.owners[0],
                    Path::new("/shared.rs"),
                    "textDocument/semanticTokens/full",
                    serde_json::json!({}),
                );
                assert_eq!(request.is_some(), enabled);
                assert_eq!(probe.pending.lock().unwrap().len(), usize::from(enabled));
            }
        }
    }

    #[test]
    fn navigation_capabilities_honor_disabled_providers() {
        for (provider, method) in [
            ("declarationProvider", "textDocument/declaration"),
            ("typeDefinitionProvider", "textDocument/typeDefinition"),
            ("implementationProvider", "textDocument/implementation"),
        ] {
            for (value, enabled) in [
                (serde_json::json!(false), false),
                (serde_json::json!(true), true),
                (serde_json::json!({}), true),
            ] {
                let mut values = serde_json::Map::new();
                values.insert(provider.into(), value);
                let capabilities: ServerCapabilities =
                    serde_json::from_value(Value::Object(values)).unwrap();
                assert_eq!(Capabilities::new(&capabilities).provides(method), enabled);
            }
        }
    }

    #[test]
    fn replacing_a_transport_invalidates_pending_and_retained_responses() {
        for completed in [false, true] {
            let mut harness = Harness::new();
            let old = harness.request(0);
            if completed {
                harness.probes[0].reply(serde_json::json!({ "result": "old" }));
                harness.app.update();
                assert!(harness.app.world().get::<Response>(old).is_some());
            }
            let replacement = Probe {
                capabilities: harness.probes[0].capabilities.clone(),
                ..Default::default()
            };
            harness
                .app
                .world_mut()
                .entity_mut(harness.servers[0])
                .insert(Transport::new(replacement.clone()));
            if !completed {
                harness.probes[0].reply(serde_json::json!({ "result": "old" }));
            }
            harness.app.update();
            assert!(harness.app.world().get::<Response>(old).is_none());
            assert_eq!(
                harness.app.world().get::<OperationState>(old),
                Some(if completed {
                    &OperationState::Succeeded
                } else {
                    &OperationState::Canceled
                })
            );
            if !completed {
                assert_eq!(*harness.probes[0].canceled.lock().unwrap(), vec![1]);
            }
            let new = harness.request(0);
            replacement.reply(serde_json::json!({ "result": "replacement" }));
            harness.app.update();
            assert_eq!(
                harness.app.world().get::<OperationState>(new),
                Some(&OperationState::Succeeded)
            );
            assert_eq!(
                harness.app.world().get::<Response>(new).unwrap().0["result"],
                "replacement"
            );
        }
    }

    #[test]
    fn completed_responses_are_invalidated_when_the_requesting_owner_rebinds() {
        let mut harness = Harness::new();
        let request = harness.request(0);
        harness.probes[0].reply(serde_json::json!({ "result": "accepted" }));
        harness.app.update();
        assert!(harness.app.world().get::<Response>(request).is_some());
        let replacement = harness
            .app
            .world_mut()
            .spawn((
                Document { version: 1 },
                DocumentPath("/shared.rs".into()),
                ChildOf(harness.servers[0]),
            ))
            .id();
        harness
            .app
            .world_mut()
            .entity_mut(harness.owners[0])
            .insert(DocumentBinding::new(replacement));
        harness.app.update();
        assert!(harness.app.world().get::<Response>(request).is_none());
        assert_eq!(
            harness.app.world().get::<OperationState>(request),
            Some(&OperationState::Succeeded)
        );
    }
}
