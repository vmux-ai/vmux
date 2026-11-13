use std::path::Path;
use std::sync::{Arc, Mutex};

use bevy_app::App;
use bevy_ecs::prelude::*;
use serde_json::Value;
use vmux_app::VmuxPlugin;
use vmux_app::lsp::{
    ChangeDocument, CloseDocument, Diagnostics, DiagnosticsSender, DocumentBinding, LaunchServer,
    Lookup, OpenDocument, ReceivedDiagnostics, Response, RestartServer, Root, Server,
    ServerCompleted, ServerFailure, ServerKey, StartServer, StopServer, Stopped, Transport,
    TransportIo, WireReply,
};
use vmux_ecs::{Cwd, OperationState};

struct MemoryTransport {
    replies: Arc<Mutex<Option<crossbeam_channel::Sender<Value>>>>,
    canceled: Arc<Mutex<Vec<i64>>>,
    capabilities: lsp_types::ServerCapabilities,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
}

impl TransportIo for MemoryTransport {
    fn request(&self, method: &str, params: Value) -> WireReply {
        assert_eq!(method, "textDocument/definition");
        assert_eq!(params["textDocument"]["uri"], "file:///shared.rs");
        let (sender, receiver) = crossbeam_channel::bounded(1);
        *self.replies.lock().unwrap() = Some(sender);
        WireReply::new(1, receiver)
    }

    fn notify(&self, method: &str, params: Value) {
        self.calls.lock().unwrap().push((method.into(), params));
    }

    fn cancel(&self, id: i64) {
        self.canceled.lock().unwrap().push(id);
    }

    fn capabilities(&self) -> &lsp_types::ServerCapabilities {
        &self.capabilities
    }

    fn exited(&self) -> bool {
        false
    }
}

struct LanguageWorkspace {
    root: Entity,
    server: Entity,
    owner: Entity,
    replies: Arc<Mutex<Option<crossbeam_channel::Sender<Value>>>>,
    canceled: Arc<Mutex<Vec<i64>>>,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    diagnostics: DiagnosticsSender,
}

impl LanguageWorkspace {
    fn attach(app: &mut App, directory: &str) -> Self {
        let replies = Arc::new(Mutex::new(None));
        let canceled = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let root = app
            .world_mut()
            .spawn((
                Root,
                Cwd(directory.into()),
                Name::new(directory.to_string()),
            ))
            .id();
        let transport = MemoryTransport {
            replies: replies.clone(),
            canceled: canceled.clone(),
            calls: calls.clone(),
            capabilities: lsp_types::ServerCapabilities {
                definition_provider: Some(lsp_types::OneOf::Left(true)),
                ..Default::default()
            },
        };
        let server = app
            .world_mut()
            .spawn((
                Server,
                ServerKey::new(directory, "language-server").with_arguments(vec!["--stdio".into()]),
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
        let launch = launches.into_iter().next().unwrap();
        assert_eq!(launch.server(), server);
        app.world_mut()
            .entity_mut(launch.operation)
            .insert(ServerCompleted::new(launch, Ok(Transport::new(transport))));
        app.update();
        let (diagnostics, inbox) = DiagnosticsSender::channel();
        app.world_mut()
            .spawn((Name::new("Language diagnostics"), inbox));
        let diagnostics = diagnostics
            .scoped(server)
            .with_transport(app.world().get::<Transport>(server).unwrap().identity());
        let owner = app
            .world_mut()
            .spawn(OpenDocument {
                server,
                path: "/shared.rs".into(),
                language: "rust".into(),
                text: "fn shared() {}".into(),
                revision: None,
            })
            .id();
        app.update();
        Self {
            root,
            server,
            owner,
            replies,
            canceled,
            calls,
            diagnostics,
        }
    }

    fn publish(&self, version: Option<i32>, message: &str) {
        self.diagnostics.send(ReceivedDiagnostics {
            path: "/shared.rs".into(),
            diagnostics: vec![lsp_types::Diagnostic {
                message: message.into(),
                ..Default::default()
            }],
            version,
            server: None,
            transport: None,
        });
    }

    fn definition(&self, app: &mut App) -> Entity {
        let mut state = bevy_ecs::system::SystemState::<Lookup>::new(app.world_mut());
        let request = state
            .get_mut(app.world_mut())
            .unwrap()
            .request(
                self.owner,
                Path::new("/shared.rs"),
                "textDocument/definition",
                serde_json::json!({ "textDocument": { "uri": "file:///shared.rs" } }),
            )
            .unwrap();
        app.world_mut().spawn(request.bundle()).id()
    }

    fn complete(&self, response: Value) {
        self.replies
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(response)
            .unwrap();
    }
}

#[test]
fn headless_language_workspaces_use_one_public_request_lifecycle() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().lsp(true).build());
    let first = LanguageWorkspace::attach(&mut app, "/one");
    let second = LanguageWorkspace::attach(&mut app, "/two");
    let discarded = first.definition(&mut app);
    let kept = second.definition(&mut app);
    app.world_mut().entity_mut(first.root).despawn();
    second.complete(serde_json::json!({ "result": { "uri": "file:///target.rs" } }));
    app.update();
    assert_eq!(
        app.world().get::<OperationState>(discarded),
        Some(&OperationState::Canceled)
    );
    assert_eq!(
        app.world().get::<OperationState>(kept),
        Some(&OperationState::Succeeded)
    );
    assert_eq!(*first.canceled.lock().unwrap(), vec![1]);
    assert!(second.canceled.lock().unwrap().is_empty());
    assert_eq!(
        app.world().get::<Response>(kept).unwrap().0["result"]["uri"],
        "file:///target.rs"
    );
    assert!(app.world().get::<Response>(discarded).is_none());
}

#[test]
fn stopped_and_failed_servers_restart_without_disrupting_another_workspace() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().lsp(true).build());
    let first = LanguageWorkspace::attach(&mut app, "/one");
    let second = LanguageWorkspace::attach(&mut app, "/two");
    let discarded = first.definition(&mut app);
    let kept = second.definition(&mut app);
    app.world_mut().write_message(StopServer {
        server: first.server,
    });
    app.update();
    assert!(app.world().get::<Stopped>(first.server).is_some());
    assert!(app.world().get::<DocumentBinding>(first.owner).is_none());
    assert_eq!(
        app.world().get::<OperationState>(discarded),
        Some(&OperationState::Canceled)
    );
    assert_eq!(*first.canceled.lock().unwrap(), vec![1]);
    app.world_mut().write_message(RestartServer {
        server: first.server,
    });
    app.world_mut().write_message(RestartServer {
        server: first.server,
    });
    app.update();
    let launches = app
        .world_mut()
        .resource_mut::<Messages<LaunchServer>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(launches.len(), 1);
    let launch = launches.into_iter().next().unwrap();
    assert_eq!(launch.key().root(), Path::new("/one"));
    assert_eq!(launch.key().command(), "language-server");
    assert_eq!(launch.key().arguments(), &["--stdio"]);
    let replacement = launch.server();
    let operation = launch.operation;
    app.world_mut()
        .entity_mut(operation)
        .insert(ServerCompleted::new(
            launch,
            Err("transport refused".into()),
        ));
    second.complete(serde_json::json!({ "result": "other workspace" }));
    app.update();
    assert_eq!(
        app.world().get::<ServerFailure>(replacement).unwrap().0,
        "transport refused"
    );
    assert_eq!(
        app.world().get::<OperationState>(operation),
        Some(&OperationState::Failed)
    );
    assert_eq!(
        app.world().get::<Response>(kept).unwrap().0["result"],
        "other workspace"
    );
    assert!(second.canceled.lock().unwrap().is_empty());
    app.world_mut().write_message(RestartServer {
        server: replacement,
    });
    app.update();
    assert!(app.world().get_entity(replacement).is_err());
    assert_eq!(
        app.world_mut()
            .resource_mut::<Messages<LaunchServer>>()
            .drain()
            .count(),
        1
    );
    assert!(app.world().get::<Transport>(second.server).is_some());
}

#[test]
fn public_document_and_diagnostic_lifetimes_survive_shared_close_and_refuse_retired_work() {
    let mut app = App::new();
    app.add_plugins(VmuxPlugin::builder().lsp(true).build());
    let first = LanguageWorkspace::attach(&mut app, "/one");
    let second = LanguageWorkspace::attach(&mut app, "/two");
    first.publish(Some(1), "first");
    second.publish(Some(1), "other root");
    app.update();
    let document = app
        .world()
        .get::<DocumentBinding>(first.owner)
        .unwrap()
        .document();
    let other = app
        .world()
        .get::<DocumentBinding>(second.owner)
        .unwrap()
        .document();
    let shared = app
        .world_mut()
        .spawn(OpenDocument {
            server: first.server,
            path: "/shared.rs".into(),
            language: "rust".into(),
            text: "fn shared() {}".into(),
            revision: None,
        })
        .id();
    app.update();
    let stale = first.definition(&mut app);
    first.complete(serde_json::json!({"result": "old owner"}));
    app.update();
    app.world_mut().write_message(CloseDocument {
        owner: first.owner,
        path: "/shared.rs".into(),
    });
    app.world_mut()
        .entity_mut(first.owner)
        .insert(OpenDocument {
            server: first.server,
            path: "/shared.rs".into(),
            language: "rust".into(),
            text: "fn shared() {}".into(),
            revision: None,
        });
    app.update();
    assert!(app.world().get::<Response>(stale).is_none());
    app.world_mut().write_message(CloseDocument {
        owner: first.owner,
        path: "/shared.rs".into(),
    });
    app.world_mut().write_message(ChangeDocument {
        owner: shared,
        path: "/shared.rs".into(),
        text: "fn changed() {}".into(),
        revision: None,
    });
    app.update();
    assert!(app.world().get::<Diagnostics>(document).is_none());
    first.publish(Some(1), "old version");
    app.update();
    assert!(app.world().get::<Diagnostics>(document).is_none());
    first.publish(Some(2), "current");
    app.update();
    assert_eq!(
        app.world().get::<Diagnostics>(document).unwrap().items()[0].message,
        "current"
    );
    app.world_mut().write_message(CloseDocument {
        owner: shared,
        path: "/shared.rs".into(),
    });
    app.update();
    let calls = first.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "textDocument/didOpen",
            "textDocument/didChange",
            "textDocument/didClose"
        ]
    );
    assert_eq!(calls[1].1["contentChanges"][0]["text"], "fn changed() {}");
    assert_eq!(
        app.world().get::<Diagnostics>(other).unwrap().items()[0].message,
        "other root"
    );
}
