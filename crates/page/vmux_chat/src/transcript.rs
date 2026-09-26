use dioxus::prelude::*;
use vmux_api::chat::{
    ChatDiffLineKind, ChatItem, ChatPlanStatus, ChatSubagentState, ChatSubagentStatus,
    ChatSubagentSummary, ChatToolArgument, ChatToolArgumentValue, ChatToolArguments, ChatToolCall,
    ChatToolChild, ChatToolChildCall, ChatTurn, ChatTurnRow, WORKING_VERB_IDS,
};
use vmux_api::prompt_media::ChatAttachment;
use vmux_ui::components::avatar::Avatar;
use vmux_ui::file_icon::{FilePath, TypeIcon};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::activity::{
    ActivityIcon, ActivityIconView, FileActivityIcon, ToolActivityIcon, ToolPresentation,
};
use vmux_ui::clipboard::Clipboard;
use vmux_ui::platform::{random_index, sleep_ms};

#[component]
pub fn UserBubble(
    avatar_name: String,
    avatar_color: String,
    #[props(default)] copy_text: String,
    #[props(default)] created_at_ms: u64,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let you = translate("team-you");
    rsx! {
        div { class: "chat-user-bubble group flex w-full flex-row-reverse items-start justify-start gap-3 px-1 py-1 text-sm [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", ..attributes,
            Avatar {
                src: None,
                seed: avatar_name.clone(),
                background: avatar_color,
                alt: avatar_name.clone(),
                class: "mt-0.5 h-8 w-8 text-[10px]".to_string(),
            }
            div { class: "relative min-w-0 max-w-[80%] flex-none pl-8",
                div { class: "mb-1 text-right text-xs font-semibold text-foreground", "{you}" }
                div { class: "flex min-w-0 flex-col items-end gap-2 text-left", {children} }
                MessageMeta { text: copy_text, created_at_ms, right: true }
            }
        }
    }
}

#[component]
pub fn AssistantTurn(
    name: String,
    avatar_src: Option<String>,
    avatar_background: String,
    #[props(default)] copy_text: String,
    #[props(default)] created_at_ms: u64,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "chat-assistant-turn group flex w-full gap-3 px-1 py-1 [contain-intrinsic-size:auto_160px] [contain:layout_paint_style] [content-visibility:auto]", ..attributes,
            Avatar {
                src: avatar_src,
                seed: name.clone(),
                background: avatar_background,
                alt: name.clone(),
                class: "mt-0.5 h-8 w-8 text-[10px]".to_string(),
            }
            div { class: "relative min-w-0 flex-1 pr-8",
                div { class: "mb-1 text-xs font-semibold text-foreground", "{name}" }
                div { class: "flex min-w-0 flex-col gap-2.5", {children} }
                MessageMeta { text: copy_text, created_at_ms }
            }
        }
    }
}

#[component]
fn MessageMeta(text: String, created_at_ms: u64, #[props(default)] right: bool) -> Element {
    let alignment = if right {
        "justify-end"
    } else {
        "justify-start"
    };
    let timestamp = message_timestamp(created_at_ms);
    rsx! {
        div { class: "mt-1 flex h-6 items-center gap-1 {alignment} text-[11px] text-muted-foreground/45",
            if let Some(timestamp) = timestamp {
                span { class: "pointer-events-none tabular-nums opacity-0 transition-opacity group-hover:opacity-100", "{timestamp}" }
            }
            if !text.is_empty() {
                MessageCopyButton { text }
            }
        }
    }
}

fn message_timestamp(created_at_ms: u64) -> Option<String> {
    if created_at_ms == 0 {
        return None;
    }
    let timestamp = i64::try_from(created_at_ms).ok()?;
    let utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp)?;
    Some(
        utc.with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string(),
    )
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;

    #[test]
    fn message_timestamp_is_local_clock_time() {
        let timestamp = message_timestamp(1_788_979_740_000).expect("timestamp");

        assert_eq!(timestamp.len(), 5);
        assert_eq!(timestamp.as_bytes()[2], b':');
        assert!(!timestamp.contains('-'));
    }

    #[test]
    fn missing_message_timestamp_stays_hidden() {
        assert_eq!(message_timestamp(0), None);
    }
}

#[component]
pub fn MessageCopyButton(text: String) -> Element {
    let label = translate("agent-copy");
    rsx! {
        button {
            class: "pointer-events-none flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 opacity-0 transition group-hover:pointer-events-auto group-hover:opacity-100 hover:bg-foreground/[0.08] hover:text-foreground focus-visible:pointer-events-auto focus-visible:opacity-100",
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                Clipboard::write(&text);
            },
            LineIconView { icon: LineIcon::Copy, class: "h-3.5 w-3.5" }
        }
    }
}

#[component]
pub fn ChatItemRow(
    absolute_index: usize,
    item: ChatItem,
    agent_name: String,
    agent_avatar: Option<String>,
    agent_color: String,
    user_name: String,
    user_color: String,
) -> Element {
    let key = absolute_index;
    let item = &item;
    match item {
        ChatItem::User {
            text,
            context,
            attachments,
            created_at_ms,
        } => rsx! {
            UserBubble {
                key: "{key}",
                avatar_name: user_name,
                avatar_color: user_color,
                copy_text: text.clone(),
                created_at_ms: *created_at_ms,
                if let Some(context) = context {
                    details { class: "disclosure user-context-panel rounded-xl border",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 px-2.5 py-2 text-xs list-none [&::-webkit-details-marker]:hidden",
                            span { class: "agent-themed-activity flex h-5 w-5 shrink-0 items-center justify-center rounded-md",
                                LineIconView { icon: LineIcon::Shield, class: "h-3 w-3" }
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
                    div { class: "flex w-full flex-col gap-2",
                        for attachment in attachments {
                            UserAttachment {
                                attachment: attachment.clone(),
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
                agent_name,
                agent_avatar,
                agent_color,
            }
        },
    }
}

#[component]
fn UserAttachment(attachment: ChatAttachment) -> Element {
    let preview_data_url = &attachment.preview_data_url;
    if attachment.mime_type.starts_with("image/") && !preview_data_url.is_empty() {
        return rsx! {
            figure {
                key: "message-attachment-{attachment.path}",
                class: "w-full overflow-hidden rounded-xl",
                title: "{attachment.name}",
                img {
                    src: "{preview_data_url}",
                    alt: "{attachment.name}",
                    loading: "lazy",
                    decoding: "async",
                    class: "max-h-80 w-full object-cover",
                }
            }
        };
    }
    rsx! {
        div {
            key: "message-attachment-{attachment.path}",
            class: "flex min-w-32 max-w-64 items-center gap-2 rounded-xl bg-foreground/[0.06] px-3 py-2 ring-1 ring-inset ring-foreground/10",
            span { class: "font-mono text-[10px] font-semibold tracking-wide text-muted-foreground", "{FilePath(&attachment.name).extension_label()}" }
            span { class: "truncate text-xs text-muted-foreground", "{attachment.name}" }
        }
    }
}

#[component]
pub fn TurnView(
    turn_index: usize,
    turn: ChatTurn,
    agent_name: String,
    agent_avatar: Option<String>,
    agent_color: String,
) -> Element {
    let key = turn_index;
    let turn = &turn;
    let reconnecting = matches!(turn.rows.last(), Some(ChatTurnRow::Reconnect { .. }));
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
    rsx! {
        div {
            key: "{key}",
            class: "flex w-full flex-col gap-2 [contain-intrinsic-size:auto_180px] [content-visibility:auto]",
            if !turn.rows.is_empty() || turn.running || duration_label.is_some() {
                AssistantTurn {
                    name: agent_name,
                    avatar_src: agent_avatar,
                    avatar_background: agent_color,
                    copy_text: turn.copy_text.clone(),
                    created_at_ms: turn.created_at_ms,
                    for row in turn.rows.iter().cloned() {
                        TurnBlock { key: "{row.index()}", row }
                    }
                    if turn.running && !reconnecting {
                        WorkingIndicator {}
                    } else if let Some(label) = duration_label {
                        div { class: "flex items-center gap-2 text-sm text-muted-foreground/70",
                            span { class: "h-1.5 w-1.5 rounded-full bg-[color:var(--agent-accent)]" }
                            span { class: "tabular-nums", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

const ROW_SUMMARY: &str = "flex min-h-6 cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden";

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
            span { class: "animate-pulse font-medium motion-reduce:animate-none", "{verb_text}" }
            span { class: "flex items-end gap-0.5 text-[color:var(--agent-accent)]",
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current motion-reduce:animate-none" }
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:120ms] motion-reduce:animate-none" }
                span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:240ms] motion-reduce:animate-none" }
            }
            span { class: "tabular-nums text-xs", "{elapsed_text}" }
        }
    }
}

#[component]
fn ToolArg(argument: ChatToolArgument) -> Element {
    let key = argument.name;
    let label = argument.label;
    let row_class = "relative flex min-w-0 items-center gap-3 py-1.5 pl-1 before:absolute before:-left-3 before:top-1/2 before:h-px before:w-2 before:bg-foreground/20";
    let label_class =
        "shrink-0 text-[10px] font-medium uppercase tracking-[0.1em] text-muted-foreground/80";
    match argument.value {
        ChatToolArgumentValue::Path(text) => rsx! {
            div { class: "{row_class}",
                {rsx! { TypeIcon { path: text.to_string(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-85" } }}
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        ChatToolArgumentValue::Code(text) => rsx! {
            div { class: "relative py-1.5 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1.5 flex items-center gap-1.5 {label_class}",
                        span { class: "h-1.5 w-1.5 rounded-full bg-success/70" }
                        "{label}"
                    }
                }
                pre { class: "max-h-56 overflow-auto whitespace-pre-wrap break-words border-l border-foreground/20 py-1 pl-3 font-mono text-[11px] leading-relaxed text-foreground/80", "{text}" }
            }
        },
        ChatToolArgumentValue::Text(text) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        ChatToolArgumentValue::Bool(value) => {
            let tone = if value {
                "bg-success/10 text-success ring-success/20"
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
        ChatToolArgumentValue::Number(value) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "ml-auto font-mono text-[11px] tabular-nums text-cyan-600 dark:text-cyan-300", "{value}" }
            }
        },
        ChatToolArgumentValue::List(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for value in values {
                        ToolArg { argument: value }
                    }
                }
            }
        },
        ChatToolArgumentValue::Object(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for value in values {
                        ToolArg { argument: value }
                    }
                }
            }
        },
        ChatToolArgumentValue::Null => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                span { class: "ml-auto text-[10px] italic text-muted-foreground/70", "None" }
            }
        },
    }
}

#[component]
fn ToolArgs(arguments: ChatToolArguments) -> Element {
    match arguments {
        ChatToolArguments::None => rsx! {},
        ChatToolArguments::Fields(fields) => rsx! {
            div { class: "ml-1 mt-2 flex flex-col border-l border-foreground/20 pl-3", aria_label: "Tool arguments",
                for argument in fields {
                    ToolArg { argument }
                }
            }
        },
        ChatToolArguments::Value(value) => rsx! {
            div { class: "ml-1 mt-2 border-l border-foreground/20 pl-3",
                ToolArg { argument: ChatToolArgument { name: String::new(), label: String::new(), value } }
            }
        },
        ChatToolArguments::Raw(raw) => rsx! {
            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{raw}" }
        },
    }
}

#[component]
fn FinishedToolCalls(calls: Vec<ChatToolCall>) -> Element {
    let count = calls.len() as i64;
    rsx! {
        details { class: "disclosure",
            summary { class: "flex cursor-pointer select-none items-center gap-2 rounded-xl px-2 py-1 text-sm text-muted-foreground list-none transition-colors hover:bg-foreground/[0.025] [&::-webkit-details-marker]:hidden",
                span { class: "font-medium",
                    {translate_with("agent-tool-calls", &[("count", TranslationValue::Number(count))])}
                }
                DisclosureIcon {}
            }
            div { class: "mt-1 flex flex-col gap-1",
                for call in calls {
                    ToolCall { call }
                }
            }
        }
    }
}

#[component]
fn ToolCall(call: ChatToolCall) -> Element {
    let key = call.index;
    let label = ToolPresentation::for_call(call.kind, &call.fallback_label).label;
    let children = call.children.clone();
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-foreground/[0.025]",
            ToolActivityIcon {
                name: call.name.clone(),
                file_path: call.file_path.clone(),
                activity: call.activity,
            }
            div { class: "min-w-0",
                details { open: call.live, class: "disclosure text-sm text-muted-foreground",
                    summary { class: ROW_SUMMARY,
                        span { class: "font-medium", "{label}" }
                        DisclosureIcon {}
                    }
                    div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{call.name}" }
                    ToolArgs { arguments: call.arguments.clone() }
                }
                if !children.is_empty() {
                    div { class: "agent-context-tree ml-0.5 mt-1.5 flex flex-col gap-1 border-l pl-3",
                        for child in children {
                            ToolChild { key: "{child.index()}", child }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SubagentRow(subagent: ChatSubagentState) -> Element {
    let key = subagent.index;
    let status_label = subagent_status_label(subagent.status);
    let status_class = subagent_status_class(subagent.status);
    let title = if subagent.title.is_empty() {
        translate("agent-subagent")
    } else {
        subagent.title.clone()
    };
    let children = subagent.children.clone();
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl bg-violet-500/[0.025] px-2 py-1.5 ring-1 ring-inset ring-violet-500/10 transition-colors hover:bg-violet-500/[0.05]",
            ActivityIconView { kind: ActivityIcon::Subagent }
            div { class: "min-w-0",
                details { open: subagent.status == ChatSubagentStatus::Running, class: "disclosure text-sm text-muted-foreground",
                    summary { class: "{ROW_SUMMARY} flex-wrap",
                        span { class: "font-medium text-foreground/85", "{title}" }
                        span { class: "rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                        DisclosureIcon {}
                    }
                    div { class: "mt-2 flex flex-wrap gap-1.5 text-[10px]",
                        span { class: "rounded-full bg-violet-500/10 px-2 py-0.5 font-semibold text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                        if !subagent.activity.is_empty() {
                            span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{subagent.activity}" }
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
                        if !subagent.child_threads.is_empty() {
                            div { span { class: "font-semibold", {format!("{} ", translate("agent-children"))} } code { class: "break-all font-mono", "{subagent.child_threads}" } }
                        }
                        div { span { class: "font-semibold", {format!("{} ", translate("agent-call"))} } code { class: "font-mono", "{subagent.call_id}" } }
                    }
                    if !subagent.raw_input.is_empty() && subagent.raw_input != "{}" {
                        details { class: "disclosure mt-2 text-[11px] text-muted-foreground",
                            summary { class: ROW_SUMMARY,
                                span { class: "font-medium", {translate("agent-raw-event")} }
                                DisclosureIcon {}
                            }
                            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground", "{subagent.raw_input}" }
                        }
                    }
                }
                if !children.is_empty() {
                    div { class: "agent-context-tree ml-0.5 mt-2 flex flex-col gap-1 border-l border-violet-500/25 pl-3",
                        for child in children {
                            ToolChild { key: "{child.index()}", child }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TurnBlock(row: ChatTurnRow) -> Element {
    let key = row.index();
    match row {
        ChatTurnRow::Text { text, .. } => rsx! {
            div {
                key: "{key}",
                class: CHAT_MD_CLASS,
                dangerous_inner_html: md_to_html(&text),
            }
        },
        ChatTurnRow::Thinking { text, latest, .. } => rsx! {
            div { key: "{key}", class: "agent-row-hover grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors",
                ActivityIconView { kind: ActivityIcon::Thinking }
                details { open: latest, class: "disclosure min-w-0 text-sm text-muted-foreground",
                    summary { class: ROW_SUMMARY,
                        span { class: "font-medium", {translate("agent-thinking")} }
                        DisclosureIcon {}
                    }
                    div { class: "mt-2 whitespace-pre-wrap border-l border-foreground/15 pl-3 text-xs leading-relaxed", "{text}" }
                }
            }
        },
        ChatTurnRow::Tool(call) => rsx! { ToolCall { call } },
        ChatTurnRow::FinishedTools { calls, .. } => rsx! { FinishedToolCalls { calls } },
        ChatTurnRow::Subagent(subagent) => rsx! { SubagentRow { subagent } },
        ChatTurnRow::Plan { steps, .. } => {
            let n = steps.len();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-indigo-500/[0.035]",
                    ActivityIconView { kind: ActivityIcon::Plan }
                    details { open: true, class: "disclosure min-w-0 text-sm",
                        summary { class: ROW_SUMMARY,
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
                                    span { class: "mt-px {plan_glyph_class(step.status)}", "{plan_glyph(step.status)}" }
                                    span { class: plan_text_class(step.status), "{step.content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatTurnRow::Diff(diff) => {
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-success/[0.035]",
                    FileActivityIcon { path: diff.path.clone(), write: true }
                    details { class: "disclosure min-w-0 text-sm text-muted-foreground",
                        summary { class: ROW_SUMMARY,
                            span { class: "font-medium", {format!("{} ", translate("agent-edited"))} }
                            code { class: "truncate font-mono text-xs text-foreground/70", "{diff.name}" }
                            DisclosureIcon {}
                        }
                        div { class: "mt-2 overflow-hidden rounded-lg ring-1 ring-inset ring-foreground/10",
                            div { class: "overflow-x-auto bg-foreground/[0.02] py-1 font-mono text-[11px] leading-relaxed",
                                for (i , line) in diff.lines.iter().enumerate() {
                                    div { key: "{i}", class: diff_line_class(line.kind), "{diff_line_prefix(line.kind)} {line.text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatTurnRow::ToolResult {
            content, is_error, ..
        } => rsx! {
            StandaloneToolResult { result_key: key as usize, content, is_error }
        },
        ChatTurnRow::Reconnect { attempt, total, .. } => rsx! {
            div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-2.5 rounded-xl px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-amber-500/[0.035]",
                ActivityIconView { kind: ActivityIcon::Reconnect }
                span {
                    class: "font-medium tabular-nums",
                    {translate_with(
                        "agent-reconnecting",
                        &[
                            ("attempt", TranslationValue::Number(attempt as i64)),
                            ("total", TranslationValue::Number(total as i64)),
                        ],
                    )}
                }
            }
        },
    }
}

#[component]
fn ToolChild(child: ChatToolChild) -> Element {
    match child {
        ChatToolChild::Tool(call) => rsx! { ToolChildCall { call } },
        ChatToolChild::Subagent(subagent) => rsx! { SubagentChild { subagent } },
        ChatToolChild::Result {
            index,
            content,
            is_error,
        } => rsx! {
            NestedToolResult { result_key: index as usize, content, is_error }
        },
    }
}

#[component]
fn ToolChildCall(call: ChatToolChildCall) -> Element {
    let label = ToolPresentation::for_call(call.kind, &call.fallback_label).label;
    rsx! {
        details { class: "disclosure text-xs text-muted-foreground",
            summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{label}" }
                DisclosureIcon {}
            }
            div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{call.name}" }
            ToolArgs { arguments: call.arguments }
        }
    }
}

#[component]
fn SubagentChild(subagent: ChatSubagentSummary) -> Element {
    let status_label = subagent_status_label(subagent.status);
    let status_class = subagent_status_class(subagent.status);
    let title = if subagent.title.is_empty() {
        translate("agent-subagent")
    } else {
        subagent.title.clone()
    };
    rsx! {
        details { class: "disclosure text-xs text-muted-foreground",
            summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{title}" }
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

fn subagent_status_label(status: ChatSubagentStatus) -> String {
    match status {
        ChatSubagentStatus::Running => translate("agent-status-running"),
        ChatSubagentStatus::Complete => translate("agent-status-done"),
        ChatSubagentStatus::Failed => translate("agent-status-failed"),
        ChatSubagentStatus::Pending => translate("agent-status-pending"),
    }
}

fn subagent_status_class(status: ChatSubagentStatus) -> &'static str {
    match status {
        ChatSubagentStatus::Running => "bg-violet-500/10 text-violet-700 dark:text-violet-300",
        ChatSubagentStatus::Complete => "bg-success/10 text-success",
        ChatSubagentStatus::Failed => "bg-red-500/10 text-red-700 dark:text-red-300",
        ChatSubagentStatus::Pending => "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    }
}

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
                summary { class: ROW_SUMMARY,
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

fn plan_glyph(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "✓",
        ChatPlanStatus::Active => "◐",
        ChatPlanStatus::Pending => "○",
    }
}

fn plan_glyph_class(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "text-success",
        ChatPlanStatus::Active => "text-amber-500",
        ChatPlanStatus::Pending => "text-muted-foreground",
    }
}

fn plan_text_class(status: ChatPlanStatus) -> &'static str {
    match status {
        ChatPlanStatus::Complete => "text-muted-foreground line-through",
        ChatPlanStatus::Active => "text-foreground",
        ChatPlanStatus::Pending => "text-muted-foreground",
    }
}

fn diff_line_class(kind: ChatDiffLineKind) -> &'static str {
    match kind {
        ChatDiffLineKind::Removed => "px-3 bg-red-500/10 text-red-300",
        ChatDiffLineKind::Added => "px-3 bg-success/10 text-success",
    }
}

fn diff_line_prefix(kind: ChatDiffLineKind) -> &'static str {
    match kind {
        ChatDiffLineKind::Removed => "-",
        ChatDiffLineKind::Added => "+",
    }
}

const CHAT_MD_CLASS: &str = "chat-md px-0.5 text-sm text-foreground/95 leading-[1.6] break-words \
    [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 \
    [&_:is(h1,h2,h3,h4)]:font-semibold [&_:is(h1,h2,h3,h4)]:leading-[1.3] \
    [&_:is(h1,h2,h3,h4)]:[margin:0.9em_0_0.35em] \
    [&_h1]:text-[1.35em] [&_h2]:text-[1.2em] [&_h3]:text-[1.05em] [&_h4]:text-[1em] \
    [&_p]:my-[0.5em] \
    [&_:is(ul,ol)]:pl-[1.4em] [&_:not(li)>:is(ul,ol)]:my-[0.4em] [&_li>:is(ul,ol)]:my-[0.15em] \
    [&_ul]:list-disc [&_ol]:list-decimal [&_li]:my-[0.15em] \
    [&_strong]:font-semibold [&_em]:italic \
    [&_a]:text-[color-mix(in_srgb,var(--agent-accent)_82%,currentColor)] \
    [&_a]:decoration-[color-mix(in_srgb,var(--agent-accent)_45%,transparent)] \
    [&_a]:underline-offset-[0.16em] \
    [&_:not(pre)>code]:font-mono [&_:not(pre)>code]:text-[0.88em] \
    [&_:not(pre)>code]:bg-[color-mix(in_srgb,var(--agent-accent)_10%,transparent)] \
    [&_:not(pre)>code]:border \
    [&_:not(pre)>code]:border-[color-mix(in_srgb,var(--agent-accent)_11%,transparent)] \
    [&_:not(pre)>code]:[padding:0.1em_0.35em] [&_:not(pre)>code]:rounded-[0.4em] \
    [&_pre]:bg-[linear-gradient(135deg,color-mix(in_srgb,var(--agent-accent)_7%,transparent),color-mix(in_srgb,var(--agent-accent)_3%,transparent))] \
    [&_pre]:border [&_pre]:border-[color-mix(in_srgb,var(--agent-accent)_11%,transparent)] \
    [&_pre]:[padding:0.7em_0.9em] [&_pre]:rounded-[0.7em] [&_pre]:overflow-x-auto \
    [&_pre]:my-[0.6em] [&_pre]:font-mono [&_pre_code]:text-[0.85em] \
    [&_blockquote]:border-l-2 \
    [&_blockquote]:border-l-[color-mix(in_srgb,var(--agent-accent)_48%,transparent)] \
    [&_blockquote]:pl-[0.8em] [&_blockquote]:my-[0.5em] [&_blockquote]:opacity-85 \
    [&_hr]:border-0 [&_hr]:border-t [&_hr]:border-t-[rgba(127,127,127,0.25)] [&_hr]:my-[0.9em] \
    [&_table]:border-collapse [&_table]:my-[0.5em] [&_table]:text-[0.95em] \
    [&_:is(th,td)]:border [&_:is(th,td)]:border-[rgba(127,127,127,0.3)] \
    [&_:is(th,td)]:[padding:0.3em_0.6em] [&_:is(th,td)]:text-left";

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

pub const MD_CSS: &str = r#"
.session-chat-page{background-image:none}
.chat-assistant-turn .disclosure>summary{transition:color 160ms ease}
.chat-assistant-turn .disclosure>summary:hover{color:color-mix(in srgb,currentColor 68%,var(--agent-accent))}
.agent-themed-activity{color:var(--agent-accent);background:color-mix(in srgb,var(--agent-accent) 11%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 18%,transparent)}
.python-activity-icon{background:linear-gradient(145deg,rgba(55,118,171,0.15),rgba(255,212,59,0.11));color:#3776ab;box-shadow:inset 0 0 0 1px rgba(55,118,171,0.3)}
.agent-working-label{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor)}
.agent-row-hover:hover{background:color-mix(in srgb,var(--agent-accent) 4%,transparent)}
.agent-code-panel,.user-context-content{background:rgba(127,127,127,0.07);box-shadow:inset 0 0 0 1px rgba(127,127,127,0.14)}
.agent-context-tree{border-color:color-mix(in srgb,var(--agent-accent) 22%,transparent)}
.agent-turn-meta{color:color-mix(in srgb,var(--agent-accent) 72%,currentColor);border-color:color-mix(in srgb,var(--agent-accent) 13%,transparent);background:color-mix(in srgb,var(--agent-accent) 7%,transparent)}
.agent-turn-meta-dot{background:var(--agent-accent)}
.user-context-panel{border-color:color-mix(in srgb,var(--agent-accent) 14%,transparent);background:color-mix(in srgb,var(--agent-accent) 5%,rgba(127,127,127,0.025))}
.user-context-panel>summary:hover{color:color-mix(in srgb,currentColor 65%,var(--agent-accent))}
@media (prefers-reduced-motion:reduce){.agent-chat-caret{animation:none}}
"#;
