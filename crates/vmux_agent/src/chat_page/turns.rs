//! Folds a flat agent transcript (`vmux_service::message::Message`) into rendered `ChatItem`s:
//! user bubbles and grouped assistant turns. Pure + unit-tested — the brain for the dumb chat
//! page (see the context-collapse design).

use crate::chat_page::event::{ChatBlock, ChatItem, ChatPlanStep, ChatTurn};
use vmux_service::message::{AssistantBlock, Message, PlanStep};

/// Group `messages` into `ChatItem`s: one `ChatItem::User` per user message, followed by one
/// `ChatItem::Turn` per started turn. `durations[i]` is the finished seconds of the `i`-th
/// emitted turn (by ordinal); out-of-range → `None`. When `running`, the last turn is marked
/// live and forced to `duration_secs = None`.
pub fn group_turns(messages: &[Message], durations: &[u32], running: bool) -> Vec<ChatItem> {
    let mut items: Vec<ChatItem> = Vec::new();
    let mut current: Option<ChatTurn> = None;
    let mut ordinal: usize = 0;

    for msg in messages {
        match msg {
            Message::User { text } => {
                flush(&mut items, &mut current, &mut ordinal, durations);
                items.push(ChatItem::User { text: text.clone() });
                current = Some(ChatTurn::default());
            }
            Message::Assistant { blocks } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                for block in blocks {
                    match block {
                        AssistantBlock::Text(t) => turn.answer.push(ChatBlock::Text(t.clone())),
                        AssistantBlock::Thinking(t) => {
                            turn.steps.push(ChatBlock::Thinking(t.clone()))
                        }
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => turn.steps.push(ChatBlock::ToolUse {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            args: args.clone(),
                        }),
                        AssistantBlock::Diff {
                            call_id,
                            path,
                            old_text,
                            new_text,
                        } => turn.steps.push(ChatBlock::Diff {
                            call_id: call_id.clone(),
                            path: path.clone(),
                            old_text: old_text.clone(),
                            new_text: new_text.clone(),
                        }),
                        AssistantBlock::Plan { steps } => turn.steps.push(ChatBlock::Plan {
                            steps: steps.iter().map(map_plan_step).collect(),
                        }),
                    }
                }
            }
            Message::ToolResult {
                content, is_error, ..
            } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                turn.steps.push(ChatBlock::ToolResult {
                    content: content.clone(),
                    is_error: *is_error,
                });
            }
        }
    }
    flush(&mut items, &mut current, &mut ordinal, durations);

    if running && let Some(ChatItem::Turn(last)) = items.last_mut() {
        last.running = true;
        last.duration_secs = None;
    }
    items
}

fn flush(
    items: &mut Vec<ChatItem>,
    current: &mut Option<ChatTurn>,
    ordinal: &mut usize,
    durations: &[u32],
) {
    if let Some(mut turn) = current.take() {
        turn.step_count = turn.steps.len() as u32;
        turn.duration_secs = durations.get(*ordinal).copied();
        *ordinal += 1;
        items.push(ChatItem::Turn(turn));
    }
}

fn map_plan_step(step: &PlanStep) -> ChatPlanStep {
    ChatPlanStep {
        content: step.content.clone(),
        status: step.status.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(blocks: Vec<AssistantBlock>) -> Message {
        Message::Assistant { blocks }
    }
    fn tool(id: &str) -> AssistantBlock {
        AssistantBlock::ToolUse {
            call_id: id.into(),
            name: "run".into(),
            args: "{}".into(),
        }
    }

    #[test]
    fn splits_steps_and_answer_folds_tool_result() {
        let msgs = vec![
            Message::User { text: "hi".into() },
            assistant(vec![AssistantBlock::Thinking("t".into()), tool("c1")]),
            Message::ToolResult {
                call_id: "c1".into(),
                content: "ok".into(),
                is_error: false,
            },
            assistant(vec![AssistantBlock::Text("done".into())]),
        ];
        let items = group_turns(&msgs, &[], false);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], ChatItem::User { text } if text == "hi"));
        let ChatItem::Turn(t) = &items[1] else {
            panic!()
        };
        assert_eq!(t.step_count, 3);
        assert_eq!(t.steps.len(), 3);
        assert!(matches!(t.steps[2], ChatBlock::ToolResult { .. }));
        assert_eq!(t.answer.len(), 1);
        assert!(!t.running);
    }

    #[test]
    fn one_turn_per_user_durations_by_ordinal() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::User { text: "b".into() },
            assistant(vec![AssistantBlock::Text("2".into())]),
        ];
        let items = group_turns(&msgs, &[5, 9], false);
        assert_eq!(items.len(), 4);
        let ChatItem::Turn(t0) = &items[1] else {
            panic!()
        };
        let ChatItem::Turn(t1) = &items[3] else {
            panic!()
        };
        assert_eq!(t0.duration_secs, Some(5));
        assert_eq!(t1.duration_secs, Some(9));
    }

    #[test]
    fn missing_duration_is_none() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::User { text: "b".into() },
            assistant(vec![AssistantBlock::Text("2".into())]),
        ];
        let items = group_turns(&msgs, &[5], false);
        let ChatItem::Turn(t1) = &items[3] else {
            panic!()
        };
        assert_eq!(t1.duration_secs, None);
    }

    #[test]
    fn running_marks_and_nulls_last_turn() {
        let msgs = vec![
            Message::User { text: "a".into() },
            assistant(vec![AssistantBlock::Text("1".into())]),
        ];
        let items = group_turns(&msgs, &[5], true);
        let ChatItem::Turn(t) = &items[1] else {
            panic!()
        };
        assert!(t.running);
        assert_eq!(t.duration_secs, None);
    }

    #[test]
    fn running_emits_empty_tail_turn_after_user() {
        let msgs = vec![Message::User { text: "a".into() }];
        let items = group_turns(&msgs, &[], true);
        assert_eq!(items.len(), 2);
        let ChatItem::Turn(t) = &items[1] else {
            panic!()
        };
        assert!(t.running);
        assert_eq!(t.step_count, 0);
        assert!(t.answer.is_empty());
    }
}
