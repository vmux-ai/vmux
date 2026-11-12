use std::path::Path;
use std::sync::{Arc, Mutex};

use bevy_app::App;
use bevy_ecs::prelude::*;
use serde_json::Value;
use vmux_app::VmuxPlugin;
use vmux_app::lsp::{
    Document, DocumentBinding, DocumentPath, LspLookup, Response, Root, Server, ServerKey,
    Transport, TransportIo, WireReply,
};
use vmux_ecs::{Cwd, OperationState};

struct MemoryTransport {
    replies: Arc<Mutex<Option<crossbeam_channel::Sender<Value>>>>,
    canceled: Arc<Mutex<Vec<i64>>>,
    capabilities: lsp_types::ServerCapabilities,
}

impl TransportIo for MemoryTransport {
    fn request(&self, method: &str, params: Value) -> WireReply {
        assert_eq!(method, "textDocument/definition");
        assert_eq!(params["textDocument"]["uri"], "file:///shared.rs");
        let (sender, receiver) = crossbeam_channel::bounded(1);
        *self.replies.lock().unwrap() = Some(sender);
        WireReply::new(1, receiver)
    }

    fn notify(&self, _method: &str, _params: Value) {}

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
    owner: Entity,
    replies: Arc<Mutex<Option<crossbeam_channel::Sender<Value>>>>,
    canceled: Arc<Mutex<Vec<i64>>>,
}

impl LanguageWorkspace {
    fn attach(app: &mut App, directory: &str) -> Self {
        let replies = Arc::new(Mutex::new(None));
        let canceled = Arc::new(Mutex::new(Vec::new()));
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
            capabilities: lsp_types::ServerCapabilities {
                definition_provider: Some(lsp_types::OneOf::Left(true)),
                ..Default::default()
            },
        };
        let server = app
            .world_mut()
            .spawn((
                Server,
                ServerKey::new(directory, "language-server"),
                Transport::new(transport),
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
        Self {
            root,
            owner,
            replies,
            canceled,
        }
    }

    fn definition(&self, app: &mut App) -> Entity {
        let mut state = bevy_ecs::system::SystemState::<LspLookup>::new(app.world_mut());
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
