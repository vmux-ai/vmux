//! Transcript grouping shared by every surface that renders a conversation.
//!
//! Folds a flat agent transcript ([`crate::message::Message`]) into rendered `ChatItem`s: user
//! bubbles and grouped assistant turns. Pure and unit-tested — the brain for the dumb chat page
//! (see the context-collapse design).

use crate::message::{AssistantBlock, Message, PlanStep, SubagentBlock};
use vmux_wire::chat::{ChatBlock, ChatItem, ChatPlanStep, ChatSubagent, ChatTurn};
use vmux_wire::prompt_media::ChatSubmitAttachment;

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
                let text = crate::protocol::extract_display_prompt(text).unwrap_or(text);
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
                let (context, text) = crate::protocol::split_private_context_prompt(text)
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
        attachments: &[crate::protocol::AgentAttachment],
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
#[path = "chat.test.rs"]
mod tests;
