//! The world's half of server-to-client LSP requests.
//!
//! A server request that needs world state cannot be answered on the reader thread, but the
//! server is blocked until it is. Each one becomes an entity carrying the handle that answers
//! it, so a system can take as long as it needs and a stalled request expires on its own
//! rather than hanging the server forever.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use serde_json::Value;

use crate::lsp::wire::{ErrorCode, RequestId};

pub struct ServerRequestPlugin;

impl Plugin for ServerRequestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServerEvents>()
            .add_message::<ServerReply>()
            .configure_sets(
                Update,
                (
                    ServerRequestSet::Receive,
                    ServerRequestSet::Answer,
                    ServerRequestSet::Reply,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    spawn_server_requests.in_set(ServerRequestSet::Receive),
                    answer_server_requests.in_set(ServerRequestSet::Reply),
                ),
            );
    }
}

/// Ordering contract for answering server requests.
///
/// Exported so a module that answers a request can order itself against `Reply` without naming
/// a system that is private to this plugin.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerRequestSet {
    Receive,
    Answer,
    Reply,
}

/// Ingress for everything the reader threads hand to the world.
///
/// Holds the sending half as well so a test can stand in for a reader thread and push through
/// the same path production uses.
#[derive(Resource)]
pub struct ServerEvents {
    tx: crossbeam_channel::Sender<ServerEvent>,
    rx: crossbeam_channel::Receiver<ServerEvent>,
}

impl Default for ServerEvents {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

impl ServerEvents {
    pub fn sender(&self) -> crossbeam_channel::Sender<ServerEvent> {
        self.tx.clone()
    }

    /// Lets a test observe what a reader thread produced without a running `App`.
    ///
    /// The channel is multi-consumer, so a receiver taken while [`ServerRequestPlugin`] is
    /// scheduled competes with it for messages.
    pub fn receiver(&self) -> crossbeam_channel::Receiver<ServerEvent> {
        self.rx.clone()
    }
}

pub enum ServerEvent {
    ApplyEdit {
        reply: ReplyHandle,
        params: lsp_types::ApplyWorkspaceEditParams,
    },
    Log {
        level: lsp_types::MessageType,
        text: String,
    },
}

/// Everything needed to answer one server request, and nothing else.
///
/// Carrying the server's own write handle is what keeps the answering systems from having to
/// reach into [`crate::lsp::manager::LspManager`] to find the right server.
#[derive(Clone)]
pub struct ReplyHandle {
    id: RequestId,
    outgoing: mpsc::Sender<Value>,
}

impl ReplyHandle {
    pub fn new(id: RequestId, outgoing: mpsc::Sender<Value>) -> Self {
        Self { id, outgoing }
    }

    pub fn ok(&self, result: Value) {
        let _ = self.outgoing.send(self.id.ok(result));
    }

    pub fn err(&self, code: ErrorCode) {
        let _ = self.outgoing.send(self.id.err(code));
    }
}

/// A server request the world has accepted but not yet answered.
#[derive(Component)]
pub struct ServerRequestPending {
    reply: ReplyHandle,
    frames: u32,
    since: Instant,
}

impl ServerRequestPending {
    /// Frames alone cannot bound the wait: `UpdateMode::Reactive` stops producing them when the
    /// app is idle, so a request arriving into an idle app would never expire.
    const MAX_FRAMES: u32 = 180;
    const MAX_WAIT: Duration = Duration::from_secs(5);

    fn new(reply: ReplyHandle) -> Self {
        Self {
            reply,
            frames: 0,
            since: Instant::now(),
        }
    }

    fn expired(&mut self) -> bool {
        self.frames += 1;
        self.frames > Self::MAX_FRAMES || self.since.elapsed() > Self::MAX_WAIT
    }
}

/// The edit a server asked us to apply, waiting for a system that owns buffers to do it.
#[derive(Component)]
pub struct AwaitingApplyEdit(pub lsp_types::ApplyWorkspaceEditParams);

#[derive(Message)]
pub struct ServerReply {
    pub request: Entity,
    pub result: Value,
}

fn spawn_server_requests(events: Res<ServerEvents>, mut commands: Commands) {
    for event in events.rx.try_iter() {
        match event {
            ServerEvent::ApplyEdit { reply, params } => {
                commands.spawn((ServerRequestPending::new(reply), AwaitingApplyEdit(params)));
            }
            ServerEvent::Log { level, text } => match level {
                lsp_types::MessageType::ERROR => tracing::error!("lsp: {text}"),
                lsp_types::MessageType::WARNING => tracing::warn!("lsp: {text}"),
                _ => tracing::info!("lsp: {text}"),
            },
        }
    }
}

/// Answer every request that a system resolved this frame, then give up on the rest.
///
/// One system, not two, because a despawn is deferred: the request answered on the frame it
/// would also have expired is still visible to the expiry pass, and would be replied to twice
/// under the same id.
fn answer_server_requests(
    mut replies: MessageReader<ServerReply>,
    mut pending: Query<(Entity, &mut ServerRequestPending)>,
    mut commands: Commands,
) {
    let mut answered = std::collections::HashSet::new();
    for reply in replies.read() {
        let Ok((entity, request)) = pending.get(reply.request) else {
            continue;
        };
        request.reply.ok(reply.result.clone());
        commands.entity(entity).despawn();
        answered.insert(entity);
    }
    for (entity, mut request) in &mut pending {
        if answered.contains(&entity) || !request.expired() {
            continue;
        }
        request.reply.err(ErrorCode::RequestCancelled);
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        app: App,
        outgoing: mpsc::Receiver<Value>,
        events: crossbeam_channel::Sender<ServerEvent>,
    }

    impl Harness {
        fn start() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ServerRequestPlugin));
            let events = app.world().resource::<ServerEvents>().sender();
            let (tx, outgoing) = mpsc::channel();
            let reply = ReplyHandle::new(RequestId::Number(1000), tx);
            events
                .send(ServerEvent::ApplyEdit {
                    reply,
                    params: lsp_types::ApplyWorkspaceEditParams {
                        label: None,
                        edit: lsp_types::WorkspaceEdit::default(),
                    },
                })
                .unwrap();
            Self {
                app,
                outgoing,
                events,
            }
        }

        fn pending(&mut self) -> Option<Entity> {
            let mut q = self
                .app
                .world_mut()
                .query_filtered::<Entity, With<ServerRequestPending>>();
            q.iter(self.app.world()).next()
        }
    }

    #[test]
    fn a_server_request_becomes_a_pending_entity() {
        let mut h = Harness::start();
        h.app.update();
        assert!(
            h.pending().is_some(),
            "request should be awaiting an answer"
        );
        assert!(
            h.outgoing.try_recv().is_err(),
            "nothing may be sent before a system answers"
        );
    }

    #[test]
    fn answering_replies_with_the_servers_own_id_and_despawns() {
        let mut h = Harness::start();
        h.app.update();
        let request = h.pending().expect("pending request");
        h.app.world_mut().write_message(ServerReply {
            request,
            result: serde_json::json!({ "applied": true }),
        });
        h.app.update();

        let sent = h
            .outgoing
            .try_recv()
            .expect("a reply must reach the server");
        assert_eq!(sent["id"], 1000);
        assert_eq!(sent["result"]["applied"], true);
        assert!(h.pending().is_none(), "answered request should despawn");
    }

    #[test]
    fn an_unanswered_request_expires_rather_than_hanging_the_server() {
        let mut h = Harness::start();
        for _ in 0..ServerRequestPending::MAX_FRAMES + 2 {
            h.app.update();
        }
        let sent = h.outgoing.try_recv().expect("expiry must still answer");
        assert_eq!(sent["error"]["code"], -32800);
        assert!(h.pending().is_none(), "expired request should despawn");
    }

    /// Answering on the very frame the request would expire must not send both replies: two
    /// results under one JSON-RPC id is a protocol violation.
    #[test]
    fn a_request_answered_as_it_expires_is_replied_to_once() {
        let mut h = Harness::start();
        for _ in 0..ServerRequestPending::MAX_FRAMES {
            h.app.update();
        }
        let request = h.pending().expect("still pending on the last good frame");
        h.app.world_mut().write_message(ServerReply {
            request,
            result: serde_json::json!({ "applied": true }),
        });
        h.app.update();

        assert_eq!(h.outgoing.try_recv().unwrap()["result"]["applied"], true);
        assert!(
            h.outgoing.try_recv().is_err(),
            "expiry must not answer a request that was already answered"
        );
    }

    #[test]
    fn a_log_event_spawns_no_request() {
        let mut h = Harness::start();
        h.events
            .send(ServerEvent::Log {
                level: lsp_types::MessageType::ERROR,
                text: "boom".to_string(),
            })
            .unwrap();
        h.app.update();
        let mut q = h
            .app
            .world_mut()
            .query_filtered::<Entity, With<ServerRequestPending>>();
        assert_eq!(q.iter(h.app.world()).count(), 1, "only the ApplyEdit");
    }
}
