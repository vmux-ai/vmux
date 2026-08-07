//! The rendered transcript: user bubbles, assistant turns, and every activity row inside them.
//!
//! This is the surface both clients draw. Hosts supply a `Vec<ChatItem>` and the attachment
//! preview cache; everything below is pure view with no host events.

use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_wire::chat::{ChatBlock, ChatItem, ChatTurn, WORKING_VERB_IDS};
use vmux_wire::prompt_media::{ChatAttachment, ChatSubmitAttachment};

use crate::activity::{
    ActivityIcon, ActivityIconView, FileActivityIcon, ToolActivityIcon, should_expand_thinking,
    tool_presentation,
};
use crate::clipboard::copy_to_clipboard;
use crate::platform::{random_index, sleep_ms};

/// Uppercased file extension used as an attachment pill's fallback glyph.
pub fn file_extension_label(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_uppercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "FILE".to_string())
}

#[component]
pub fn UserBubble(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "chat-user-bubble flex max-w-[80%] self-end flex-col gap-2 rounded-[1.35rem] rounded-tr-md border p-2.5 text-sm [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", ..attributes,
            {children}
        }
    }
}

#[component]
pub fn AssistantTurn(
    #[props(default = true)] standalone: bool,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let placement = if standalone {
        "max-w-[94%] self-start"
    } else {
        "w-full"
    };
    rsx! {
        div { class: "chat-assistant-turn relative flex flex-col gap-2.5 overflow-hidden rounded-2xl border px-3.5 py-3 [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto] {placement}", ..attributes,
            {children}
        }
    }
}

#[component]
pub fn MessageCopyButton(text: String) -> Element {
    let label = translate("agent-copy");
    rsx! {
        button {
            class: "absolute right-2 top-2 z-10 flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground/60 transition hover:bg-foreground/[0.08] hover:text-foreground",
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                copy_to_clipboard(&text);
            },
            svg {
                class: "h-3.5 w-3.5",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
            }
        }
    }
}

#[component]
pub fn ChatItemRow(
    absolute_index: usize,
    item: ChatItem,
    attachment_previews: Signal<HashMap<String, ChatAttachment>>,
    latest_tool_block: Option<usize>,
) -> Element {
    let key = absolute_index;
    let item = &item;
    match item {
        ChatItem::User {
            text,
            context,
            attachments,
        } => rsx! {
            UserBubble {
                key: "{key}",
                class: "chat-user-bubble relative flex max-w-[80%] self-end flex-col gap-2 rounded-[1.35rem] rounded-tr-md border py-2.5 pl-2.5 pr-10 text-sm",
                style: "content-visibility:auto;contain-intrinsic-size:auto 96px;",
                if !text.is_empty() {
                    MessageCopyButton { text: text.clone() }
                }
                if let Some(context) = context {
                    details { class: "disclosure user-context-panel rounded-xl border",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 px-2.5 py-2 text-xs list-none [&::-webkit-details-marker]:hidden",
                            span { class: "agent-themed-activity flex h-5 w-5 shrink-0 items-center justify-center rounded-md",
                                svg {
                                    class: "h-3 w-3",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "1.8",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z" }
                                }
                            }
                            span { class: "font-medium", {translate("agent-prompt-context")} }
                            span {
                                class: "text-[10px] text-muted-foreground",
                                {translate_with(
                                    "agent-bytes",
                                    &[("count", TranslationValue::Number(context.len() as i64))],
                                )}
                            }
                            DisclosureIcon {}
                        }
                        pre { class: "user-context-content max-h-72 overflow-auto whitespace-pre-wrap rounded-lg px-3 py-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{context}" }
                    }
                }
                if !text.is_empty() {
                    div { class: "whitespace-pre-wrap px-1.5", "{text}" }
                }
                if !attachments.is_empty() {
                    div { class: "flex flex-wrap justify-end gap-2",
                        for attachment in attachments {
                            UserAttachment {
                                attachment: attachment.clone(),
                                previews: attachment_previews,
                            }
                        }
                    }
                }
            }
        },
        ChatItem::Turn(turn) => rsx! {
            TurnView {
                turn_index: key,
                turn: turn.clone(),
                latest_tool_index: latest_tool_block,
            }
        },
    }
}

/// One file chip under a user message.
#[component]
fn UserAttachment(
    attachment: ChatSubmitAttachment,
    previews: Signal<HashMap<String, ChatAttachment>>,
) -> Element {
    let preview_data_url = previews
        .peek()
        .get(&attachment.path)
        .map(|preview| preview.preview_data_url.clone())
        .unwrap_or_default();
    if attachment.mime_type.starts_with("image/") && !preview_data_url.is_empty() {
        return rsx! {
            figure {
                key: "message-attachment-{attachment.path}",
                class: "max-w-full overflow-hidden rounded-xl bg-black/10 ring-1 ring-inset ring-foreground/10",
                img {
                    src: "{preview_data_url}",
                    alt: "{attachment.name}",
                    loading: "lazy",
                    decoding: "async",
                    class: "max-h-80 max-w-full object-contain",
                }
                figcaption { class: "max-w-72 truncate px-2.5 py-1.5 text-[10px] text-muted-foreground", "{attachment.name}" }
            }
        };
    }
    rsx! {
        div {
            key: "message-attachment-{attachment.path}",
            class: "flex min-w-32 max-w-64 items-center gap-2 rounded-xl bg-foreground/[0.06] px-3 py-2 ring-1 ring-inset ring-foreground/10",
            span { class: "font-mono text-[10px] font-semibold tracking-wide text-muted-foreground", "{file_extension_label(&attachment.name)}" }
            span { class: "truncate text-xs text-muted-foreground", "{attachment.name}" }
        }
    }
}

/// One assistant turn: its prose, activity blocks and run-state.
#[component]
pub fn TurnView(turn_index: usize, turn: ChatTurn, latest_tool_index: Option<usize>) -> Element {
    let key = turn_index;
    let turn = &turn;
    let reconnecting = matches!(turn.blocks.last(), Some(ChatBlock::Reconnect { .. }));
    let block_count = turn.blocks.len();
    let blocks = turn
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(key, block)| {
            if turn.parent_tool_index(key).is_some() {
                return None;
            }
            let children = turn
                .blocks
                .iter()
                .enumerate()
                .filter(|(child_key, _)| turn.parent_tool_index(*child_key) == Some(key))
                .collect::<Vec<_>>();
            Some((key, block, children))
        })
        .collect::<Vec<_>>();
    let duration_label = turn.duration_secs.map(|duration| {
        if turn.step_count == 0 {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for",
                &[("duration", TranslationValue::String(&elapsed))],
            )
        } else if turn.step_count == 1 {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(1)),
                ],
            )
        } else {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(turn.step_count as i64)),
                ],
            )
        }
    });
    let copy_text = turn
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text(text) if !text.is_empty() => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    rsx! {
        div {
            key: "{key}",
            class: "flex max-w-[92%] flex-col gap-2 self-start",
            style: "content-visibility:auto;contain-intrinsic-size:auto 180px;",
            if !blocks.is_empty() {
                AssistantTurn {
                    standalone: false,
                    class: "chat-assistant-turn relative flex flex-col gap-2.5 overflow-hidden rounded-2xl border py-3 pl-3.5 pr-10",
                    if !copy_text.is_empty() {
                        MessageCopyButton { text: copy_text.clone() }
                    }
                    for (j , block , children) in blocks {
                        TurnBlock {
                            block_index: j,
                            block: block.clone(),
                            nested: children
                                .iter()
                                .map(|(index, child)| (*index, (*child).clone()))
                                .collect::<Vec<_>>(),
                            latest_thinking: should_expand_thinking(j, block_count),
                            latest_tool: latest_tool_index == Some(j),
                        }
                    }
                }
            }
            if turn.running && !reconnecting {
                WorkingIndicator {}
            } else if let Some(label) = duration_label {
                div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground/70",
                    span { class: "h-1.5 w-1.5 rounded-full bg-[color:var(--agent-accent)]" }
                    span { class: "tabular-nums", "{label}" }
                }
            }
        }
    }
}

/// The twisty on a collapsible activity row.
#[component]
fn DisclosureIcon() -> Element {
    rsx! {
        span {
            class: "disclosure-icon relative inline-block h-3 w-3 shrink-0 text-muted-foreground",
            aria_hidden: "true",
        }
    }
}

#[component]
pub fn WorkingIndicator() -> Element {
    let mut elapsed = use_signal(|| 0u32);
    let mut verb = use_signal(|| translate("agent-working-working"));
    use_future(move || async move {
        loop {
            sleep_ms(1000).await;
            elapsed.set(elapsed() + 1);
        }
    });
    use_future(move || async move {
        loop {
            sleep_ms(2500).await;
            verb.set(translate(
                WORKING_VERB_IDS[random_index(WORKING_VERB_IDS.len())],
            ));
        }
    });
    let verb_text = verb();
    let elapsed_text = fmt_elapsed(elapsed());
    rsx! {
        div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground",
            span { class: "agent-working-label font-medium", "{verb_text}" }
            span { class: "flex items-end gap-0.5 text-[color:var(--agent-accent)]",
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:120ms]" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:240ms]" }
            }
            span { class: "tabular-nums text-xs", "{elapsed_text}" }
        }
    }
}

fn normalized_tool_args(args: &str) -> Option<serde_json::Value> {
    let mut value = serde_json::from_str::<serde_json::Value>(args).ok()?;
    while let serde_json::Value::Object(map) = &value {
        let Some(arguments) = map.get("arguments") else {
            break;
        };
        if map.contains_key("server") || map.contains_key("tool") || map.contains_key("name") {
            value = arguments.clone();
        } else {
            break;
        }
    }
    Some(value)
}

fn tool_arg_label(key: &str) -> String {
    let mut label = key.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn tool_arg_is_path(key: &str, value: &str) -> bool {
    matches!(
        key,
        "path" | "file" | "file_path" | "cwd" | "dir" | "directory" | "workdir"
    ) || value.starts_with('/')
}

/// One argument of a tool call, recursing through nested objects and arrays.
#[component]
fn ToolArg(name: String, value: serde_json::Value) -> Element {
    let key = name;
    let label = tool_arg_label(&key);
    let row_class = "relative flex min-w-0 items-center gap-3 py-1.5 pl-1 before:absolute before:-left-3 before:top-1/2 before:h-px before:w-2 before:bg-foreground/20";
    let label_class =
        "shrink-0 text-[10px] font-medium uppercase tracking-[0.1em] text-muted-foreground/80";
    match value {
        serde_json::Value::String(text) if tool_arg_is_path(&key, &text) => rsx! {
            div { class: "{row_class}",
                {rsx! { TypeIcon { path: text.to_string(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-85" } }}
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        serde_json::Value::String(text)
            if matches!(
                key.as_str(),
                "cmd" | "command" | "script" | "patch" | "text" | "content"
            ) || text.contains('\n') =>
        {
            rsx! {
                div { class: "relative py-1.5 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                    if !key.is_empty() {
                        div { class: "mb-1.5 flex items-center gap-1.5 {label_class}",
                            span { class: "h-1.5 w-1.5 rounded-full bg-emerald-400/70" }
                            "{label}"
                        }
                    }
                    pre { class: "max-h-56 overflow-auto whitespace-pre-wrap break-words border-l border-foreground/20 py-1 pl-3 font-mono text-[11px] leading-relaxed text-foreground/80", "{text}" }
                }
            }
        }
        serde_json::Value::String(text) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        serde_json::Value::Bool(value) => {
            let tone = if value {
                "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
            } else {
                "bg-foreground/[0.04] text-muted-foreground ring-foreground/10"
            };
            rsx! {
                div { class: "{row_class}",
                    if !key.is_empty() {
                        span { class: "{label_class}", "{label}" }
                    }
                    span { class: "rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ring-inset {tone}", "{value}" }
                }
            }
        }
        serde_json::Value::Number(value) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "ml-auto font-mono text-[11px] tabular-nums text-cyan-600 dark:text-cyan-300", "{value}" }
            }
        },
        serde_json::Value::Array(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for (index , value) in values.into_iter().enumerate() {
                        ToolArg { name: format!("{}", index + 1), value }
                    }
                }
            }
        },
        serde_json::Value::Object(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for (child_key , child_value) in values {
                        ToolArg { name: child_key, value: child_value }
                    }
                }
            }
        },
        serde_json::Value::Null => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                span { class: "ml-auto text-[10px] italic text-muted-foreground/70", "None" }
            }
        },
    }
}

/// The argument list of a tool call, parsed out of its JSON.
#[component]
fn ToolArgs(args: String) -> Element {
    let args = args.as_str();
    let Some(value) = normalized_tool_args(args) else {
        return rsx! {
            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{args}" }
        };
    };
    match value {
        serde_json::Value::Object(map) if map.is_empty() => rsx! {},
        serde_json::Value::Object(map) => rsx! {
            div { class: "ml-1 mt-2 flex flex-col border-l border-foreground/20 pl-3", aria_label: "Tool arguments",
                for (key , value) in map {
                    ToolArg { name: key, value: value }
                }
            }
        },
        value => rsx! {
            div { class: "ml-1 mt-2 border-l border-foreground/20 pl-3", ToolArg { name: String::new(), value: value } }
        },
    }
}

/// One block of an assistant turn: prose, thinking, a tool call and what it produced.
#[component]
pub fn TurnBlock(
    block_index: usize,
    block: ChatBlock,
    nested: Vec<(usize, ChatBlock)>,
    latest_thinking: bool,
    latest_tool: bool,
) -> Element {
    let key = block_index;
    let block = &block;
    let children: Vec<(usize, &ChatBlock)> = nested
        .iter()
        .map(|(index, child)| (*index, child))
        .collect();
    let children = children.as_slice();
    match block {
        ChatBlock::Text(text) => rsx! {
            div {
                key: "{key}",
                class: "chat-md px-0.5 text-sm leading-relaxed text-foreground/95",
                dangerous_inner_html: md_to_html(text),
            }
        },
        ChatBlock::Thinking(text) => rsx! {
            div { key: "{key}", class: "agent-row-hover grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors",
                ActivityIconView { kind: ActivityIcon::Thinking }
                details { open: latest_thinking, class: "disclosure min-w-0 text-sm text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", {translate("agent-thinking")} }
                        DisclosureIcon {}
                    }
                    div { class: "mt-2 whitespace-pre-wrap border-l border-foreground/15 pl-3 text-xs leading-relaxed", "{text}" }
                }
            }
        },
        ChatBlock::ToolUse { name, args, .. } => {
            let (icon, label) = tool_presentation(name, args);
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-foreground/[0.025]",
                    ToolActivityIcon { name: name.clone(), args: args.clone(), fallback: icon }
                    div { class: "min-w-0",
                        details { open: latest_tool, class: "disclosure text-sm text-muted-foreground",
                            summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                span { class: "font-medium", "{label}" }
                                DisclosureIcon {}
                            }
                            div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                            if !args.is_empty() && args != "{}" {
                                ToolArgs { args: args.to_string() }
                            }
                        }
                        if !children.is_empty() {
                            div { class: "agent-context-tree ml-0.5 mt-1.5 flex flex-col gap-1 border-l pl-3",
                                for (child_key , child) in children {
                                    ToolChild { child_key: *child_key, block: (*child).clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Subagent(subagent) => {
            let status_label = subagent_status_label(&subagent.status);
            let status_class = subagent_status_class(&subagent.status);
            let title = if subagent.title.is_empty() {
                translate("agent-subagent")
            } else {
                subagent.title.replace('_', " ")
            };
            let action = subagent.action.replace('_', " ");
            let child_threads = subagent.child_thread_ids.join(", ");
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl bg-violet-500/[0.025] px-2 py-1.5 ring-1 ring-inset ring-violet-500/10 transition-colors hover:bg-violet-500/[0.05]",
                    ActivityIconView { kind: ActivityIcon::Subagent }
                    div { class: "min-w-0",
                        details { open: subagent.status == "in_progress", class: "disclosure text-sm text-muted-foreground",
                            summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                span { class: "font-medium text-foreground/85", "{title}" }
                                span { class: "rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                                DisclosureIcon {}
                            }
                            div { class: "mt-2 flex flex-wrap gap-1.5 text-[10px]",
                                span { class: "rounded-full bg-violet-500/10 px-2 py-0.5 font-semibold text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                                if !subagent.action.is_empty() {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{action}" }
                                }
                                if let Some(agent_name) = &subagent.agent_name {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{agent_name}" }
                                }
                                if let Some(model) = &subagent.model {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 font-mono text-foreground/60", "{model}" }
                                }
                                if let Some(effort) = &subagent.reasoning_effort {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{effort}" }
                                }
                            }
                            if let Some(prompt) = &subagent.prompt {
                                div { class: "mt-2 rounded-lg bg-foreground/[0.025] p-2 text-xs leading-relaxed text-foreground/75 ring-1 ring-inset ring-foreground/10",
                                    div { class: "mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground/70", {translate("agent-prompt")} }
                                    div { class: "whitespace-pre-wrap", "{prompt}" }
                                }
                            }
                            div { class: "mt-2 grid gap-1 text-[10px] text-muted-foreground/75",
                                if let Some(thread_id) = &subagent.thread_id {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-thread"))} } code { class: "font-mono", "{thread_id}" } }
                                }
                                if let Some(parent_thread_id) = &subagent.parent_thread_id {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-parent"))} } code { class: "font-mono", "{parent_thread_id}" } }
                                }
                                if !child_threads.is_empty() {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-children"))} } code { class: "break-all font-mono", "{child_threads}" } }
                                }
                                div { span { class: "font-semibold", {format!("{} ", translate("agent-call"))} } code { class: "font-mono", "{subagent.call_id}" } }
                            }
                            if !subagent.raw_input.is_empty() && subagent.raw_input != "{}" {
                                details { class: "disclosure mt-2 text-[11px] text-muted-foreground",
                                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                        span { class: "font-medium", {translate("agent-raw-event")} }
                                        DisclosureIcon {}
                                    }
                                    pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground", "{subagent.raw_input}" }
                                }
                            }
                        }
                        if !children.is_empty() {
                            div { class: "agent-context-tree ml-0.5 mt-2 flex flex-col gap-1 border-l border-violet-500/25 pl-3",
                                for (child_key , child) in children {
                                    ToolChild { child_key: *child_key, block: (*child).clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Plan { steps } => {
            let n = steps.len();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-indigo-500/[0.035]",
                    ActivityIconView { kind: ActivityIcon::Plan }
                    details { open: true, class: "disclosure min-w-0 text-sm",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium text-foreground/80", {translate("agent-plan")} }
                            span {
                                class: "text-xs text-muted-foreground",
                                {translate_with(
                                    "agent-tasks",
                                    &[("count", TranslationValue::Number(n as i64))],
                                )}
                            }
                            DisclosureIcon {}
                        }
                        ul { class: "mt-2 flex flex-col gap-1.5 border-l border-indigo-500/20 pl-3",
                            for (i , step) in steps.iter().enumerate() {
                                li { key: "{i}", class: "flex items-start gap-2 text-xs",
                                    span { class: "mt-px {plan_glyph_class(&step.status)}", "{plan_glyph(&step.status)}" }
                                    span { class: plan_text_class(&step.status), "{step.content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Diff {
            path,
            old_text,
            new_text,
            ..
        } => {
            let old = old_text.as_deref().unwrap_or("");
            let lines: Vec<(String, &'static str)> =
                similar::TextDiff::from_lines(old, new_text.as_str())
                    .iter_all_changes()
                    .filter_map(|c| match c.tag() {
                        similar::ChangeTag::Delete => Some((
                            format!("- {}", c.value().trim_end_matches('\n')),
                            "px-3 bg-red-500/10 text-red-300",
                        )),
                        similar::ChangeTag::Insert => Some((
                            format!("+ {}", c.value().trim_end_matches('\n')),
                            "px-3 bg-emerald-500/10 text-emerald-300",
                        )),
                        similar::ChangeTag::Equal => None,
                    })
                    .collect();
            let fname = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-green-500/[0.035]",
                    FileActivityIcon { path: path.clone(), write: true }
                    details { class: "disclosure min-w-0 text-sm text-muted-foreground",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium", {format!("{} ", translate("agent-edited"))} }
                            code { class: "truncate font-mono text-xs text-foreground/70", "{fname}" }
                            DisclosureIcon {}
                        }
                        div { class: "mt-2 overflow-hidden rounded-lg ring-1 ring-inset ring-foreground/10",
                            div { class: "overflow-x-auto bg-foreground/[0.02] py-1 font-mono text-[11px] leading-relaxed",
                                for (i , (line , cls)) in lines.iter().enumerate() {
                                    div { key: "{i}", class: "{cls}", "{line}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::ToolResult {
            content, is_error, ..
        } => rsx! {
            StandaloneToolResult { result_key: key, content: content.clone(), is_error: *is_error }
        },
        ChatBlock::Reconnect { attempt, total } => rsx! {
            div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-2.5 rounded-xl px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-amber-500/[0.035]",
                ActivityIconView { kind: ActivityIcon::Reconnect }
                span {
                    class: "font-medium tabular-nums",
                    {translate_with(
                        "agent-reconnecting",
                        &[
                            ("attempt", TranslationValue::Number(*attempt as i64)),
                            ("total", TranslationValue::Number(*total as i64)),
                        ],
                    )}
                }
            }
        },
    }
}

/// A block nested under a tool call, which may itself hold more.
#[component]
fn ToolChild(child_key: usize, block: ChatBlock) -> Element {
    let key = child_key;
    let block = &block;
    match block {
        ChatBlock::ToolUse { name, args, .. } => {
            let (_, label) = tool_presentation(name, args);
            rsx! {
                details { key: "{key}", class: "disclosure text-xs text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{label}" }
                        DisclosureIcon {}
                    }
                    div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                    if !args.is_empty() && args != "{}" {
                        ToolArgs { args: args.to_string() }
                    }
                }
            }
        }
        ChatBlock::Subagent(subagent) => {
            let status_label = subagent_status_label(&subagent.status);
            let status_class = subagent_status_class(&subagent.status);
            rsx! {
                details { key: "{key}", class: "disclosure text-xs text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{subagent.title}" }
                        span { class: "rounded-full px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                        DisclosureIcon {}
                    }
                    div { class: "mt-1 flex flex-wrap gap-1 text-[10px]",
                        span { class: "rounded-full bg-violet-500/10 px-1.5 py-0.5 text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                        if let Some(agent_name) = &subagent.agent_name {
                            span { class: "rounded-full bg-foreground/[0.055] px-1.5 py-0.5", "{agent_name}" }
                        }
                    }
                    if let Some(prompt) = &subagent.prompt {
                        div { class: "mt-1.5 whitespace-pre-wrap rounded-lg bg-foreground/[0.025] p-2 text-[11px] leading-relaxed ring-1 ring-inset ring-foreground/10", "{prompt}" }
                    }
                }
            }
        }
        ChatBlock::ToolResult {
            content, is_error, ..
        } => rsx! {
            NestedToolResult { result_key: key, content: content.clone(), is_error: *is_error }
        },
        _ => rsx! {},
    }
}

fn subagent_status_label(status: &str) -> String {
    match status {
        "in_progress" => translate("agent-status-running"),
        "completed" => translate("agent-status-done"),
        "failed" => translate("agent-status-failed"),
        _ => translate("agent-status-pending"),
    }
}

fn subagent_status_class(status: &str) -> &'static str {
    match status {
        "in_progress" => "bg-violet-500/10 text-violet-700 dark:text-violet-300",
        "completed" => "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        "failed" => "bg-red-500/10 text-red-700 dark:text-red-300",
        _ => "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    }
}

/// A tool result folded under the call that produced it.
#[component]
fn NestedToolResult(result_key: usize, content: String, is_error: bool) -> Element {
    let key = result_key;
    let content = content.as_str();
    let tone = if is_error {
        "text-red-600 dark:text-red-300"
    } else {
        "text-teal-700/80 dark:text-teal-300/80"
    };
    let panel = if is_error {
        "bg-red-500/[0.045] ring-red-500/15"
    } else {
        "bg-teal-500/[0.035] ring-teal-500/10"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    rsx! {
        details { key: "{key}", class: "disclosure text-xs {tone}",
            summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{label}" }
                DisclosureIcon {}
            }
            pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset {panel}", "{content}" }
        }
    }
}

/// A tool result with no matching call in view.
#[component]
fn StandaloneToolResult(result_key: usize, content: String, is_error: bool) -> Element {
    let key = result_key;
    let content = content.as_str();
    let tone = if is_error {
        "text-red-600 dark:text-red-300"
    } else {
        "text-teal-700/80 dark:text-teal-300/80"
    };
    let panel = if is_error {
        "bg-red-500/[0.045] ring-red-500/15"
    } else {
        "bg-teal-500/[0.035] ring-teal-500/10"
    };
    let row = if is_error {
        "hover:bg-red-500/[0.035]"
    } else {
        "hover:bg-teal-500/[0.035]"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    let icon = if is_error {
        ActivityIcon::Error
    } else {
        ActivityIcon::Output
    };
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors {row}",
            ActivityIconView { kind: icon }
            details { class: "disclosure min-w-0 text-sm {tone}",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "{label}" }
                    DisclosureIcon {}
                }
                pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset {panel}", "{content}" }
            }
        }
    }
}

pub fn fmt_elapsed(secs: u32) -> String {
    if secs >= 60 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn plan_glyph(status: &str) -> &'static str {
    match status {
        "completed" => "✓",
        "in_progress" => "◐",
        _ => "○",
    }
}

fn plan_glyph_class(status: &str) -> &'static str {
    match status {
        "completed" => "text-emerald-500",
        "in_progress" => "text-amber-500",
        _ => "text-muted-foreground",
    }
}

fn plan_text_class(status: &str) -> &'static str {
    match status {
        "completed" => "text-muted-foreground line-through",
        "in_progress" => "text-foreground",
        _ => "text-muted-foreground",
    }
}

/// Render assistant markdown to HTML, dropping any raw HTML the agent emits (markdown only —
/// never inject arbitrary markup into the page).
fn md_to_html(src: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(src, opts)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// Scoped styling for markdown rendered via `dangerous_inner_html` (Tailwind can't see generated
/// HTML, and its preflight strips heading/list defaults). Theme-neutral rgba so it works in both
/// light and dark.
pub const MD_CSS: &str = r#"
.agent-chat-prompt-shell::before{content:"";position:absolute;inset:-28px -42px;z-index:-1;border-radius:2.5rem;background:radial-gradient(60% 90% at 50% 75%,rgba(255,255,255,0.1),transparent 72%);pointer-events:none}
.agent-chat-page{background-image:radial-gradient(80% 55% at 15% 0%,color-mix(in srgb,var(--agent-accent) 9%,transparent),transparent 65%),radial-gradient(75% 55% at 90% 10%,color-mix(in srgb,var(--agent-accent) 7%,transparent),transparent 62%),radial-gradient(65% 45% at 55% 100%,color-mix(in srgb,var(--agent-accent) 5%,transparent),transparent 70%)}
.agent-chat-header{border-color:color-mix(in srgb,var(--agent-accent) 12%,transparent)}
.chat-user-bubble,.chat-assistant-turn{content-visibility:auto;contain-intrinsic-size:auto 160px;contain:layout paint style;transition:border-color 180ms ease,box-shadow 180ms ease,transform 180ms ease}
.chat-user-bubble{border-color:color-mix(in srgb,var(--agent-accent) 18%,transparent);background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 19%,transparent),color-mix(in srgb,var(--agent-accent) 9%,transparent) 58%,color-mix(in srgb,var(--agent-accent) 4%,transparent));box-shadow:0 10px 32px color-mix(in srgb,var(--agent-accent) 9%,transparent)}
.chat-user-bubble:hover{border-color:color-mix(in srgb,var(--agent-accent) 30%,transparent);box-shadow:0 14px 38px color-mix(in srgb,var(--agent-accent) 14%,transparent);transform:translateY(-1px)}
.chat-assistant-turn{border-color:color-mix(in srgb,var(--agent-accent) 9%,rgba(127,127,127,0.08));background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 5%,transparent),rgba(127,127,127,0.025) 55%,transparent);box-shadow:0 10px 35px rgba(0,0,0,0.035)}
.chat-assistant-turn::before{content:"";position:absolute;inset:0 auto 0 0;width:2px;background:linear-gradient(180deg,color-mix(in srgb,var(--agent-accent) 82%,transparent),color-mix(in srgb,var(--agent-accent) 52%,transparent),color-mix(in srgb,var(--agent-accent) 28%,transparent));opacity:0.75}
.chat-assistant-turn:hover{border-color:color-mix(in srgb,var(--agent-accent) 17%,transparent);box-shadow:0 14px 40px color-mix(in srgb,var(--agent-accent) 5%,rgba(0,0,0,0.055))}
.chat-assistant-turn .disclosure>summary{transition:color 160ms ease}
.chat-assistant-turn .disclosure>summary:hover{color:color-mix(in srgb,currentColor 68%,var(--agent-accent))}
.agent-themed-activity{color:var(--agent-accent);background:color-mix(in srgb,var(--agent-accent) 11%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 18%,transparent)}
.python-activity-icon{background:linear-gradient(145deg,rgba(55,118,171,0.15),rgba(255,212,59,0.11));color:#3776ab;box-shadow:inset 0 0 0 1px rgba(55,118,171,0.3)}
.agent-working-label{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor)}
.agent-row-hover:hover{background:color-mix(in srgb,var(--agent-accent) 4%,transparent)}
.agent-code-panel,.user-context-content{background:color-mix(in srgb,var(--agent-accent) 4%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 11%,transparent)}
.agent-context-tree{border-color:color-mix(in srgb,var(--agent-accent) 22%,transparent)}
.agent-turn-meta{color:color-mix(in srgb,var(--agent-accent) 72%,currentColor);border-color:color-mix(in srgb,var(--agent-accent) 13%,transparent);background:color-mix(in srgb,var(--agent-accent) 7%,transparent)}
.agent-turn-meta-dot{background:var(--agent-accent)}
.user-context-panel{border-color:color-mix(in srgb,var(--agent-accent) 14%,transparent);background:color-mix(in srgb,var(--agent-accent) 5%,rgba(127,127,127,0.025))}
.user-context-panel>summary:hover{color:color-mix(in srgb,currentColor 65%,var(--agent-accent))}
.chat-md{line-height:1.6;word-break:break-word}
.chat-md>*:first-child{margin-top:0}
.chat-md>*:last-child{margin-bottom:0}
.chat-md h1,.chat-md h2,.chat-md h3,.chat-md h4{font-weight:600;line-height:1.3;margin:0.9em 0 0.35em}
.chat-md h1{font-size:1.35em}
.chat-md h2{font-size:1.2em}
.chat-md h3{font-size:1.05em}
.chat-md h4{font-size:1em}
.chat-md p{margin:0.5em 0}
.chat-md ul,.chat-md ol{margin:0.4em 0;padding-left:1.4em}
.chat-md ul{list-style:disc}
.chat-md ol{list-style:decimal}
.chat-md li{margin:0.15em 0}
.chat-md li>ul,.chat-md li>ol{margin:0.15em 0}
.chat-md strong{font-weight:600}
.chat-md em{font-style:italic}
.chat-md a{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor);text-decoration-color:color-mix(in srgb,var(--agent-accent) 45%,transparent);text-underline-offset:0.16em}
.chat-md code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.88em;background:color-mix(in srgb,var(--agent-accent) 10%,transparent);border:1px solid color-mix(in srgb,var(--agent-accent) 11%,transparent);padding:0.1em 0.35em;border-radius:0.4em}
.chat-md pre{background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 7%,transparent),color-mix(in srgb,var(--agent-accent) 3%,transparent));border:1px solid color-mix(in srgb,var(--agent-accent) 11%,transparent);padding:0.7em 0.9em;border-radius:0.7em;overflow-x:auto;margin:0.6em 0}
.chat-md pre code{background:none;border:0;padding:0;font-size:0.85em}
.chat-md blockquote{border-left:2px solid color-mix(in srgb,var(--agent-accent) 48%,transparent);padding-left:0.8em;margin:0.5em 0;opacity:0.85}
.chat-md hr{border:0;border-top:1px solid rgba(127,127,127,0.25);margin:0.9em 0}
.chat-md table{border-collapse:collapse;margin:0.5em 0;font-size:0.95em}
.chat-md th,.chat-md td{border:1px solid rgba(127,127,127,0.3);padding:0.3em 0.6em;text-align:left}
@media (prefers-reduced-motion:reduce){.agent-chat-caret{animation:none}.chat-user-bubble,.chat-assistant-turn{transition:none}.chat-user-bubble:hover{transform:none}}
"#;
