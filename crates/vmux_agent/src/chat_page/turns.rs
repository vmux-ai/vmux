//! Folds a flat agent transcript (`vmux_service::message::Message`) into rendered `ChatItem`s:
//! user bubbles and grouped assistant turns. Pure + unit-tested — the brain for the dumb chat
//! page (see the context-collapse design).

use crate::chat_page::event::{
    ChatBlock, ChatItem, ChatPlanStep, ChatSubagent, ChatSubmitAttachment, ChatTurn,
};
use vmux_service::message::{AssistantBlock, Message, PlanStep, SubagentBlock};

/// Group `messages` into `ChatItem`s: one `ChatItem::User` per user message, followed by one
/// `ChatItem::Turn` per started turn. `durations[i]` is the finished seconds of the `i`-th
/// emitted turn (by ordinal); out-of-range → `None`. When `running`, the last turn is marked
/// live and forced to `duration_secs = None`.
#[cfg(test)]
pub fn group_turns(messages: &[Message], durations: &[u32], running: bool) -> Vec<ChatItem> {
    group_turns_page(&[], messages, durations, running, 0, usize::MAX).items
}

pub struct ChatItemPage {
    pub items: Vec<ChatItem>,
    pub start: usize,
    pub end: usize,
    pub total: usize,
}

pub fn grouped_item_count(imported: &[Message], live: &[Message]) -> usize {
    let mut count = 0usize;
    let mut current_turn = false;
    for message in imported.iter().chain(live) {
        match message {
            Message::User { text, attachments } => {
                if current_turn {
                    count += 1;
                }
                let text = vmux_service::protocol::extract_display_prompt(text).unwrap_or(text);
                if !text.trim().is_empty() || !attachments.is_empty() {
                    count += 1;
                }
                current_turn = true;
            }
            Message::Assistant { .. } | Message::ToolResult { .. } => current_turn = true,
        }
    }
    if current_turn {
        count += 1;
    }
    count
}

pub fn group_turns_tail(
    imported: &[Message],
    live: &[Message],
    durations: &[u32],
    running: bool,
    limit: usize,
) -> ChatItemPage {
    let total = grouped_item_count(imported, live);
    group_turns_page_with_total(
        imported,
        live,
        durations,
        running,
        total.saturating_sub(limit),
        total,
        total,
    )
}

pub fn group_turns_before(
    imported: &[Message],
    live: &[Message],
    durations: &[u32],
    running: bool,
    before: usize,
    limit: usize,
) -> ChatItemPage {
    let total = grouped_item_count(imported, live);
    let end = before.min(total);
    group_turns_page_with_total(
        imported,
        live,
        durations,
        running,
        end.saturating_sub(limit),
        end,
        total,
    )
}

#[cfg(test)]
fn group_turns_page(
    imported: &[Message],
    live: &[Message],
    durations: &[u32],
    running: bool,
    start: usize,
    end: usize,
) -> ChatItemPage {
    let total = grouped_item_count(imported, live);
    group_turns_page_with_total(imported, live, durations, running, start, end, total)
}

fn group_turns_page_with_total(
    imported: &[Message],
    live: &[Message],
    durations: &[u32],
    running: bool,
    start: usize,
    end: usize,
    total: usize,
) -> ChatItemPage {
    let start = start.min(total);
    let end = end.min(total).max(start);
    let mut builder = PageBuilder::new(start, end, durations);

    for message in imported.iter().chain(live) {
        match message {
            Message::User { text, attachments } => {
                builder.flush_turn();
                let (context, text) = vmux_service::protocol::split_private_context_prompt(text)
                    .map(|(context, display)| (Some(context), display))
                    .unwrap_or((None, text));
                if !text.trim().is_empty() || !attachments.is_empty() {
                    builder.push_user(text, context, attachments);
                }
                builder.start_turn();
            }
            Message::Assistant { blocks } => {
                builder.start_turn();
                if let Some(turn) = builder.current.as_mut() {
                    push_assistant_blocks(turn, blocks);
                }
            }
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                builder.start_turn();
                if let Some(turn) = builder.current.as_mut() {
                    turn.blocks.push(ChatBlock::ToolResult {
                        call_id: call_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    });
                }
            }
        }
    }
    builder.flush_turn();
    if running
        && end == total
        && let Some(ChatItem::Turn(last)) = builder.items.last_mut()
    {
        last.running = true;
        last.duration_secs = None;
    }
    ChatItemPage {
        items: builder.items,
        start,
        end,
        total,
    }
}

struct PageBuilder<'a> {
    items: Vec<ChatItem>,
    start: usize,
    end: usize,
    item_index: usize,
    turn_ordinal: usize,
    durations: &'a [u32],
    current_exists: bool,
    current: Option<ChatTurn>,
}

impl<'a> PageBuilder<'a> {
    fn new(start: usize, end: usize, durations: &'a [u32]) -> Self {
        Self {
            items: Vec::with_capacity(end.saturating_sub(start)),
            start,
            end,
            item_index: 0,
            turn_ordinal: 0,
            durations,
            current_exists: false,
            current: None,
        }
    }

    fn captures(&self) -> bool {
        self.item_index >= self.start && self.item_index < self.end
    }

    fn start_turn(&mut self) {
        if self.current_exists {
            return;
        }
        self.current_exists = true;
        if self.captures() {
            self.current = Some(ChatTurn::default());
        }
    }

    fn push_user(
        &mut self,
        text: &str,
        context: Option<&str>,
        attachments: &[vmux_service::protocol::AgentAttachment],
    ) {
        if self.captures() {
            self.items.push(ChatItem::User {
                text: text.to_string(),
                context: context.map(str::to_string),
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
        }
        self.item_index += 1;
    }

    fn flush_turn(&mut self) {
        if !self.current_exists {
            return;
        }
        if let Some(mut turn) = self.current.take() {
            turn.step_count = turn
                .blocks
                .iter()
                .enumerate()
                .filter(|(index, block)| {
                    !matches!(block, ChatBlock::Text(_)) && turn.parent_tool_index(*index).is_none()
                })
                .count() as u32;
            turn.duration_secs = self.durations.get(self.turn_ordinal).copied();
            self.items.push(ChatItem::Turn(turn));
        }
        self.current_exists = false;
        self.turn_ordinal += 1;
        self.item_index += 1;
    }
}

fn push_assistant_blocks(turn: &mut ChatTurn, blocks: &[AssistantBlock]) {
    for block in blocks {
        match block {
            AssistantBlock::Text(text) => push_assistant_text(turn, text),
            AssistantBlock::Thinking(text) => turn.blocks.push(ChatBlock::Thinking(text.clone())),
            AssistantBlock::ToolUse {
                call_id,
                name,
                args,
                parent_call_id,
            } => turn.blocks.push(ChatBlock::ToolUse {
                call_id: call_id.clone(),
                name: name.clone(),
                args: args.clone(),
                parent_call_id: parent_call_id.clone(),
            }),
            AssistantBlock::Subagent(subagent) => turn
                .blocks
                .push(ChatBlock::Subagent(Box::new(map_subagent(subagent)))),
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

fn map_subagent(subagent: &SubagentBlock) -> ChatSubagent {
    ChatSubagent {
        call_id: subagent.call_id.clone(),
        provider: subagent.provider.clone(),
        title: subagent.title.clone(),
        status: subagent.status.clone(),
        action: subagent.action.clone(),
        agent_name: subagent.agent_name.clone(),
        thread_id: subagent.thread_id.clone(),
        parent_thread_id: subagent.parent_thread_id.clone(),
        child_thread_ids: subagent.child_thread_ids.clone(),
        parent_call_id: subagent.parent_call_id.clone(),
        prompt: subagent.prompt.clone(),
        model: subagent.model.clone(),
        reasoning_effort: subagent.reasoning_effort.clone(),
        raw_input: subagent.raw_input.clone(),
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
            parent_call_id: None,
        }
    }

    fn subagent(id: &str) -> AssistantBlock {
        AssistantBlock::Subagent(Box::new(SubagentBlock {
            call_id: id.into(),
            provider: "Claude".into(),
            title: "Inspect ACP support".into(),
            status: "in_progress".into(),
            action: "delegate".into(),
            agent_name: Some("Explore".into()),
            thread_id: None,
            parent_thread_id: None,
            child_thread_ids: Vec::new(),
            parent_call_id: None,
            prompt: Some("Trace metadata".into()),
            model: Some("sonnet".into()),
            reasoning_effort: None,
            raw_input: "{}".into(),
        }))
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
    fn tail_page_only_clones_recent_items() {
        let messages = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text("one".into())]),
            Message::user("b"),
            assistant(vec![AssistantBlock::Text("two".into())]),
            Message::user("c"),
            assistant(vec![AssistantBlock::Text("three".into())]),
        ];

        let page = group_turns_tail(&[], &messages, &[1, 2, 3], false, 3);

        assert_eq!((page.start, page.end, page.total), (3, 6, 6));
        assert_eq!(page.items.len(), 3);
        assert!(matches!(&page.items[0], ChatItem::Turn(turn) if turn.duration_secs == Some(2)));
        assert!(matches!(&page.items[1], ChatItem::User { text, .. } if text == "c"));
    }

    #[test]
    fn older_page_ends_at_requested_cursor() {
        let messages = vec![
            Message::user("a"),
            assistant(vec![AssistantBlock::Text("one".into())]),
            Message::user("b"),
            assistant(vec![AssistantBlock::Text("two".into())]),
        ];

        let page = group_turns_before(&[], &messages, &[1, 2], false, 3, 2);

        assert_eq!((page.start, page.end, page.total), (1, 3, 4));
        assert!(matches!(&page.items[0], ChatItem::Turn(turn) if turn.duration_secs == Some(1)));
        assert!(matches!(&page.items[1], ChatItem::User { text, .. } if text == "b"));
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
                    parent_call_id: None,
                },
                AssistantBlock::ToolUse {
                    call_id: "review-1".into(),
                    name: "guardian_review".into(),
                    args: "{}".into(),
                    parent_call_id: None,
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
    fn subagent_children_and_results_fold_into_one_visible_step() {
        let msgs = vec![
            Message::user("a"),
            assistant(vec![
                subagent("agent-1"),
                AssistantBlock::ToolUse {
                    call_id: "read-1".into(),
                    name: "read_file".into(),
                    args: "{}".into(),
                    parent_call_id: Some("agent-1".into()),
                },
            ]),
            Message::ToolResult {
                call_id: "read-1".into(),
                content: "file contents".into(),
                is_error: false,
            },
            Message::ToolResult {
                call_id: "agent-1".into(),
                content: "done".into(),
                is_error: false,
            },
        ];

        let items = group_turns(&msgs, &[], false);
        let ChatItem::Turn(turn) = &items[1] else {
            panic!()
        };
        assert_eq!(turn.step_count, 1);
        assert!(matches!(&turn.blocks[0], ChatBlock::Subagent(_)));
        assert_eq!(turn.parent_tool_index(1), Some(0));
        assert_eq!(turn.parent_tool_index(2), Some(0));
        assert_eq!(turn.parent_tool_index(3), Some(0));
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
