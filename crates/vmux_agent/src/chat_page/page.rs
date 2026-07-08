#![allow(non_snake_case)]

use crate::chat_page::event::{
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatBlock, ChatCancel, ChatClearQueue, ChatMessage,
    ChatResume, ChatSnapshot, ChatSubmit, RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry,
    ResumableSessions, ResumeListRequest, ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT,
    SlashCommandEntry, SlashCommands,
};
use dioxus::prelude::*;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

/// True when the page has a non-collapsed text selection — so Ctrl+C should copy, not interrupt.
fn has_text_selection() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .map(|s| !s.is_collapsed())
        .unwrap_or(false)
}

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
    let mut agent_name = use_signal(String::new);
    let mut agent_icon = use_signal(String::new);
    let mut accent = use_signal(String::new);
    let mut draft = use_signal(String::new);
    let mut elapsed = use_signal(|| 0u32);
    let mut at_bottom = use_signal(|| true);
    let mut last_top = use_signal(|| 0i32);
    let mut queued = use_signal(Vec::<String>::new);
    let mut paused = use_signal(|| false);
    // Slash-menu state: `resume_mode` false = command list (typing `/…`), true = session picker.
    let mut slash_cmds = use_signal(Vec::<SlashCommandEntry>::new);
    let mut sessions = use_signal(Vec::<ResumableSessionEntry>::new);
    let mut menu_sel = use_signal(|| 0usize);
    let mut resume_mode = use_signal(|| false);

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
        // Subscribe to any transcript/status change (each snapshot is a fresh `set`). Only pin to
        // the bottom when the user is already there — if they scrolled up to read, leave them.
        let _ = messages.read().len();
        let _ = status.read();
        if !*at_bottom.peek() {
            return;
        }
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
        queued.set(snap.queued.clone());
        paused.set(snap.paused);
        agent_name.set(snap.agent_name.clone());
        agent_icon.set(snap.agent_icon.clone());
        accent.set(snap.accent_color.clone());
        if snap.status == "awaiting" {
            approval.set(Some((
                snap.approval_call_id.clone(),
                snap.approval_name.clone(),
            )));
        } else {
            approval.set(None);
        }
    });

    let _cmds = use_bin_event_listener::<SlashCommands, _>(SLASH_COMMANDS_EVENT, move |s| {
        slash_cmds.set(s.commands.clone());
    });
    let _sess = use_bin_event_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |s| {
        sessions.set(s.sessions.clone());
        menu_sel.set(0);
    });

    // Drive the document (→ tab) title from the agent + its live run-state, so the user can see
    // what each agent is doing without opening the pane.
    use_effect(move || {
        let name = {
            let n = agent_name();
            if n.is_empty() { current_agent() } else { n }
        };
        let title = match status().as_str() {
            "streaming" => format!("● {name}"),
            "installing" => format!("{name} — Installing…"),
            "awaiting" => format!("{name} — Approve?"),
            "errored" => format!("{name} — Error"),
            _ => name,
        };
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&title);
        }
    });

    let header_name = {
        let n = agent_name();
        if n.is_empty() { agent.clone() } else { n }
    };
    let dot_style = if accent().is_empty() {
        String::new()
    } else {
        format!("background:{}", accent())
    };

    let draft_val = draft();
    let menu_open = slash_query(&draft_val).is_some() && !resume_mode();
    let session_menu_open = resume_mode() && !sessions.read().is_empty();
    let filtered_cmds: Vec<SlashCommandEntry> = {
        let q = slash_query(&draft_val).unwrap_or("").to_lowercase();
        slash_cmds
            .read()
            .iter()
            .filter(|c| c.name.starts_with(&q))
            .cloned()
            .collect()
    };
    let cmd_menu_open = menu_open && !filtered_cmds.is_empty();

    rsx! {
        main {
            class: "relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(129,140,248,0.05), transparent 55%);",
            style { dangerous_inner_html: MD_CSS }
            div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                div { class: "absolute left-1/2 top-[-10%] h-[30rem] w-[30rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/10" }
            }
            header { class: "relative z-10 flex items-center gap-2.5 border-b border-foreground/10 bg-background/50 px-5 py-3 backdrop-blur-xl",
                {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-6 w-6 text-[11px]")}
                span { class: "h-2.5 w-2.5 rounded-full {status_dot_class(&status())}" }
                span { class: "bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-sm font-semibold capitalize text-transparent",
                    "{header_name}"
                }
            }
            div {
                id: "chat-scroll",
                class: "relative z-10 flex-1 overflow-y-auto px-4 py-6",
                onscroll: move |_| {
                    if let Some(el) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id("chat-scroll"))
                    {
                        let top = el.scroll_top();
                        let dist = el.scroll_height() - top - el.client_height();
                        // Re-pin once the user reaches the bottom; unpin only when they scroll UP
                        // (scroll_top decreases). Never unpin from our own programmatic
                        // scroll-to-bottom, which only moves down and would otherwise poison
                        // `at_bottom` with a stale, mid-stream scroll height.
                        if dist <= 48 {
                            at_bottom.set(true);
                        } else if top < *last_top.peek() - 4 {
                            at_bottom.set(false);
                        }
                        last_top.set(top);
                    }
                },
                div { class: "mx-auto flex max-w-3xl flex-col gap-4",
                    if messages.read().is_empty() && status() == "idle" {
                        div { class: "flex flex-col items-center gap-3 py-24 text-center",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
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
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.32s]", style: "{dot_style}" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.16s]", style: "{dot_style}" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70", style: "{dot_style}" }
                            }
                            span { class: "animate-pulse bg-gradient-to-r from-foreground/45 via-foreground to-foreground/45 bg-clip-text font-medium text-transparent", "Working…" }
                            span { class: "tabular-nums text-xs text-muted-foreground", "{fmt_elapsed(elapsed())}" }
                        }
                    }
                    if status() == "installing" {
                        div { class: "flex items-center gap-2.5 text-sm",
                            span { class: "flex items-end gap-1",
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.32s]", style: "{dot_style}" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70 [animation-delay:-0.16s]", style: "{dot_style}" }
                                span { class: "h-1.5 w-1.5 animate-bounce rounded-full bg-foreground/70", style: "{dot_style}" }
                            }
                            span { class: "animate-pulse bg-gradient-to-r from-foreground/45 via-foreground to-foreground/45 bg-clip-text font-medium text-transparent", "{error}" }
                        }
                    }
                    if status() == "errored" {
                        div { class: "rounded-xl bg-red-500/10 px-4 py-3 text-sm text-red-600 ring-1 ring-inset ring-red-500/20 dark:text-red-300",
                            "{error}"
                        }
                    }
                    if paused() {
                        div { class: "flex items-center gap-3 py-1 text-xs text-muted-foreground",
                            span { class: "h-px flex-1 bg-foreground/10" }
                            span { class: "shrink-0", "interrupted" }
                            span { class: "h-px flex-1 bg-foreground/10" }
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
                div { class: "relative mx-auto flex max-w-3xl flex-col gap-2",
                    if cmd_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 w-full overflow-hidden rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
                            for (i , c) in filtered_cmds.iter().enumerate() {
                                div {
                                    key: "sc{i}",
                                    class: if i == menu_sel() { "flex items-baseline gap-3 px-3.5 py-2 text-sm bg-foreground/10" } else { "flex items-baseline gap-3 px-3.5 py-2 text-sm" },
                                    span { class: "font-medium text-foreground", "/{c.name}" }
                                    span { class: "text-xs text-muted-foreground", "{c.description}" }
                                }
                            }
                        }
                    }
                    if session_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
                            for (i , s) in sessions.read().iter().enumerate() {
                                div {
                                    key: "rs{i}",
                                    class: if i == menu_sel() { "flex flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex flex-col gap-0.5 px-3.5 py-2" },
                                    span { class: "truncate text-sm text-foreground", "{s.title}" }
                                    span { class: "truncate text-xs text-muted-foreground", "{s.subtitle}" }
                                }
                            }
                        }
                    }
                    if !queued.read().is_empty() {
                        div { class: "flex flex-col items-end gap-1.5",
                            for (qi , qtext) in queued.read().iter().enumerate() {
                                div {
                                    key: "q{qi}",
                                    class: "flex max-w-[80%] items-baseline gap-2 rounded-2xl border border-dashed border-foreground/20 bg-foreground/[0.03] px-3.5 py-2 text-sm text-muted-foreground",
                                    span { class: "shrink-0 text-[10px] uppercase tracking-wide text-foreground/40", "queued" }
                                    span { class: "whitespace-pre-wrap break-words", "{qtext}" }
                                }
                            }
                            if paused() {
                                div { class: "flex items-center gap-1",
                                    button {
                                        class: "flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                                        title: "Resume queued prompts",
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ChatResume);
                                        },
                                        svg {
                                            class: "h-3.5 w-3.5",
                                            view_box: "0 0 24 24",
                                            fill: "currentColor",
                                            path { d: "M8 5v14l11-7z" }
                                        }
                                        span { class: "tabular-nums", "{queued.read().len()}" }
                                    }
                                    button {
                                        class: "flex items-center rounded-lg p-1 text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                                        title: "Clear queue",
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ChatClearQueue);
                                        },
                                        svg {
                                            class: "h-3.5 w-3.5",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            path { d: "M6 6l12 12M18 6L6 18" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-end gap-2",
                        textarea {
                            class: "max-h-40 flex-1 resize-none rounded-xl bg-foreground/[0.06] px-3.5 py-2.5 text-sm ring-1 ring-inset ring-foreground/10 transition focus:bg-foreground/[0.09] focus:outline-none focus:ring-foreground/25",
                            rows: "1",
                            placeholder: "Message the agent…",
                            value: "{draft}",
                            oninput: move |e| draft.set(e.value()),
                            onkeydown: move |e| {
                                let streaming = matches!(status().as_str(), "streaming" | "awaiting");
                                let draft_now = draft.peek().clone();
                                let sess_open = *resume_mode.peek() && !sessions.peek().is_empty();
                                let cmd_items: Vec<SlashCommandEntry> = if slash_query(&draft_now)
                                    .is_some()
                                    && !*resume_mode.peek()
                                {
                                    let q = slash_query(&draft_now).unwrap_or("").to_lowercase();
                                    slash_cmds
                                        .peek()
                                        .iter()
                                        .filter(|c| c.name.starts_with(&q))
                                        .cloned()
                                        .collect()
                                } else {
                                    Vec::new()
                                };
                                let any_menu = !cmd_items.is_empty() || sess_open;
                                let len = if sess_open {
                                    sessions.peek().len()
                                } else {
                                    cmd_items.len()
                                };

                                if any_menu && matches!(e.key(), Key::ArrowDown | Key::ArrowUp) {
                                    e.prevent_default();
                                    if len > 0 {
                                        let cur = *menu_sel.peek();
                                        let next = match e.key() {
                                            Key::ArrowDown => (cur + 1) % len,
                                            _ => (cur + len - 1) % len,
                                        };
                                        menu_sel.set(next);
                                    }
                                    return;
                                }
                                if any_menu && e.key() == Key::Enter && !e.modifiers().shift() {
                                    e.prevent_default();
                                    let sel = *menu_sel.peek();
                                    if sess_open {
                                        if let Some(s) = sessions.peek().get(sel) {
                                            let _ = try_cef_bin_emit_rkyv(&ResumeSession {
                                                kind: s.kind.clone(),
                                                sid: s.sid.clone(),
                                                cwd: s.cwd.clone(),
                                            });
                                        }
                                        resume_mode.set(false);
                                        draft.set(String::new());
                                    } else if let Some(c) = cmd_items.get(sel) {
                                        run_slash_command(&c.name, resume_mode, draft);
                                    }
                                    return;
                                }
                                if any_menu && e.key() == Key::Escape {
                                    e.prevent_default();
                                    resume_mode.set(false);
                                    draft.set(String::new());
                                    return;
                                }

                                if e.key() == Key::Enter && !e.modifiers().shift() {
                                    e.prevent_default();
                                    do_submit(draft, at_bottom);
                                } else if e.key() == Key::Escape {
                                    if streaming {
                                        e.prevent_default();
                                        let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                    } else if !draft.peek().is_empty() {
                                        draft.set(String::new());
                                    }
                                } else if e.modifiers().ctrl()
                                    && matches!(e.key(), Key::Character(c) if c == "c")
                                    && streaming
                                    && !has_text_selection()
                                {
                                    e.prevent_default();
                                    let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                }
                            },
                        }
                        if matches!(status().as_str(), "streaming" | "awaiting") {
                            button {
                                class: "flex items-center justify-center rounded-xl p-2.5 text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground active:scale-[0.98]",
                                title: "Interrupt (Esc)",
                                onclick: move |_| {
                                    let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                },
                                svg {
                                    class: "h-4 w-4",
                                    view_box: "0 0 24 24",
                                    fill: "currentColor",
                                    rect { x: "6", y: "6", width: "12", height: "12", rx: "2.5" }
                                }
                            }
                        } else {
                            button {
                                class: "flex items-center justify-center rounded-xl p-2.5 text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground active:scale-[0.98]",
                                title: "Send (Enter)",
                                onclick: move |_| do_submit(draft, at_bottom),
                                svg {
                                    class: "h-4 w-4",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M12 19V5" }
                                    path { d: "M5 12l7-7 7 7" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A draft is in slash mode when it is a single `/token` (a leading slash, no whitespace yet).
fn slash_query(draft: &str) -> Option<&str> {
    let d = draft.strip_prefix('/')?;
    if d.contains(char::is_whitespace) {
        None
    } else {
        Some(d)
    }
}

/// Run a selected vmux slash command. `resume` opens the session picker; `cli`/`acp` hand the
/// current session to the other runtime. Unknown names are ignored (the raw text still submits
/// via the normal Enter path).
fn run_slash_command(name: &str, mut resume_mode: Signal<bool>, mut draft: Signal<String>) {
    match name {
        "resume" => {
            let _ = try_cef_bin_emit_rkyv(&ResumeListRequest);
            resume_mode.set(true);
            draft.set(String::new());
        }
        "cli" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "cli".into() });
            draft.set(String::new());
        }
        "acp" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "acp".into() });
            draft.set(String::new());
        }
        _ => {}
    }
}

/// Emit the draft as a submit intent, clearing the input only if the IPC succeeded so a failed
/// emit never silently swallows the user's message. The queued/sent turn arrives via snapshot.
fn do_submit(mut draft: Signal<String>, mut at_bottom: Signal<bool>) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
    if try_cef_bin_emit_rkyv(&ChatSubmit { text }).is_err() {
        return;
    }
    at_bottom.set(true);
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
            div {
                key: "{key}",
                class: "chat-md text-sm leading-relaxed",
                dangerous_inner_html: md_to_html(text),
            }
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

fn status_dot_class(status: &str) -> &'static str {
    match status {
        "streaming" => "bg-amber-400 animate-pulse shadow-[0_0_8px_rgba(251,191,36,0.65)]",
        "installing" => "bg-sky-400 animate-pulse shadow-[0_0_8px_rgba(56,189,248,0.65)]",
        "awaiting" => "bg-violet-400 animate-pulse shadow-[0_0_8px_rgba(167,139,250,0.65)]",
        "errored" => "bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.65)]",
        _ => "bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.65)]",
    }
}

/// The agent avatar: its favicon if resolvable, else an accent-filled circle with the initial.
fn avatar_node(icon: &str, accent: &str, agent: &str, name: &str, size_class: &str) -> Element {
    let url = format!("vmux://agent/{agent}");
    let src = favicon_src_for_url(icon, &url);
    let initial: String = name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_default();
    let fallback = if accent.is_empty() { "#6366f1" } else { accent };
    let style = if src.is_some() {
        String::new()
    } else {
        format!("background:{fallback}")
    };
    rsx! {
        div {
            class: "flex shrink-0 items-center justify-center overflow-hidden rounded-full font-semibold text-white {size_class}",
            style: "{style}",
            if let Some(src) = src.as_ref() {
                img { class: "h-full w-full object-cover", src: "{src}" }
            } else {
                "{initial}"
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
const MD_CSS: &str = r#"
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
.chat-md a{color:#6ea8fe;text-decoration:underline}
.chat-md code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.88em;background:rgba(127,127,127,0.18);padding:0.1em 0.35em;border-radius:0.35em}
.chat-md pre{background:rgba(127,127,127,0.14);padding:0.7em 0.9em;border-radius:0.6em;overflow-x:auto;margin:0.6em 0}
.chat-md pre code{background:none;padding:0;font-size:0.85em}
.chat-md blockquote{border-left:2px solid rgba(127,127,127,0.4);padding-left:0.8em;margin:0.5em 0;opacity:0.85}
.chat-md hr{border:0;border-top:1px solid rgba(127,127,127,0.25);margin:0.9em 0}
.chat-md table{border-collapse:collapse;margin:0.5em 0;font-size:0.95em}
.chat-md th,.chat-md td{border:1px solid rgba(127,127,127,0.3);padding:0.3em 0.6em;text-align:left}
"#;
