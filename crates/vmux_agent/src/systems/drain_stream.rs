use bevy::prelude::*;

use crate::components::{AgentApprovalPolicy, AgentMessages, AgentSession};
use crate::events::{AgentApprovalRequest, AgentDelta, AgentToolStatus, ToolStatus};
use crate::message::{AssistantBlock, Message};
use crate::run_state::AgentRunState;
use crate::stream::{PartialToolUse, StopReason, StreamEvent};

pub fn drain_stream(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut AgentRunState,
        &mut AgentMessages,
        &AgentApprovalPolicy,
        &AgentSession,
    )>,
) {
    for (entity, mut state, mut messages, policy, _session) in &mut q {
        let drained: Vec<StreamEvent> = match &*state {
            AgentRunState::Streaming { rx, .. } => rx.try_iter().collect(),
            _ => continue,
        };
        if drained.is_empty() {
            continue;
        }

        ensure_assistant_tail(&mut messages);

        let mut next_state: Option<AgentRunState> = None;
        for event in drained {
            match event {
                StreamEvent::TextDelta(text) => {
                    append_text_delta(&mut messages, &text);
                    commands.trigger(AgentDelta {
                        session: entity,
                        text,
                    });
                }
                StreamEvent::ToolUseStart { call_id, name } => {
                    if let AgentRunState::Streaming { partial, .. } = &mut *state {
                        *partial = Some(PartialToolUse {
                            call_id,
                            name,
                            args_buf: String::new(),
                        });
                    }
                }
                StreamEvent::ToolUseArgsDelta {
                    call_id,
                    json_chunk,
                } => {
                    if let AgentRunState::Streaming { partial, .. } = &mut *state
                        && let Some(p) = partial
                    {
                        if !call_id.is_empty() && p.call_id.is_empty() {
                            p.call_id = call_id;
                        }
                        p.args_buf.push_str(&json_chunk);
                    }
                }
                StreamEvent::ToolUseEnd {
                    call_id: streamed_id,
                } => {
                    let p = match &mut *state {
                        AgentRunState::Streaming { partial, .. } => partial.take(),
                        _ => None,
                    };
                    if let Some(mut p) = p {
                        if p.call_id.is_empty() && !streamed_id.is_empty() {
                            p.call_id = streamed_id;
                        }
                        push_tool_use_block(&mut messages, &p);
                        let args_value: serde_json::Value = serde_json::from_str(&p.args_buf)
                            .unwrap_or_else(|_| serde_json::Value::String(p.args_buf.clone()));
                        commands.trigger(AgentToolStatus {
                            session: entity,
                            call_id: p.call_id.clone(),
                            status: ToolStatus::Pending,
                        });
                        if policy.auto.contains(&p.name) {
                            next_state = Some(spawn_running_tool(&p, args_value));
                        } else {
                            commands.trigger(AgentApprovalRequest {
                                session: entity,
                                call_id: p.call_id.clone(),
                                name: p.name.clone(),
                                args: args_value.clone(),
                            });
                            next_state = Some(AgentRunState::AwaitingApproval {
                                call_id: p.call_id,
                                name: p.name,
                                args: args_value,
                            });
                        }
                    }
                }
                StreamEvent::StopTurn {
                    reason: StopReason::EndTurn,
                } => {
                    if next_state.is_none() {
                        next_state = Some(AgentRunState::Idle);
                    }
                }
                StreamEvent::StopTurn {
                    reason: StopReason::ToolUse,
                } => {}
                StreamEvent::StopTurn {
                    reason: StopReason::MaxTokens | StopReason::Other,
                } => {
                    next_state = Some(AgentRunState::Idle);
                }
                StreamEvent::Error(msg) => {
                    next_state = Some(AgentRunState::Errored(msg));
                }
            }
        }

        if let Some(new_state) = next_state {
            *state = new_state;
        }
    }
}

fn ensure_assistant_tail(messages: &mut AgentMessages) {
    if !matches!(messages.0.last(), Some(Message::Assistant { .. })) {
        messages.0.push(Message::Assistant { blocks: Vec::new() });
    }
}

fn append_text_delta(messages: &mut AgentMessages, text: &str) {
    let Some(Message::Assistant { blocks }) = messages.0.last_mut() else {
        return;
    };
    if let Some(AssistantBlock::Text(buf)) = blocks.last_mut() {
        buf.push_str(text);
    } else {
        blocks.push(AssistantBlock::Text(text.to_string()));
    }
}

fn push_tool_use_block(messages: &mut AgentMessages, p: &PartialToolUse) {
    let Some(Message::Assistant { blocks }) = messages.0.last_mut() else {
        return;
    };
    blocks.push(AssistantBlock::ToolUse {
        call_id: p.call_id.clone(),
        name: p.name.clone(),
        args: p.args_buf.clone(),
    });
}

fn spawn_running_tool(p: &PartialToolUse, args: serde_json::Value) -> AgentRunState {
    let call_id = p.call_id.clone();
    let task = crate::tool_dispatch::spawn_tool_task(call_id.clone(), p.name.clone(), args);
    AgentRunState::RunningTool { call_id, task }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::PartialToolUse;
    use crate::{AgentKind, AgentVariant};
    use bevy::tasks::IoTaskPool;
    use crossbeam_channel::unbounded;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, drain_stream);
        app
    }

    fn make_session() -> AgentSession {
        AgentSession {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Page,
            sid: "t".into(),
            provider: "mock".into(),
            model: "m".into(),
        }
    }

    #[test]
    fn text_delta_then_end_turn_goes_idle() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::TextDelta("hi".into())).unwrap();
        tx.send(StreamEvent::StopTurn {
            reason: StopReason::EndTurn,
        })
        .unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: None,
                },
            ))
            .id();
        app.update();
        assert!(matches!(
            app.world().get::<AgentRunState>(entity),
            Some(AgentRunState::Idle)
        ));
    }

    #[test]
    fn tool_use_without_policy_transitions_to_awaiting_approval() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::ToolUseStart {
            call_id: "c1".into(),
            name: "list_spaces".into(),
        })
        .unwrap();
        tx.send(StreamEvent::ToolUseArgsDelta {
            call_id: String::new(),
            json_chunk: "{\"x\":1}".into(),
        })
        .unwrap();
        tx.send(StreamEvent::ToolUseEnd {
            call_id: String::new(),
        })
        .unwrap();
        tx.send(StreamEvent::StopTurn {
            reason: StopReason::ToolUse,
        })
        .unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: Some(PartialToolUse::default()),
                },
            ))
            .id();
        app.update();
        match app.world().get::<AgentRunState>(entity).unwrap() {
            AgentRunState::AwaitingApproval { call_id, name, .. } => {
                assert_eq!(call_id, "c1");
                assert_eq!(name, "list_spaces");
            }
            _ => panic!("expected AwaitingApproval"),
        }
    }

    #[test]
    fn error_event_transitions_to_errored() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        tx.send(StreamEvent::Error("HTTP 500: boom".into()))
            .unwrap();
        drop(tx);
        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                make_session(),
                AgentMessages::default(),
                AgentApprovalPolicy::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: None,
                },
            ))
            .id();
        app.update();
        match app.world().get::<AgentRunState>(entity).unwrap() {
            AgentRunState::Errored(msg) => assert!(msg.contains("HTTP 500")),
            _ => panic!("expected Errored"),
        }
    }
}
