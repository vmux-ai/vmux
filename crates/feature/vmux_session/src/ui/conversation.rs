use base64::Engine;
use dioxus::prelude::*;
use pulldown_cmark::{Event, Parser};
use std::collections::HashMap;
use vmux_api::chat::ChatItem;
use vmux_ui::i18n::translate;

use super::item::{ChatItemRow, UserBubble};
use super::state::Chat;
use super::transcript::HandoffDivider;

#[component]
pub(super) fn Section(turn: Turn, latest_index: usize, chat: Chat) -> Element {
    let prompt = turn.prompt();
    let has_header = prompt.is_some();
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
                header { class: "pointer-events-none sticky top-0 z-30 h-12", "data-vmux-sticky-header": "", inert: true, aria_hidden: true,
                    div { class: "flex h-12 flex-col justify-center bg-[linear-gradient(to_bottom,var(--background)_0%,color-mix(in_srgb,var(--background)_90%,transparent)_75%,transparent_100%)] pb-3 pt-1", "data-vmux-sticky-content": "",
                        if let Some(excerpt) = prompt {
                            PinnedMessage {
                                excerpt,
                                chat,
                                label: translate("session-turn-view-prompt"),
                                name: user_name.clone(),
                                color: user_color.clone(),
                            }
                        }
                    }
                }
            }
            div { class: if has_header { "-mt-12 flex flex-col gap-5" } else { "flex flex-col gap-5" },
                for (offset, item) in turn.items.into_iter().enumerate() {
                    {
                        let index = start + offset;
                        let item_key = turn.item_keys.get(offset).cloned().unwrap_or_else(|| index.to_string());
                        let item_id = Turn::anchor(&item_key);
                        let user_anchor = matches!(&item, ChatItem::User { .. });
                        rsx! {
                            div {
                                key: "{item_key}",
                                id: "{item_id}",
                                "data-vmux-scroll-anchor": user_anchor.then_some(""),
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
) -> Element {
    let text = excerpt.label();
    let target = excerpt.target;
    rsx! {
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
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Turn {
    key: String,
    start: usize,
    items: Vec<ChatItem>,
    item_keys: Vec<String>,
}

impl Turn {
    pub(super) fn group(items: Vec<ChatItem>, loaded_start: usize) -> Vec<Self> {
        let mut turns: Vec<Self> = Vec::new();
        for (offset, item) in items.into_iter().enumerate() {
            if matches!(item, ChatItem::User { .. }) || turns.is_empty() {
                turns.push(Self {
                    key: (loaded_start + offset).to_string(),
                    start: loaded_start + offset,
                    items: Vec::new(),
                    item_keys: Vec::new(),
                });
            }
            turns.last_mut().unwrap().items.push(item);
        }
        turns
    }

    pub(super) fn key(&self) -> String {
        self.key.clone()
    }

    fn anchor(key: &str) -> String {
        let key = if key.parse::<usize>().is_ok() {
            key.to_string()
        } else {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key)
        };
        format!("chat-item-{key}")
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
                    target: Self::anchor(
                        &self
                            .item_keys
                            .get(offset)
                            .cloned()
                            .unwrap_or_else(|| (self.start + offset).to_string()),
                    ),
                    text,
                });
            }
        }
        None
    }
}

#[derive(Default)]
pub(super) struct Keys {
    sections: HashMap<String, String>,
    items: HashMap<String, String>,
}

impl Keys {
    pub(super) fn assign(&mut self, turns: &mut [Turn]) {
        let mut next_sections = HashMap::new();
        let mut next_items = HashMap::new();
        for turn in turns {
            let mut section_ids = Vec::new();
            turn.item_keys.clear();
            for (offset, item) in turn.items.iter().enumerate() {
                let mut ids = item.identity_keys();
                if ids.is_empty() {
                    ids.push((turn.start + offset).to_string());
                }
                let key = ids
                    .iter()
                    .find_map(|id| self.items.get(id))
                    .cloned()
                    .unwrap_or_else(|| ids[0].clone());
                for id in &ids {
                    next_items.insert(id.clone(), key.clone());
                }
                turn.item_keys.push(key);
                section_ids.extend(ids);
            }
            turn.key = section_ids
                .iter()
                .find_map(|id| self.sections.get(id))
                .cloned()
                .unwrap_or_else(|| section_ids[0].clone());
            for id in section_ids {
                next_sections.insert(id, turn.key.clone());
            }
        }
        self.sections = next_sections;
        self.items = next_items;
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
    use vmux_api::chat::{ChatTurn, ChatTurnRow};
    use vmux_api::prompt_media::ChatAttachment;

    #[test]
    fn same_turn_prefix_preserves_existing_section_and_message_keys() {
        let mut keys = Keys::default();
        let mut initial = Turn::group(
            vec![ChatItem::Turn(ChatTurn {
                block_ids: vec!["existing-response".into()],
                ..Default::default()
            })],
            0,
        );
        keys.assign(&mut initial);
        let section = initial[0].key();
        let item = initial[0].item_keys[0].clone();
        let mut expanded = Turn::group(
            vec![
                ChatItem::User {
                    event_id: "first-prompt".into(),
                    text: "First prompt".into(),
                    context: None,
                    attachments: Vec::new(),
                    created_at_ms: 0,
                },
                ChatItem::Turn(ChatTurn {
                    block_ids: vec!["older-response".into(), "existing-response".into()],
                    ..Default::default()
                }),
            ],
            0,
        );
        keys.assign(&mut expanded);
        assert_eq!(expanded[0].key(), section);
        assert_eq!(expanded[0].item_keys[1], item);
        assert_ne!(expanded[0].item_keys[0], item);
        assert_eq!(
            expanded[0].prompt().unwrap().target,
            Turn::anchor("first-prompt")
        );
    }

    #[test]
    fn sticky_prompts_keep_their_own_message_targets_across_turns() {
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
        assert_eq!(turns[0].prompt().unwrap().target, "chat-item-8");
        assert_eq!(turns[1].prompt().unwrap().target, "chat-item-10");
        assert_eq!(turns[1].prompt().unwrap().label(), "Second request");
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
        assert_eq!(turns[1].prompt().unwrap().target, "chat-item-25");
    }

    #[test]
    fn sticky_prompts_show_attachment_names_when_the_message_has_no_text() {
        let turns = Turn::group(
            vec![ChatItem::User {
                event_id: String::new(),
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
    }
}
