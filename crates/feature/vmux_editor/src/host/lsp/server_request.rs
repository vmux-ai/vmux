use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use serde_json::Value;

use crate::lsp::wire::{ErrorCode, RequestId};

pub struct ServerRequestPlugin;

impl Plugin for ServerRequestPlugin {
    fn build(&self, app: &mut App) {
        let (apply_edit_sender, apply_edit_receiver) = crossbeam_channel::unbounded();
        let (log_sender, log_receiver) = crossbeam_channel::unbounded();
        app.add_systems(PreStartup, move |mut commands: Commands| {
            commands.spawn((
                Name::new("LSP server input"),
                ServerInputSender {
                    apply_edits: apply_edit_sender.clone(),
                    logs: log_sender.clone(),
                },
                ApplyEditInbox(apply_edit_receiver.clone()),
                ServerLogInbox(log_receiver.clone()),
            ));
        })
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
                spawn_apply_edit_requests.in_set(ServerRequestSet::Receive),
                write_server_logs.in_set(ServerRequestSet::Receive),
                answer_server_requests.in_set(ServerRequestSet::Reply),
            ),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerRequestSet {
    Receive,
    Answer,
    Reply,
}

#[derive(Component, Clone)]
pub struct ServerInputSender {
    pub(crate) apply_edits: crossbeam_channel::Sender<ApplyEditInput>,
    pub(crate) logs: crossbeam_channel::Sender<ServerLogInput>,
}

impl Default for ServerInputSender {
    fn default() -> Self {
        let (apply_edits, _) = crossbeam_channel::unbounded();
        let (logs, _) = crossbeam_channel::unbounded();
        Self { apply_edits, logs }
    }
}

#[derive(Component)]
struct ApplyEditInbox(crossbeam_channel::Receiver<ApplyEditInput>);

#[derive(Component)]
struct ServerLogInbox(crossbeam_channel::Receiver<ServerLogInput>);

pub struct ApplyEditInput {
    pub reply: ReplyHandle,
    pub root: PathBuf,
    pub params: lsp_types::ApplyWorkspaceEditParams,
}

pub struct ServerLogInput {
    pub level: lsp_types::MessageType,
    pub text: String,
}

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

#[derive(Component)]
pub struct ServerRequestPending {
    reply: ReplyHandle,
    frames: u32,
    since: Instant,
}

impl ServerRequestPending {
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

#[derive(Component)]
pub struct AwaitingApplyEdit {
    pub root: PathBuf,
    pub params: lsp_types::ApplyWorkspaceEditParams,
}

#[derive(Message)]
pub struct ServerReply {
    pub request: Entity,
    pub result: Value,
}

fn spawn_apply_edit_requests(inputs: Single<&ApplyEditInbox>, mut commands: Commands) {
    for input in inputs.0.try_iter() {
        commands.spawn((
            ServerRequestPending::new(input.reply),
            AwaitingApplyEdit {
                root: input.root,
                params: input.params,
            },
        ));
    }
}

fn write_server_logs(inputs: Single<&ServerLogInbox>) {
    for input in inputs.0.try_iter() {
        match input.level {
            lsp_types::MessageType::ERROR => tracing::error!("lsp: {}", input.text),
            lsp_types::MessageType::WARNING => tracing::warn!("lsp: {}", input.text),
            _ => tracing::info!("lsp: {}", input.text),
        }
    }
}

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
        inputs: ServerInputSender,
    }

    impl Harness {
        fn start() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ServerRequestPlugin));
            app.update();
            let inputs = {
                let world = app.world_mut();
                let mut senders = world.query::<&ServerInputSender>();
                senders.single(world).unwrap().clone()
            };
            let (tx, outgoing) = mpsc::channel();
            let reply = ReplyHandle::new(RequestId::Number(1000), tx);
            inputs
                .apply_edits
                .send(ApplyEditInput {
                    reply,
                    root: std::path::PathBuf::from("/tmp/project"),
                    params: lsp_types::ApplyWorkspaceEditParams {
                        label: None,
                        edit: lsp_types::WorkspaceEdit::default(),
                    },
                })
                .unwrap();
            Self {
                app,
                outgoing,
                inputs,
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
        h.inputs
            .logs
            .send(ServerLogInput {
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
