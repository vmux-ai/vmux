use bevy::prelude::*;

use crate::components::AgentMessages;
use crate::events::{AgentDelta, AgentToolStatus, ToolStatus};
use crate::message::{AssistantBlock, Message};
use crate::run_state::AgentRunState;
use crate::stream::{StopReason, StreamEvent};

pub fn drain_stream(
    mut commands: Commands,
    mut q: Query<(Entity, &mut AgentRunState, &mut AgentMessages)>,
) {
    for (entity, mut state, mut messages) in &mut q {
        let drained: Vec<StreamEvent> = match &*state {
            AgentRunState::Streaming { rx, .. } => rx.try_iter().collect(),
            _ => continue,
        };
        if drained.is_empty() {
            continue;
        }

        ensure_assistant_tail(&mut messages);

        let mut should_idle = false;
        for event in drained {
            match event {
                StreamEvent::TextDelta(text) => {
                    append_text_delta(&mut messages, &text);
                    commands.trigger(AgentDelta {
                        session: entity,
                        text,
                    });
                }
                StreamEvent::ToolUseEnd { call_id } => {
                    commands.trigger(AgentToolStatus {
                        session: entity,
                        call_id,
                        status: ToolStatus::Pending,
                    });
                }
                StreamEvent::StopTurn {
                    reason: StopReason::EndTurn,
                } => {
                    should_idle = true;
                }
                _ => {}
            }
        }

        if should_idle {
            *state = AgentRunState::Idle;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::echo::synthetic_echo_stream;
    use bevy::tasks::IoTaskPool;
    use crossbeam_channel::unbounded;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, drain_stream);
        app
    }

    #[test]
    fn echo_stream_appends_text_and_idles() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<StreamEvent>();
        for e in synthetic_echo_stream("hi") {
            tx.send(e).unwrap();
        }
        drop(tx);

        let task = IoTaskPool::get().spawn(async {});
        let entity = app
            .world_mut()
            .spawn((
                AgentMessages::default(),
                AgentRunState::Streaming {
                    rx,
                    _task: task,
                    partial: None,
                },
            ))
            .id();

        app.update();

        let world = app.world();
        let state = world.get::<AgentRunState>(entity).unwrap();
        assert!(matches!(state, AgentRunState::Idle));
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::Assistant { blocks } => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    AssistantBlock::Text(t) => assert_eq!(t, "echo: hi"),
                    _ => panic!("expected text block"),
                }
            }
            _ => panic!("expected assistant message"),
        }
    }
}
