#![allow(non_snake_case)]

use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatBlock, ChatMessage, ChatSnapshot, ChatSubmit,
};
use dioxus::prelude::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

/// The agent id from the page URL (`vmux://agent/<id>` → `<id>`); the chat UI is shared
/// across agents and only the id differs.
fn current_agent() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .and_then(|path| path.split('/').find(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "agent".to_string())
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let agent = current_agent();
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut status = use_signal(|| "idle".to_string());
    let mut error = use_signal(String::new);
    let mut approval = use_signal(|| Option::<(String, String)>::None);
    let mut draft = use_signal(String::new);
    let mut elapsed = use_signal(|| 0u32);

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            if matches!(status().as_str(), "streaming" | "installing") {
                elapsed.set(elapsed() + 1);
            } else if elapsed() != 0 {
                elapsed.set(0);
            }
        }
    });

    use_effect(move || {
        // Subscribe to any transcript/status change (each snapshot is a fresh `set`), then pin the
        // scroll container to the bottom so new output stays in view as the turn grows.
        let _ = messages.read().len();
        let _ = status.read();
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("chat-scroll"))
        {
            el.set_scroll_top(el.scroll_height());
        }
    });

    let _listener = use_bin_event_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snap| {
        if let Ok(parsed) = serde_json::from_str::<Vec<ChatMessage>>(&snap.messages_json) {
            messages.set(parsed);
        }
        status.set(snap.status.clone());
        error.set(snap.error.clone());
        if snap.status == "awaiting" {
            approval.set(Some((
                snap.approval_call_id.clone(),
                snap.approval_name.clone(),
            )));
        } else {
            approval.set(None);
        }
    });

    rsx! {
        main {
            class: "relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(129,140,248,0.05), transparent 55%);",
            div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                div { class: "absolute left-1/2 top-[-10%] h-[30rem] w-[30rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/10" }
            }
            header { class: "relative z-10 flex items-center gap-2.5 border-b border-foreground/10 bg-background/50 px-5 py-3 backdrop-blur-xl",
                span { class: "h-2.5 w-2.5 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.65)]" }
                span { class: "bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-sm font-semibold capitalize text-transparent",
                    "{agent}"
                }
            }
            div {
                id: "chat-scroll",
                class: "relative z-10 flex-1 overflow-y-auto px-4 py-6",
                div { class: "mx-auto flex max-w-3xl flex-col gap-4",
                    if messages.read().is_empty() && status() == "idle" {
                        div { class: "flex flex-col items-center gap-2 py-24 text-center",
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{agent}"
                            }
                            p { class: "text-sm text-muted-foreground", "Ready when you are." }
                        }
                    }
                    for (i , msg) in messages.read().iter().enumerate() {
                        {render_message(i, msg)}
                    }
                    if status() == "streaming" {
                        div { class: "flex items-center gap-2.5 text-sm",
                            span { class: "flex items-end gap-1",
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.32s]" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.16s]" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70" }
                            }
                            span { class: "animate-pulse bg-gradient-to-r from-foreground/45 via-foreground to-foreground/45 bg-clip-text font-medium text-transparent", "Working…" }
                            span { class: "tabular-nums text-xs text-muted-foreground", "{fmt_elapsed(elapsed())}" }
                        }
                    }
                    if status() == "installing" {
                        div { class: "flex items-center gap-2.5 text-sm",
                            span { class: "flex items-end gap-1",
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.32s]" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.16s]" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70" }
                            }
                            span { class: "animate-pulse bg-gradient-to-r from-foreground/45 via-foreground to-foreground/45 bg-clip-text font-medium text-transparent", "{error}" }
                        }
                    }
                    if status() == "errored" {
                        div { class: "rounded-xl bg-red-500/10 px-4 py-3 text-sm text-red-600 ring-1 ring-inset ring-red-500/20 dark:text-red-300",
                            "{error}"
                        }
                    }
                }
            }

            if let Some((call_id, name)) = approval() {
                div { class: "border-t border-foreground/10 bg-foreground/[0.04] px-4 py-3",
                    div { class: "mx-auto flex max-w-3xl items-center gap-3",
                        span { class: "flex-1 text-sm text-foreground",
                            "Allow "
                            code { class: "font-mono text-amber-500", "{name}" }
                            "?"
                        }
                        button {
                            class: "rounded-lg px-3 py-1.5 text-sm text-muted-foreground hover:bg-foreground/10",
                            onclick: {
                                let call_id = call_id.clone();
                                move |_| send_approval(call_id.clone(), 0)
                            },
                            "Deny"
                        }
                        button {
                            class: "rounded-lg bg-foreground/10 px-3 py-1.5 text-sm hover:bg-foreground/20",
                            onclick: {
                                let call_id = call_id.clone();
                                move |_| send_approval(call_id.clone(), 2)
                            },
                            "Allow always"
                        }
                        button {
                            class: "rounded-lg bg-foreground px-3 py-1.5 text-sm font-medium text-background",
                            onclick: {
                                let call_id = call_id.clone();
                                move |_| send_approval(call_id.clone(), 1)
                            },
                            "Allow"
                        }
                    }
                }
            }

            div { class: "relative z-10 border-t border-foreground/10 bg-background/50 px-4 py-3 backdrop-blur-xl",
                div { class: "mx-auto flex max-w-3xl items-end gap-2",
                    textarea {
                        class: "max-h-40 flex-1 resize-none rounded-xl bg-foreground/[0.06] px-3.5 py-2.5 text-sm ring-1 ring-inset ring-foreground/10 transition focus:bg-foreground/[0.09] focus:outline-none focus:ring-foreground/25",
                        rows: "1",
                        placeholder: "Message the agent…",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                e.prevent_default();
                                do_submit(draft, messages, status);
                            }
                        },
                    }
                    button {
                        class: "rounded-xl bg-foreground px-4 py-2 text-sm font-medium text-background hover:brightness-110 active:scale-[0.99]",
                        onclick: move |_| do_submit(draft, messages, status),
                        "Send"
                    }
                }
            }
        }
    }
}

fn do_submit(
    mut draft: Signal<String>,
    mut messages: Signal<Vec<ChatMessage>>,
    mut status: Signal<String>,
) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
    // Optimistically append the prompt + show a working state so it appears instantly; the host
    // snapshot (which includes this same user turn) reconciles it.
    messages.write().push(ChatMessage::User { text: text.clone() });
    status.set("streaming".to_string());
    let _ = try_cef_bin_emit_rkyv(&ChatSubmit { text });
    draft.set(String::new());
}

fn send_approval(call_id: String, decision: u8) {
    let _ = try_cef_bin_emit_rkyv(&ChatApproval { call_id, decision });
}

fn render_message(key: usize, msg: &ChatMessage) -> Element {
    match msg {
        ChatMessage::User { text } => rsx! {
            div {
                key: "{key}",
                class: "max-w-[80%] self-end whitespace-pre-wrap rounded-2xl bg-foreground/[0.08] px-4 py-2.5 text-sm",
                "{text}"
            }
        },
        ChatMessage::Assistant { blocks } => rsx! {
            div { key: "{key}", class: "flex max-w-[85%] flex-col gap-2 self-start",
                for (j , block) in blocks.iter().enumerate() {
                    {render_block(j, block)}
                }
            }
        },
        ChatMessage::ToolResult {
            content, is_error, ..
        } => {
            let tone = if *is_error {
                "text-red-500"
            } else {
                "text-muted-foreground"
            };
            let label = if *is_error { "Error" } else { "Output" };
            rsx! {
                details {
                    key: "{key}",
                    class: "group max-w-[85%] self-start rounded-xl bg-foreground/[0.05] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 text-xs {tone} list-none [&::-webkit-details-marker]:hidden",
                        span { class: "text-[10px] transition group-open:rotate-90", "▸" }
                        span { "{label}" }
                    }
                    pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap font-mono text-[11px] text-muted-foreground", "{content}" }
                }
            }
        }
    }
}

fn render_block(key: usize, block: &ChatBlock) -> Element {
    match block {
        ChatBlock::Text(text) => rsx! {
            div { key: "{key}", class: "whitespace-pre-wrap text-sm leading-relaxed", "{text}" }
        },
        ChatBlock::Thinking(text) => rsx! {
            details {
                key: "{key}",
                class: "group rounded-xl bg-foreground/[0.03] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                summary { class: "flex cursor-pointer select-none items-center gap-2 text-xs text-muted-foreground list-none [&::-webkit-details-marker]:hidden",
                    span { class: "text-[10px] transition group-open:rotate-90", "▸" }
                    span { class: "font-medium", "Thinking" }
                }
                div { class: "mt-2 whitespace-pre-wrap border-l-2 border-foreground/10 pl-3 text-xs italic leading-relaxed text-muted-foreground", "{text}" }
            }
        },
        ChatBlock::ToolUse { name, args, .. } => rsx! {
            details {
                key: "{key}",
                class: "group rounded-xl bg-foreground/[0.05] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "text-[10px] text-muted-foreground transition group-open:rotate-90", "▸" }
                    span { class: "font-mono text-xs text-amber-500", "{name}" }
                }
                if !args.is_empty() && args != "{}" {
                    pre { class: "mt-1.5 overflow-x-auto font-mono text-[11px] text-muted-foreground", "{args}" }
                }
            }
        },
        ChatBlock::Plan { steps } => {
            let n = steps.len();
            rsx! {
                details {
                    key: "{key}",
                    open: true,
                    class: "group rounded-xl bg-foreground/[0.04] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 text-xs list-none [&::-webkit-details-marker]:hidden",
                        span { class: "text-[10px] text-muted-foreground transition group-open:rotate-90", "▸" }
                        span { class: "font-medium text-foreground", "Plan" }
                        span { class: "text-muted-foreground", "· {n} tasks" }
                    }
                    ul { class: "mt-2 flex flex-col gap-1.5",
                        for (i , step) in steps.iter().enumerate() {
                            li { key: "{i}", class: "flex items-start gap-2 text-xs",
                                span { class: "mt-px {plan_glyph_class(&step.status)}", "{plan_glyph(&step.status)}" }
                                span { class: plan_text_class(&step.status), "{step.content}" }
                            }
                        }
                    }
                }
            }
        },
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
                div {
                    key: "{key}",
                    class: "overflow-hidden rounded-xl ring-1 ring-inset ring-foreground/10",
                    div { class: "flex items-center gap-2 border-b border-foreground/10 bg-foreground/[0.05] px-3 py-1.5",
                        span { class: "font-mono text-xs font-medium text-amber-400", "{fname}" }
                        span { class: "text-[10px] uppercase tracking-wide text-muted-foreground", "proposed edit" }
                    }
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

fn fmt_elapsed(secs: u32) -> String {
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
