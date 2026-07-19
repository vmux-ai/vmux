//! Folds a flat agent transcript (`vmux_service::message::Message`) into rendered `ChatItem`s:
//! user bubbles and grouped assistant turns. Pure + unit-tested — the brain for the dumb chat
//! page (see the context-collapse design).

use crate::chat_page::event::{ChatBlock, ChatItem, ChatPlanStep, ChatSubmitAttachment, ChatTurn};
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
            Message::User { text, attachments } => {
                flush(&mut items, &mut current, &mut ordinal, durations);
                let (context, text) = vmux_service::protocol::split_private_context_prompt(text)
                    .map(|(context, display)| (Some(context.to_string()), display))
                    .unwrap_or((None, text));
                if text.trim().is_empty() && attachments.is_empty() {
                    current = Some(ChatTurn::default());
                    continue;
                }
                items.push(ChatItem::User {
                    text: text.to_string(),
                    context,
                    attachments: attachments
                        .iter()
                        .map(|attachment| ChatSubmitAttachment {
                            path: attachment.path.clone(),
                            name: attachment.name.clone(),
                            mime_type: attachment.mime_type.clone(),
                            size: attachment.size,
                        })
                        .collect(),
                });
                current = Some(ChatTurn::default());
            }
            Message::Assistant { blocks } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                for block in blocks {
                    match block {
                        AssistantBlock::Text(t) => push_assistant_text(turn, t),
                        AssistantBlock::Thinking(t) => {
                            turn.blocks.push(ChatBlock::Thinking(t.clone()))
                        }
                        AssistantBlock::ToolUse {
                            call_id,
                            name,
                            args,
                        } => turn.blocks.push(ChatBlock::ToolUse {
                            call_id: call_id.clone(),
                            name: name.clone(),
                            args: args.clone(),
                        }),
                        AssistantBlock::Diff {
                            call_id,
                            path,
                            old_text,
                            new_text,
                        } => turn.blocks.push(ChatBlock::Diff {
                            call_id: call_id.clone(),
                            path: path.clone(),
                            old_text: old_text.clone(),
                            new_text: new_text.clone(),
                        }),
                        AssistantBlock::Plan { steps } => turn.blocks.push(ChatBlock::Plan {
                            steps: steps.iter().map(map_plan_step).collect(),
                        }),
                    }
                }
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                let turn = current.get_or_insert_with(ChatTurn::default);
                turn.blocks.push(ChatBlock::ToolResult {
                    call_id: call_id.clone(),
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
        turn.step_count = turn
            .blocks
            .iter()
            .enumerate()
            .filter(|(index, block)| {
                !matches!(block, ChatBlock::Text(_)) && turn.parent_tool_index(*index).is_none()
            })
            .count() as u32;
        turn.duration_secs = durations.get(*ordinal).copied();
        *ordinal += 1;
        items.push(ChatItem::Turn(turn));
    }
}

fn push_assistant_text(turn: &mut ChatTurn, text: &str) {
    let mut prose = String::new();
    for line in text.split_inclusive('\n') {
        if let Some((attempt, total)) = reconnect_progress(line.trim()) {
            push_prose(turn, &mut prose);
            push_reconnect(turn, attempt, total);
        } else {
            prose.push_str(line);
        }
    }
    push_prose(turn, &mut prose);
}

fn push_prose(turn: &mut ChatTurn, prose: &mut String) {
    if prose.trim().is_empty() {
        prose.clear();
        return;
    }
    turn.blocks
        .push(ChatBlock::Text(std::mem::take(prose).trim().to_string()));
}

fn push_reconnect(turn: &mut ChatTurn, attempt: u32, total: u32) {
    let block = ChatBlock::Reconnect { attempt, total };
    if matches!(turn.blocks.last(), Some(ChatBlock::Reconnect { .. })) {
        *turn.blocks.last_mut().expect("reconnect tail") = block;
    } else {
        turn.blocks.push(block);
    }
}

fn reconnect_progress(text: &str) -> Option<(u32, u32)> {
    let rest = text.strip_prefix("Reconnecting")?;
    let rest = rest.trim_start_matches('.').trim_start_matches('…').trim();
    let (attempt, total) = rest.split_once('/')?;
    Some((attempt.trim().parse().ok()?, total.trim().parse().ok()?))
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
            Message::user("hi"),
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
        assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "hi"));
        let ChatItem::Turn(t) = &items[1] else {
            panic!()
        };
        assert_eq!(t.step_count, 2);
        assert_eq!(t.blocks.len(), 4);
        assert!(matches!(t.blocks[2], ChatBlock::ToolResult { .. }));
        assert!(matches!(&t.blocks[3], ChatBlock::Text(text) if text == "done"));
        assert!(!t.running);
    }

    #[test]
    fn user_attachments_are_projected_into_chat_items() {
        let messages = vec![Message::user_with_attachments(
            "inspect",
            vec![vmux_service::protocol::AgentAttachment {
                path: "/tmp/image.png".into(),
                name: "image.png".into(),
                mime_type: "image/png".into(),
                size: 3,
            }],
        )];

        let items = group_turns(&messages, &[], false);

        assert!(matches!(
            &items[0],
            ChatItem::User {
                text, attachments, ..
            }
                if text == "inspect"
                    && attachments.len() == 1
                    && attachments[0].path == "/tmp/image.png"
        ));
    }

    #[test]
    fn one_turn_per_user_durations_by_ordinal() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::user("b"),
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
    fn private_continuation_starts_hidden_turn() {
        let private = vmux_service::protocol::compose_agent_prompt(
            "",
            Some("Workspace selected. Continue the original request."),
        );
        let messages = vec![
            Message::user("fix it"),
            assistant(vec![AssistantBlock::Text("Choose a workspace.".into())]),
            Message::user(private),
            assistant(vec![AssistantBlock::Text("Which branch?".into())]),
        ];

        let items = group_turns(&messages, &[], false);

        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "fix it"));
        assert!(matches!(
            &items[1],
            ChatItem::Turn(turn)
                if matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "Choose a workspace.")
        ));
        assert!(matches!(
            &items[2],
            ChatItem::Turn(turn)
                if matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "Which branch?")
        ));
    }

    #[test]
    fn private_context_is_collapsed_separately_from_display_prompt() {
        let private = vmux_service::protocol::compose_agent_prompt(
            "show me something fun",
            Some("workspace policy"),
        );
        let messages = vec![Message::user(format!("show me something fun{private}"))];

        let items = group_turns(&messages, &[], false);

        assert!(matches!(
            &items[0],
            ChatItem::User { text, context, .. }
                if text == "show me something fun"
                    && context.as_deref() == Some("workspace policy")
        ));
    }

    #[test]
    fn missing_duration_is_none() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text("1".into())]),
            Message::user("b"),
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
            Message::user("a"),
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
        let msgs = vec![Message::user("a")];
        let items = group_turns(&msgs, &[], true);
        assert_eq!(items.len(), 2);
        let ChatItem::Turn(t) = &items[1] else {
            panic!()
        };
        assert!(t.running);
        assert_eq!(t.step_count, 0);
        assert!(t.blocks.is_empty());
    }

    #[test]
    fn preserves_step_and_prose_order() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![
                AssistantBlock::Text("before".into()),
                tool("c1"),
                AssistantBlock::Text("after".into()),
            ]),
        ];
        let items = group_turns(&msgs, &[], false);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert!(matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "before"));
        assert!(matches!(&turn.blocks[1], ChatBlock::ToolUse { .. }));
        assert!(matches!(&turn.blocks[2], ChatBlock::Text(text) if text == "after"));
    }

    #[test]
    fn unmatched_tool_result_remains_a_step() {
        let msgs = vec![
            Message::user("a"),
            Message::ToolResult {
                call_id: "missing".into(),
                content: "output".into(),
                is_error: false,
            },
        ];
        let items = group_turns(&msgs, &[], false);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert_eq!(turn.step_count, 1);
    }

    #[test]
    fn guardian_and_results_count_as_one_tool_step() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![
                AssistantBlock::ToolUse {
                    call_id: "read-1".into(),
                    name: "read_file".into(),
                    args: "{}".into(),
                },
                AssistantBlock::ToolUse {
                    call_id: "review-1".into(),
                    name: "guardian_review".into(),
                    args: "{}".into(),
                },
            ]),
            Message::ToolResult {
                call_id: "read-1".into(),
                content: "output".into(),
                is_error: false,
            },
        ];
        let items = group_turns(&msgs, &[], false);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert_eq!(turn.step_count, 1);
    }

    #[test]
    fn collapses_consecutive_reconnect_updates() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text(
                "Reconnecting... 1/5\n\nReconnecting… 2/5\nReconnecting 3/5".into(),
            )]),
        ];
        let items = group_turns(&msgs, &[], true);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert_eq!(turn.blocks.len(), 1);
        assert!(matches!(
            turn.blocks[0],
            ChatBlock::Reconnect {
                attempt: 3,
                total: 5
            }
        ));
        assert_eq!(turn.step_count, 1);
    }

    #[test]
    fn reconnect_updates_do_not_swallow_prose() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text(
                "before\nReconnecting... 2/5\nafter".into(),
            )]),
        ];
        let items = group_turns(&msgs, &[], false);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert!(matches!(&turn.blocks[0], ChatBlock::Text(text) if text == "before"));
        assert!(matches!(
            turn.blocks[1],
            ChatBlock::Reconnect {
                attempt: 2,
                total: 5
            }
        ));
        assert!(matches!(&turn.blocks[2], ChatBlock::Text(text) if text == "after"));
    }
}
