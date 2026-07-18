use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast, mpsc};

use crate::agent_broker::AgentBroker;
use crate::message::{AssistantBlock, Message};
use crate::protocol::{
    AgentAttachment, AgentRequestId, AgentRunStatus, ApprovalDecision, ServiceMessage,
};
use crate::providers::{anthropic, mistral, openai};
use crate::stream::{BuildRequest, ParseSse, StreamEvent, ToolDef};

pub struct PageProvider {
    pub build_request: BuildRequest,
    pub parse_sse: ParseSse,
    pub env_var: &'static str,
}

pub fn resolve_provider(provider: &str) -> Option<PageProvider> {
    match provider {
        "anthropic" => Some(PageProvider {
            build_request: anthropic::build_request,
            parse_sse: anthropic::parse_sse,
            env_var: "ANTHROPIC_API_KEY",
        }),
        "openai" => Some(PageProvider {
            build_request: openai::build_request,
            parse_sse: openai::parse_sse,
            env_var: "OPENAI_API_KEY",
        }),
        "mistral" => Some(PageProvider {
            build_request: mistral::build_request,
            parse_sse: mistral::parse_sse,
            env_var: "MISTRAL_API_KEY",
        }),
        _ => None,
    }
}

pub enum SessionInput {
    User {
        text: String,
        attachments: Vec<AgentAttachment>,
    },
    Approve {
        call_id: String,
        decision: ApprovalDecision,
    },
    Cancel,
    Close,
}

pub struct SessionHandle {
    pub input_tx: mpsc::UnboundedSender<SessionInput>,
    pub stream_tx: broadcast::Sender<ServiceMessage>,
    pub messages: Arc<Mutex<Vec<Message>>>,
    pub task: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
pub struct AgentSessionManager {
    sessions: HashMap<String, SessionHandle>,
}

impl AgentSessionManager {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        &mut self,
        sid: String,
        provider_name: &str,
        model: String,
        tools: Vec<ToolDef>,
        auto_tools: HashSet<String>,
        broker: AgentBroker,
    ) -> Result<(), String> {
        if self.sessions.contains_key(&sid) {
            return Ok(());
        }
        let provider = resolve_provider(provider_name)
            .ok_or_else(|| format!("unknown page-agent provider: {provider_name}"))?;
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (stream_tx, _) = broadcast::channel(256);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let task = tokio::spawn(run_session(
            sid.clone(),
            provider,
            model,
            tools,
            auto_tools,
            input_rx,
            stream_tx.clone(),
            broker,
            messages.clone(),
        ));
        self.sessions.insert(
            sid,
            SessionHandle {
                input_tx,
                stream_tx,
                messages,
                task,
            },
        );
        Ok(())
    }

    pub fn input(&self, sid: &str, input: SessionInput) {
        if let Some(handle) = self.sessions.get(sid) {
            let _ = handle.input_tx.send(input);
        }
    }

    pub fn subscribe(&self, sid: &str) -> Option<broadcast::Receiver<ServiceMessage>> {
        self.sessions.get(sid).map(|h| h.stream_tx.subscribe())
    }

    pub async fn snapshot(&self, sid: &str) -> Option<ServiceMessage> {
        let handle = self.sessions.get(sid)?;
        Some(snapshot_message(sid, &handle.messages).await)
    }

    pub fn close(&mut self, sid: &str) {
        if let Some(handle) = self.sessions.remove(sid) {
            let _ = handle.input_tx.send(SessionInput::Close);
            handle.task.abort();
        }
    }
}

async fn snapshot_message(sid: &str, messages: &Arc<Mutex<Vec<Message>>>) -> ServiceMessage {
    let msgs = messages.lock().await;
    let messages_json = serde_json::to_string(&*msgs).unwrap_or_else(|_| "[]".to_string());
    ServiceMessage::AgentMessagesSnapshot {
        sid: sid.to_string(),
        messages_json,
    }
}

fn spawn_sse(
    request: reqwest::Request,
    parse: ParseSse,
) -> (
    mpsc::UnboundedReceiver<StreamEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (cb_tx, cb_rx) = crossbeam_channel::unbounded::<StreamEvent>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<StreamEvent>();
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = cb_rx.recv() {
            if ev_tx.send(event).is_err() {
                break;
            }
        }
    });
    let http = tokio::spawn(async move {
        crate::http::drive_sse(request, parse, cb_tx).await;
    });
    (ev_rx, http)
}

fn append_text(blocks: &mut Vec<AssistantBlock>, text: &str) {
    if let Some(AssistantBlock::Text(buf)) = blocks.last_mut() {
        buf.push_str(text);
    } else {
        blocks.push(AssistantBlock::Text(text.to_string()));
    }
}

enum Decision {
    Allow,
    Deny,
    Cancelled,
    Closed,
}

async fn recv_user(
    input_rx: &mut mpsc::UnboundedReceiver<SessionInput>,
) -> Option<(String, Vec<AgentAttachment>)> {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::User { text, attachments }) => return Some((text, attachments)),
            Some(SessionInput::Approve { .. }) | Some(SessionInput::Cancel) => continue,
            Some(SessionInput::Close) | None => return None,
        }
    }
}

async fn await_decision(
    input_rx: &mut mpsc::UnboundedReceiver<SessionInput>,
    call_id: &str,
) -> Decision {
    loop {
        match input_rx.recv().await {
            Some(SessionInput::Approve {
                call_id: cid,
                decision,
            }) if cid == call_id => {
                return match decision {
                    ApprovalDecision::Allow => Decision::Allow,
                    ApprovalDecision::Deny => Decision::Deny,
                };
            }
            Some(SessionInput::Cancel) => return Decision::Cancelled,
            Some(SessionInput::Approve { .. }) | Some(SessionInput::User { .. }) => continue,
            Some(SessionInput::Close) | None => return Decision::Closed,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    sid: String,
    provider: PageProvider,
    model: String,
    tools: Vec<ToolDef>,
    auto_tools: HashSet<String>,
    mut input_rx: mpsc::UnboundedReceiver<SessionInput>,
    stream_tx: broadcast::Sender<ServiceMessage>,
    broker: AgentBroker,
    messages: Arc<Mutex<Vec<Message>>>,
) {
    let api_key: Option<String> = if provider.env_var.is_empty() {
        Some(String::new())
    } else {
        std::env::var(provider.env_var).ok()
    };

    loop {
        let Some((text, attachments)) = recv_user(&mut input_rx).await else {
            return;
        };
        messages
            .lock()
            .await
            .push(Message::user_with_attachments(text, attachments));

        let Some(key) = api_key.as_deref() else {
            let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                sid: sid.clone(),
                status: AgentRunStatus::Errored(format!("Missing {}", provider.env_var)),
            });
            continue;
        };

        loop {
            let request = {
                let msgs = messages.lock().await;
                (provider.build_request)(&model, msgs.as_slice(), &tools, key)
            };
            let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                sid: sid.clone(),
                status: AgentRunStatus::Streaming,
            });

            let (mut ev_rx, http) = spawn_sse(request, provider.parse_sse);
            let mut blocks: Vec<AssistantBlock> = Vec::new();
            let mut partial: Option<(String, String, String)> = None;
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut errored: Option<String> = None;
            let mut cancelled = false;

            loop {
                tokio::select! {
                    biased;
                    signal = input_rx.recv() => {
                        match signal {
                            Some(SessionInput::Cancel) => {
                                cancelled = true;
                                break;
                            }
                            Some(SessionInput::Close) | None => {
                                http.abort();
                                return;
                            }
                            Some(SessionInput::User { .. }) | Some(SessionInput::Approve { .. }) => {}
                        }
                    }
                    event = ev_rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        match event {
                            StreamEvent::TextDelta(text) => {
                                append_text(&mut blocks, &text);
                                let _ = stream_tx.send(ServiceMessage::AgentDelta {
                                    sid: sid.clone(),
                                    text,
                                });
                            }
                            StreamEvent::ToolUseStart { call_id, name } => {
                                partial = Some((call_id, name, String::new()));
                            }
                            StreamEvent::ToolUseArgsDelta {
                                call_id,
                                json_chunk,
                            } => {
                                if let Some(p) = &mut partial {
                                    if p.0.is_empty() && !call_id.is_empty() {
                                        p.0 = call_id;
                                    }
                                    p.2.push_str(&json_chunk);
                                }
                            }
                            StreamEvent::ToolUseEnd { call_id } => {
                                if let Some((mut cid, name, args)) = partial.take() {
                                    if cid.is_empty() && !call_id.is_empty() {
                                        cid = call_id;
                                    }
                                    blocks.push(AssistantBlock::ToolUse {
                                        call_id: cid.clone(),
                                        name: name.clone(),
                                        args: args.clone(),
                                    });
                                    pending_tool = Some((cid, name, args));
                                }
                            }
                            StreamEvent::StopTurn { .. } => {}
                            StreamEvent::Error(msg) => errored = Some(msg),
                        }
                    }
                }
            }

            if cancelled {
                http.abort();
            }
            if !blocks.is_empty() {
                messages.lock().await.push(Message::Assistant { blocks });
            }
            if cancelled {
                let _ = stream_tx.send(snapshot_message(&sid, &messages).await);
                let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                    sid: sid.clone(),
                    status: AgentRunStatus::Interrupted,
                });
                break;
            }

            if let Some(msg) = errored {
                let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                    sid: sid.clone(),
                    status: AgentRunStatus::Errored(msg),
                });
                break;
            }

            let _ = stream_tx.send(snapshot_message(&sid, &messages).await);

            let Some((call_id, name, args)) = pending_tool else {
                let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                    sid: sid.clone(),
                    status: AgentRunStatus::Idle,
                });
                break;
            };

            let args_json = if args.trim().is_empty() {
                "{}".to_string()
            } else {
                args
            };

            if !auto_tools.contains(&name) {
                let _ = stream_tx.send(ServiceMessage::AgentAwaitingApproval {
                    sid: sid.clone(),
                    call_id: call_id.clone(),
                    name: name.clone(),
                    args_json: args_json.clone(),
                });
                match await_decision(&mut input_rx, &call_id).await {
                    Decision::Closed => return,
                    Decision::Cancelled => {
                        let _ = stream_tx.send(ServiceMessage::AgentRunStatusChanged {
                            sid: sid.clone(),
                            status: AgentRunStatus::Interrupted,
                        });
                        break;
                    }
                    Decision::Deny => {
                        messages.lock().await.push(Message::ToolResult {
                            call_id,
                            content: "Tool call denied by user.".to_string(),
                            is_error: true,
                        });
                        continue;
                    }
                    Decision::Allow => {}
                }
            }

            let (content, is_error) = match broker
                .tool_call(AgentRequestId::new(), sid.clone(), name, args_json)
                .await
            {
                Ok(result) => result,
                Err(e) => (e, true),
            };
            messages.lock().await.push(Message::ToolResult {
                call_id,
                content,
                is_error,
            });
            let _ = stream_tx.send(snapshot_message(&sid, &messages).await);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_broker() -> AgentBroker {
        let (agent_tx, _) = broadcast::channel::<ServiceMessage>(16);
        AgentBroker::new(
            agent_tx,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
        )
    }

    #[test]
    fn resolve_provider_known_and_unknown() {
        assert!(resolve_provider("anthropic").is_some());
        assert!(resolve_provider("openai").is_some());
        assert!(resolve_provider("mistral").is_some());
        assert!(resolve_provider("nope").is_none());
    }

    #[tokio::test]
    async fn spawn_then_snapshot_empty_then_close() {
        let mut mgr = AgentSessionManager::default();
        mgr.spawn(
            "s".to_string(),
            "anthropic",
            "m".to_string(),
            Vec::new(),
            HashSet::new(),
            test_broker(),
        )
        .unwrap();
        match mgr.snapshot("s").await {
            Some(ServiceMessage::AgentMessagesSnapshot { messages_json, .. }) => {
                assert_eq!(messages_json, "[]");
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        mgr.close("s");
        assert!(mgr.snapshot("s").await.is_none());
    }

    #[tokio::test]
    async fn spawn_is_idempotent_per_sid() {
        let mut mgr = AgentSessionManager::default();
        mgr.spawn(
            "s".into(),
            "openai",
            "m".into(),
            Vec::new(),
            HashSet::new(),
            test_broker(),
        )
        .unwrap();
        mgr.spawn(
            "s".into(),
            "openai",
            "m".into(),
            Vec::new(),
            HashSet::new(),
            test_broker(),
        )
        .unwrap();
        assert!(mgr.snapshot("s").await.is_some());
        mgr.close("s");
    }

    #[tokio::test]
    async fn unknown_provider_is_rejected() {
        let mut mgr = AgentSessionManager::default();
        let err = mgr
            .spawn(
                "s".into(),
                "bogus",
                "m".into(),
                Vec::new(),
                HashSet::new(),
                test_broker(),
            )
            .unwrap_err();
        assert!(err.contains("bogus"));
    }
}
