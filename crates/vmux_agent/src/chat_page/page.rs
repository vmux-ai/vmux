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
        main { class: "flex h-screen flex-col bg-background text-foreground",
            header { class: "flex items-center gap-2 border-b border-foreground/10 px-4 py-2.5",
                span { class: "h-2 w-2 rounded-full bg-emerald-500" }
                span { class: "text-sm font-semibold capitalize", "{agent}" }
            }
            div { class: "flex-1 overflow-y-auto px-4 py-6",
                div { class: "mx-auto flex max-w-3xl flex-col gap-4",
                    for (i , msg) in messages.read().iter().enumerate() {
                        {render_message(i, msg)}
                    }
                    if status() == "streaming" {
                        div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                            span { class: "h-3 w-3 animate-spin rounded-full border-2 border-muted-foreground/40 border-t-foreground" }
                            "Working…"
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

            div { class: "border-t border-foreground/10 px-4 py-3",
                div { class: "mx-auto flex max-w-3xl items-end gap-2",
                    textarea {
                        class: "max-h-40 flex-1 resize-none rounded-xl bg-foreground/[0.06] px-3 py-2 text-sm ring-1 ring-inset ring-foreground/10 focus:outline-none focus:ring-foreground/20",
                        rows: "1",
                        placeholder: "Message the agent…",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                e.prevent_default();
                                do_submit(draft);
                            }
                        },
                    }
                    button {
                        class: "rounded-xl bg-foreground px-4 py-2 text-sm font-medium text-background hover:brightness-110 active:scale-[0.99]",
                        onclick: move |_| do_submit(draft),
                        "Send"
                    }
                }
            }
        }
    }
}

fn do_submit(mut draft: Signal<String>) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
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
                "bg-red-500/10 text-red-500"
            } else {
                "bg-foreground/[0.05] text-muted-foreground"
            };
            rsx! {
                div {
                    key: "{key}",
                    class: "max-w-[85%] self-start overflow-x-auto whitespace-pre-wrap rounded-xl px-3 py-2 font-mono text-xs {tone}",
                    "{content}"
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
        ChatBlock::ToolUse { name, args, .. } => rsx! {
            div {
                key: "{key}",
                class: "rounded-xl bg-foreground/[0.05] px-3 py-2 ring-1 ring-inset ring-foreground/10",
                div { class: "font-mono text-xs text-amber-500", "{name}" }
                if !args.is_empty() && args != "{}" {
                    pre { class: "mt-1 overflow-x-auto font-mono text-[11px] text-muted-foreground", "{args}" }
                }
            }
        },
    }
}
