use dioxus::prelude::*;
use pulldown_cmark::{Event, Parser};
use std::collections::HashMap;
use vmux_api::chat::{ChatItem, ChatTurnRow};
use vmux_ui::i18n::translate;

use super::item::{AssistantTurn, ChatItemRow, UserBubble};
use super::state::Chat;
use super::transcript::HandoffDivider;

#[component]
pub(super) fn Section(turn: Turn, latest_index: usize, chat: Chat) -> Element {
    let prompt = turn.prompt();
    let response = turn.response();
    let has_header = prompt.is_some() || response.is_some();
    let both = prompt.is_some() && response.is_some();
    let header_height = if both { "h-24" } else { "h-12" };
    let overlap = if both { "-mt-24" } else { "-mt-12" };
    let start = turn.start;
    let handoff_source = chat.handoff.source();
    let handoff_truncated = chat.handoff.truncated();
    let icon = chat.identity.icon();
    let agent_avatar = (!icon.is_empty()).then_some(icon);
    let agent = chat.agent();
    let agent_name = chat.header_name();
    let agent_color = chat.accent().css;
    let user_name = chat.user.name();
    let user_color = chat.user.color();
    rsx! {
        section { class: "relative", "data-vmux-sticky-section": "",
            div { class: "pointer-events-none absolute inset-x-0 top-0 h-px", "data-vmux-sticky-sentinel": "", aria_hidden: true }
            if has_header {
                header { class: "pointer-events-none sticky top-0 z-30 {header_height}", "data-vmux-sticky-header": "", inert: true, aria_hidden: true,
                    div { class: "flex {header_height} flex-col justify-center gap-1 bg-[linear-gradient(to_bottom,var(--background)_0%,color-mix(in_srgb,var(--background)_90%,transparent)_75%,transparent_100%)] pb-3 pt-1", "data-vmux-sticky-content": "",
                        if let Some(excerpt) = prompt {
                            PinnedMessage {
                                excerpt,
                                chat,
                                label: translate("session-turn-view-prompt"),
                                name: user_name.clone(),
                                color: user_color.clone(),
                                right: true,
                            }
                        }
                        if let Some(excerpt) = response {
                            PinnedMessage {
                                excerpt,
                                chat,
                                label: translate("session-turn-view-response"),
                                name: agent_name.clone(),
                                color: agent_color.clone(),
                                agent_id: Some(agent.clone()),
                                avatar: agent_avatar.clone(),
                            }
                        }
                    }
                }
            }
            div { class: if has_header { format!("{overlap} flex flex-col gap-5") } else { "flex flex-col gap-5".to_string() },
                for (offset, item) in turn.items.into_iter().enumerate() {
                    {
                        let index = start + offset;
                        rsx! {
                            div {
                                key: "{index}",
                                id: "chat-item-{index}",
                                "data-vmux-scroll-anchor": "",
                                ChatItemRow {
                                    absolute_index: index,
                                    item,
                                    agent_id: agent.clone(),
                                    agent_name: agent_name.clone(),
                                    agent_avatar: agent_avatar.clone(),
                                    agent_color: agent_color.clone(),
                                    user_name: user_name.clone(),
                                    user_color: user_color.clone(),
                                    latest: index == latest_index,
                                    on_media_load: move |_| {
                                        if *chat.transcript.at_bottom.peek() {
                                            chat.transcript.follow();
                                        }
                                    },
                                }
                            }
                            if !handoff_source.is_empty() && chat.handoff.boundary(index) {
                                HandoffDivider { source: handoff_source.clone(), truncated: handoff_truncated }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PinnedMessage(
    excerpt: Excerpt,
    chat: Chat,
    label: String,
    name: String,
    color: String,
    #[props(default)] right: bool,
    #[props(default)] agent_id: Option<String>,
    #[props(default)] avatar: Option<String>,
) -> Element {
    let text = excerpt.label();
    let target = excerpt.target;
    rsx! {
        if right {
            UserBubble { avatar_name: name, avatar_color: color, compact: true,
                button {
                    r#type: "button",
                    class: "block min-w-0 truncate text-left text-sm leading-5 text-foreground/90",
                    aria_label: label,
                    title: excerpt.text,
                    onclick: move |_| chat.transcript.reveal(&target),
                    "{text}"
                }
            }
        } else {
            AssistantTurn { agent_id: agent_id.unwrap_or_default(), name, avatar_src: avatar, avatar_background: color, compact: true,
                button {
                    r#type: "button",
                    class: "block w-full min-w-0 truncate text-left text-sm leading-6 text-foreground/80",
                    aria_label: label,
                    title: excerpt.text,
                    onclick: move |_| chat.transcript.reveal(&target),
                    "{text}"
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Turn {
    key: usize,
    start: usize,
    items: Vec<ChatItem>,
}

impl Turn {
    pub(super) fn group(items: Vec<ChatItem>, loaded_start: usize) -> Vec<Self> {
        let mut turns: Vec<Self> = Vec::new();
        for (offset, item) in items.into_iter().enumerate() {
            if matches!(item, ChatItem::User { .. }) || turns.is_empty() {
                turns.push(Self {
                    key: loaded_start + offset,
                    start: loaded_start + offset,
                    items: Vec::new(),
                });
            }
            turns.last_mut().unwrap().items.push(item);
        }
        turns
    }

    pub(super) fn key(&self) -> usize {
        self.key
    }

    fn prompt(&self) -> Option<Excerpt> {
        for (offset, item) in self.items.iter().enumerate() {
            let ChatItem::User {
                text, attachments, ..
            } = item
            else {
                continue;
            };
            let text = if text.trim().is_empty() {
                let mut names = Vec::new();
                for attachment in attachments {
                    names.push(attachment.name.as_str());
                }
                names.join(", ")
            } else {
                text.clone()
            };
            if !text.is_empty() {
                return Some(Excerpt {
                    target: format!("chat-item-{}", self.start + offset),
                    text,
                });
            }
        }
        None
    }

    fn response(&self) -> Option<Excerpt> {
        for (offset, item) in self.items.iter().enumerate().rev() {
            let ChatItem::Turn(turn) = item else {
                continue;
            };
            for row in turn.rows.iter().rev() {
                if let ChatTurnRow::Text { index, text } = row
                    && !text.trim().is_empty()
                {
                    return Some(Excerpt {
                        target: format!("chat-response-{}-{index}", self.start + offset),
                        text: text.clone(),
                    });
                }
            }
        }
        None
    }
}

#[derive(Default)]
pub(super) struct Keys {
    items: HashMap<usize, usize>,
}

impl Keys {
    pub(super) fn assign(&mut self, turns: &mut [Turn]) {
        let mut next = HashMap::new();
        for turn in turns {
            for offset in 0..turn.items.len() {
                if let Some(key) = self.items.get(&(turn.start + offset)) {
                    turn.key = *key;
                    break;
                }
            }
            for offset in 0..turn.items.len() {
                next.insert(turn.start + offset, turn.key);
            }
        }
        self.items = next;
    }
}

#[derive(Clone, PartialEq)]
struct Excerpt {
    target: String,
    text: String,
}

impl Excerpt {
    fn label(&self) -> String {
        for line in self.text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let mut label = String::new();
            for event in Parser::new(line.trim()) {
                if let Event::Text(text) | Event::Code(text) = event {
                    label.push_str(&text);
                }
            }
            if !label.is_empty() {
                return label;
            }
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::chat::ChatTurn;
    use vmux_api::prompt_media::ChatAttachment;

    #[test]
    fn sticky_previews_pair_each_prompt_with_that_turns_latest_response() {
        let turns = Turn::group(
            vec![
                ChatItem::user("First request"),
                ChatItem::Turn(ChatTurn {
                    rows: vec![
                        ChatTurnRow::Text {
                            index: 1,
                            text: "Initial response".into(),
                        },
                        ChatTurnRow::Text {
                            index: 4,
                            text: "## Final **answer**\n\nMore detail".into(),
                        },
                    ],
                    ..Default::default()
                }),
                ChatItem::user("Second request"),
                ChatItem::Turn(ChatTurn {
                    rows: vec![ChatTurnRow::Text {
                        index: 2,
                        text: "Another answer".into(),
                    }],
                    ..Default::default()
                }),
            ],
            8,
        );
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].prompt().unwrap().label(), "First request");
        let first = turns[0].response().unwrap();
        assert_eq!(first.label(), "Final answer");
        assert_eq!(first.target, "chat-response-9-4");
        assert_eq!(turns[1].prompt().unwrap().target, "chat-item-10");
        assert_eq!(turns[1].response().unwrap().label(), "Another answer");
    }

    #[test]
    fn sticky_history_without_a_loaded_prompt_does_not_borrow_the_next_prompts_text() {
        let turns = Turn::group(
            vec![
                ChatItem::Turn(ChatTurn {
                    rows: vec![ChatTurnRow::Text {
                        index: 3,
                        text: "Earlier answer".into(),
                    }],
                    ..Default::default()
                }),
                ChatItem::user("Next request"),
                ChatItem::Turn(ChatTurn {
                    rows: vec![ChatTurnRow::Text {
                        index: 1,
                        text: "Next answer".into(),
                    }],
                    ..Default::default()
                }),
            ],
            24,
        );
        assert!(turns[0].prompt().is_none());
        assert_eq!(turns[0].response().unwrap().target, "chat-response-24-3");
        assert_eq!(turns[1].prompt().unwrap().target, "chat-item-25");
    }

    #[test]
    fn sticky_prompts_show_attachment_names_when_the_message_has_no_text() {
        let turns = Turn::group(
            vec![ChatItem::User {
                text: String::new(),
                context: None,
                created_at_ms: 0,
                attachments: vec![ChatAttachment {
                    name: "recording.mov".into(),
                    ..Default::default()
                }],
            }],
            0,
        );
        assert_eq!(turns[0].prompt().unwrap().label(), "recording.mov");
        assert!(turns[0].response().is_none());
    }

    #[test]
    fn sticky_history_prepends_keep_existing_sections_and_messages_mounted() {
        let item = ChatItem::Turn(ChatTurn {
            rows: vec![ChatTurnRow::Text {
                index: 0,
                text: "Existing response".into(),
            }],
            ..Default::default()
        });
        let mut keys = Keys::default();
        let mut original = Turn::group(vec![item.clone()], 24);
        keys.assign(&mut original);
        let key = original[0].key();
        let mut expanded = Turn::group(vec![ChatItem::user("Earlier prompt"), item], 23);
        keys.assign(&mut expanded);
        assert_eq!(expanded[0].key(), key);
        assert_eq!(expanded[0].prompt().unwrap().target, "chat-item-23");
        assert_eq!(expanded[0].response().unwrap().target, "chat-response-24-0");
    }
}
