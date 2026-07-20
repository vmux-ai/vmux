#![allow(non_snake_case)]

use dioxus::prelude::*;
use futures_util::StreamExt;
use gloo_net::eventsource::futures::EventSource;
use gloo_net::http::Request;
use wasm_bindgen::JsValue;

use crate::message::{AssistantBlock, Message, PlanStep};
use crate::remote::{
    ApprovalRequest, PairRequest, PromptRequest, RemoteApproval, RemoteEvent, RemoteSession,
    RemoteStatus,
};

#[derive(Clone, Copy, PartialEq)]
enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

enum ApiError {
    Unauthorized,
    Message(String),
}

#[component]
pub fn Page() -> Element {
    let mut auth = use_signal(|| AuthState::Loading);
    let mut pair_token = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut sessions = use_signal(Vec::<RemoteSession>::new);
    let current = use_signal(|| None::<RemoteSession>);
    let mut messages = use_signal(Vec::<Message>::new);
    let mut live_delta = use_signal(String::new);
    let mut status = use_signal(|| RemoteStatus::Idle);
    let mut approval = use_signal(|| None::<RemoteApproval>);
    let mut draft = use_signal(String::new);
    let connected = use_signal(|| false);
    let mut drawer = use_signal(|| false);
    let stream_generation = use_signal(|| 0_u64);

    use_effect(register_service_worker);
    use_effect(move || {
        let _ = messages.read().len();
        let _ = live_delta.read().len();
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("remote-chat-scroll"))
        {
            element.set_scroll_top(element.scroll_height());
        }
    });

    use_future(move || async move {
        if let Some(token) = token_from_hash() {
            pair_token.set(token.clone());
            match pair(&token).await {
                Ok(()) => clear_hash(),
                Err(message) => {
                    error.set(message);
                    auth.set(AuthState::Unpaired);
                    return;
                }
            }
        }
        match fetch_sessions().await {
            Ok(next) => {
                auth.set(AuthState::Paired);
                sessions.set(next.clone());
                if let Some(first) = next.first().cloned() {
                    open_session(
                        first,
                        current,
                        messages,
                        live_delta,
                        status,
                        approval,
                        connected,
                        drawer,
                        stream_generation,
                    );
                }
            }
            Err(ApiError::Unauthorized) => auth.set(AuthState::Unpaired),
            Err(ApiError::Message(message)) => {
                error.set(message);
                auth.set(AuthState::Unpaired);
            }
        }
    });

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            if auth() != AuthState::Paired {
                continue;
            }
            let Ok(next) = fetch_sessions().await else {
                continue;
            };
            sessions.set(next.clone());
            if current().is_none()
                && let Some(first) = next.first().cloned()
            {
                open_session(
                    first,
                    current,
                    messages,
                    live_delta,
                    status,
                    approval,
                    connected,
                    drawer,
                    stream_generation,
                );
            }
        }
    });

    if auth() == AuthState::Loading {
        return rsx! {
            RemoteHead {}
            div { class: "flex h-dvh items-center justify-center bg-[#0b0b0d] text-white",
                div { class: "h-7 w-7 animate-spin rounded-full border-2 border-white/20 border-t-white" }
            }
        };
    }

    if auth() == AuthState::Unpaired {
        return rsx! {
            RemoteHead {}
            PairScreen {
                token: pair_token(),
                error: error(),
                on_token: move |value| pair_token.set(value),
                on_pair: move |_| {
                    let token = pair_token().trim().to_string();
                    if token.is_empty() {
                        return;
                    }
                    spawn(async move {
                        match pair(&token).await {
                            Ok(()) => match fetch_sessions().await {
                                Ok(next) => {
                                    error.set(String::new());
                                    auth.set(AuthState::Paired);
                                    sessions.set(next.clone());
                                    if let Some(first) = next.first().cloned() {
                                        open_session(
                                            first,
                                            current,
                                            messages,
                                            live_delta,
                                            status,
                                            approval,
                                            connected,
                                            drawer,
                                            stream_generation,
                                        );
                                    }
                                }
                                Err(ApiError::Unauthorized) => {
                                    error.set("Pairing was rejected.".to_string())
                                }
                                Err(ApiError::Message(message)) => error.set(message),
                            },
                            Err(message) => error.set(message),
                        }
                    });
                },
            }
        };
    }

    let current_value = current();
    let is_streaming = matches!(status(), RemoteStatus::Streaming);
    let can_send = current_value.is_some() && !draft().trim().is_empty();
    let selected_sid = current_value
        .as_ref()
        .map(|session| session.sid.clone())
        .unwrap_or_default();
    let approval_sid = selected_sid.clone();
    let submit_sid = selected_sid.clone();
    let cancel_sid = selected_sid.clone();

    rsx! {
        RemoteHead {}
        div { class: "flex h-dvh min-h-0 bg-[#0b0b0d] text-zinc-100",
            aside { class: "hidden w-80 shrink-0 border-r border-white/10 bg-black/20 md:flex md:flex-col",
                SessionList {
                    sessions: sessions(),
                    selected_sid: selected_sid.clone(),
                    on_select: move |session| open_session(
                        session,
                        current,
                        messages,
                        live_delta,
                        status,
                        approval,
                        connected,
                        drawer,
                        stream_generation,
                    ),
                }
            }
            if drawer() {
                div {
                    class: "fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden",
                    onclick: move |_| drawer.set(false),
                    aside {
                        class: "h-full w-[88%] max-w-sm border-r border-white/10 bg-[#101014] shadow-2xl",
                        onclick: move |event| event.stop_propagation(),
                        SessionList {
                            sessions: sessions(),
                            selected_sid: selected_sid.clone(),
                            on_select: move |session| open_session(
                                session,
                                current,
                                messages,
                                live_delta,
                                status,
                                approval,
                                connected,
                                drawer,
                                stream_generation,
                            ),
                        }
                    }
                }
            }
            main { class: "flex min-w-0 flex-1 flex-col",
                header { class: "flex h-14 shrink-0 items-center gap-3 border-b border-white/10 bg-[#0b0b0d]/90 px-3 backdrop-blur-xl sm:px-4",
                    button {
                        class: "flex h-9 w-9 items-center justify-center rounded-xl text-zinc-400 hover:bg-white/10 hover:text-white md:hidden",
                        onclick: move |_| drawer.set(true),
                        aria_label: "Open sessions",
                        "☰"
                    }
                    if let Some(session) = current_value.as_ref() {
                        div { class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-violet-500 to-cyan-400 text-sm font-bold text-white shadow-lg shadow-violet-500/20",
                            {session.name.chars().next().unwrap_or('V').to_uppercase().to_string()}
                        }
                        div { class: "min-w-0 flex-1",
                            div { class: "flex items-center gap-2",
                                h1 { class: "truncate text-sm font-semibold", "{session.name}" }
                                span { class: status_dot(&status()) }
                            }
                            div { class: "truncate text-[11px] text-zinc-500",
                                {session.model.clone().unwrap_or_else(|| session.runtime.clone())}
                                " · "
                                {cwd_name(&session.cwd)}
                            }
                        }
                        div { class: if connected() { "text-[10px] font-medium uppercase tracking-wider text-emerald-400" } else { "text-[10px] font-medium uppercase tracking-wider text-amber-400" },
                            if connected() { "live" } else { "connecting" }
                        }
                    } else {
                        h1 { class: "text-sm font-semibold", "Vmux Remote" }
                    }
                }
                div { id: "remote-chat-scroll", class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-5 sm:px-5",
                    div { class: "mx-auto flex min-h-full max-w-3xl flex-col gap-4",
                        if current_value.is_none() {
                            EmptyState { has_sessions: !sessions().is_empty(), on_open: move |_| drawer.set(true) }
                        } else if messages().is_empty() && live_delta().is_empty() {
                            div { class: "my-auto flex flex-col items-center gap-3 py-16 text-center",
                                div { class: "flex h-16 w-16 items-center justify-center rounded-3xl bg-gradient-to-br from-violet-500/25 to-cyan-400/20 text-2xl ring-1 ring-inset ring-white/10", "V" }
                                h2 { class: "text-xl font-semibold", "Continue from anywhere" }
                                p { class: "max-w-xs text-sm leading-relaxed text-zinc-500", "This conversation is running on your Mac." }
                            }
                        }
                        for (index, message) in messages().into_iter().enumerate() {
                            MessageView { key: "message-{index}", message }
                        }
                        if !live_delta().is_empty() {
                            div { class: "mr-8 rounded-2xl rounded-bl-md border border-white/10 bg-white/[0.04] px-4 py-3 text-sm leading-6 text-zinc-200 sm:mr-20",
                                div { class: "whitespace-pre-wrap break-words", "{live_delta}" }
                                span { class: "ml-1 inline-block h-4 w-1 animate-pulse rounded-full bg-violet-400 align-middle" }
                            }
                        }
                        if let RemoteStatus::Errored(message) = status() {
                            div { class: "rounded-2xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-200", "{message}" }
                        }
                    }
                }
                if let Some(pending) = approval() {
                    ApprovalBar {
                        approval: pending,
                        on_decide: move |(call_id, allow)| {
                            let sid = approval_sid.clone();
                            approval.set(None);
                            spawn(async move {
                                let _ = post_json(
                                    &format!("/api/sessions/{sid}/approval"),
                                    &ApprovalRequest { call_id, allow },
                                )
                                .await;
                            });
                        },
                    }
                }
                if current_value.is_some() {
                    div { class: "shrink-0 border-t border-white/10 bg-[#0b0b0d]/95 px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-3 backdrop-blur-xl sm:px-5",
                        form {
                            class: "mx-auto flex max-w-3xl items-end gap-2 rounded-2xl border border-white/10 bg-white/[0.055] p-2 shadow-2xl shadow-black/30 focus-within:border-violet-400/40 focus-within:bg-white/[0.07]",
                            onsubmit: move |event| {
                                event.prevent_default();
                                let text = draft().trim().to_string();
                                if text.is_empty() || submit_sid.is_empty() {
                                    return;
                                }
                                let sid = submit_sid.clone();
                                draft.set(String::new());
                                live_delta.set(String::new());
                                status.set(RemoteStatus::Streaming);
                                messages.write().push(Message::user(text.clone()));
                                spawn(async move {
                                    if let Err(failure) = post_json(
                                        &format!("/api/sessions/{sid}/messages"),
                                        &PromptRequest { text },
                                    )
                                    .await
                                    {
                                        let message = match failure {
                                            ApiError::Unauthorized => {
                                                "Phone pairing expired.".to_string()
                                            }
                                            ApiError::Message(message) => message,
                                        };
                                        status.set(RemoteStatus::Errored(message));
                                    }
                                });
                            },
                            textarea {
                                class: "max-h-36 min-h-10 flex-1 resize-none bg-transparent px-2 py-2 text-[16px] leading-6 text-white outline-none placeholder:text-zinc-600",
                                rows: "1",
                                placeholder: "Message agent…",
                                value: "{draft}",
                                oninput: move |event| draft.set(event.value()),
                            }
                            if is_streaming {
                                button {
                                    r#type: "button",
                                    class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-zinc-700 text-white active:scale-95",
                                    onclick: move |_| {
                                        let sid = cancel_sid.clone();
                                        spawn(async move {
                                            let _ = Request::post(&format!("/api/sessions/{sid}/cancel")).send().await;
                                        });
                                    },
                                    aria_label: "Stop",
                                    "■"
                                }
                            } else {
                                button {
                                    r#type: "submit",
                                    disabled: !can_send,
                                    class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-white font-bold text-black transition active:scale-95 disabled:cursor-not-allowed disabled:bg-zinc-800 disabled:text-zinc-600",
                                    aria_label: "Send",
                                    "↑"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RemoteHead() -> Element {
    rsx! {
        document::Title { "Vmux Remote" }
        document::Meta { name: "theme-color", content: "#0b0b0d" }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "black-translucent" }
        document::Link { rel: "manifest", href: "/manifest.webmanifest" }
        document::Link { rel: "apple-touch-icon", href: "/icon.png" }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PairScreenProps {
    token: String,
    error: String,
    on_token: EventHandler<String>,
    on_pair: EventHandler<()>,
}

#[component]
fn PairScreen(props: PairScreenProps) -> Element {
    rsx! {
        div { class: "flex h-dvh items-center justify-center bg-[#0b0b0d] px-5 text-zinc-100",
            div { class: "w-full max-w-sm rounded-3xl border border-white/10 bg-white/[0.045] p-6 shadow-2xl shadow-black/40",
                div { class: "mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-violet-500 to-cyan-400 text-xl font-bold text-white shadow-lg shadow-violet-500/20", "V" }
                h1 { class: "text-2xl font-semibold", "Pair with your Mac" }
                p { class: "mt-2 text-sm leading-6 text-zinc-500", "Run vmux remote on your Mac, then paste the pairing token." }
                form {
                    class: "mt-6 flex flex-col gap-3",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-12 rounded-xl border border-white/10 bg-black/30 px-4 font-mono text-sm outline-none focus:border-violet-400/50",
                        r#type: "password",
                        autocomplete: "one-time-code",
                        placeholder: "Pairing token",
                        value: "{props.token}",
                        oninput: move |event| props.on_token.call(event.value()),
                    }
                    if !props.error.is_empty() {
                        p { class: "text-sm text-red-300", "{props.error}" }
                    }
                    button { class: "h-12 rounded-xl bg-white font-semibold text-black active:scale-[0.99]", r#type: "submit", "Pair" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SessionListProps {
    sessions: Vec<RemoteSession>,
    selected_sid: String,
    on_select: EventHandler<RemoteSession>,
}

#[component]
fn SessionList(props: SessionListProps) -> Element {
    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-14 items-center border-b border-white/10 px-4",
                span { class: "text-sm font-semibold", "Sessions" }
                span { class: "ml-auto rounded-full bg-white/10 px-2 py-0.5 text-[10px] text-zinc-400", "{props.sessions.len()}" }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                if props.sessions.is_empty() {
                    p { class: "px-3 py-8 text-center text-sm leading-6 text-zinc-600", "No active agent chats. Start one in Vmux on your Mac." }
                }
                for session in props.sessions {
                    {
                        let selected = session.sid == props.selected_sid;
                        let selected_session = session.clone();
                        rsx! {
                            button {
                                key: "{session.sid}",
                                class: if selected { "mb-1 flex w-full items-center gap-3 rounded-2xl bg-white/10 p-3 text-left ring-1 ring-inset ring-white/10" } else { "mb-1 flex w-full items-center gap-3 rounded-2xl p-3 text-left hover:bg-white/[0.055]" },
                                onclick: move |_| props.on_select.call(selected_session.clone()),
                                div { class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-violet-500/30 to-cyan-400/20 text-sm font-semibold ring-1 ring-inset ring-white/10",
                                    {session.name.chars().next().unwrap_or('V').to_uppercase().to_string()}
                                }
                                div { class: "min-w-0 flex-1",
                                    div { class: "flex items-center gap-2",
                                        span { class: "truncate text-sm font-medium", "{session.name}" }
                                        span { class: status_dot(&session.status) }
                                    }
                                    div { class: "mt-0.5 truncate text-[11px] text-zinc-500",
                                        {session.model.clone().unwrap_or_else(|| session.runtime.clone())}
                                        " · "
                                        {cwd_name(&session.cwd)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EmptyStateProps {
    has_sessions: bool,
    on_open: EventHandler<()>,
}

#[component]
fn EmptyState(props: EmptyStateProps) -> Element {
    rsx! {
        div { class: "my-auto flex flex-col items-center gap-3 py-16 text-center",
            div { class: "flex h-16 w-16 items-center justify-center rounded-3xl bg-white/[0.055] text-2xl ring-1 ring-inset ring-white/10", "V" }
            h2 { class: "text-xl font-semibold", if props.has_sessions { "Pick a session" } else { "No active chats" } }
            p { class: "max-w-xs text-sm leading-6 text-zinc-500", if props.has_sessions { "Choose an agent chat from your Mac." } else { "Start an agent chat in Vmux. It will appear here." } }
            if props.has_sessions {
                button { class: "mt-2 rounded-xl bg-white px-4 py-2 text-sm font-semibold text-black md:hidden", onclick: move |_| props.on_open.call(()), "Open sessions" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ApprovalBarProps {
    approval: RemoteApproval,
    on_decide: EventHandler<(String, bool)>,
}

#[component]
fn ApprovalBar(props: ApprovalBarProps) -> Element {
    rsx! {
        div { class: "shrink-0 border-t border-amber-400/20 bg-amber-400/[0.07] px-3 py-3 sm:px-5",
            div { class: "mx-auto flex max-w-3xl items-center gap-3",
                div { class: "min-w-0 flex-1",
                    div { class: "text-sm font-medium text-amber-100", "Allow {props.approval.name}?" }
                    div { class: "mt-0.5 truncate font-mono text-[11px] text-amber-200/50", "{props.approval.args_json}" }
                }
                button {
                    class: "rounded-xl px-3 py-2 text-sm text-zinc-300 hover:bg-white/10",
                    onclick: {
                        let call_id = props.approval.call_id.clone();
                        move |_| props.on_decide.call((call_id.clone(), false))
                    },
                    "Deny"
                }
                button {
                    class: "rounded-xl bg-amber-200 px-3 py-2 text-sm font-semibold text-black active:scale-95",
                    onclick: {
                        let call_id = props.approval.call_id.clone();
                        move |_| props.on_decide.call((call_id.clone(), true))
                    },
                    "Allow"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MessageViewProps {
    message: Message,
}

#[component]
fn MessageView(props: MessageViewProps) -> Element {
    match props.message {
        Message::User { text, .. } => rsx! {
            div { class: "ml-8 self-end rounded-2xl rounded-br-md bg-white px-4 py-3 text-sm leading-6 text-black sm:ml-20",
                div { class: "whitespace-pre-wrap break-words", "{text}" }
            }
        },
        Message::Assistant { blocks } => rsx! {
            div { class: "mr-8 flex flex-col gap-2 sm:mr-20",
                for (index, block) in blocks.into_iter().enumerate() {
                    AssistantBlockView { key: "block-{index}", block }
                }
            }
        },
        Message::ToolResult {
            content, is_error, ..
        } => rsx! {
            details { class: if is_error { "rounded-xl border border-red-500/20 bg-red-500/[0.06] px-3 py-2 text-xs text-red-200" } else { "rounded-xl border border-white/10 bg-white/[0.035] px-3 py-2 text-xs text-zinc-400" },
                summary { class: "cursor-pointer select-none font-medium", if is_error { "Tool error" } else { "Tool result" } }
                pre { class: "mt-2 overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-5", "{content}" }
            }
        },
    }
}

#[derive(Props, Clone, PartialEq)]
struct AssistantBlockViewProps {
    block: AssistantBlock,
}

#[component]
fn AssistantBlockView(props: AssistantBlockViewProps) -> Element {
    match props.block {
        AssistantBlock::Text(text) => rsx! {
            div { class: "rounded-2xl rounded-bl-md border border-white/10 bg-white/[0.04] px-4 py-3 text-sm leading-6 text-zinc-200",
                div { class: "whitespace-pre-wrap break-words", "{text}" }
            }
        },
        AssistantBlock::Thinking(text) => rsx! {
            details { class: "rounded-xl border border-white/10 bg-white/[0.025] px-3 py-2 text-xs text-zinc-500",
                summary { class: "cursor-pointer select-none font-medium text-zinc-400", "Thinking" }
                div { class: "mt-2 whitespace-pre-wrap break-words leading-5", "{text}" }
            }
        },
        AssistantBlock::ToolUse { name, args, .. } => rsx! {
            details { class: "rounded-xl border border-white/10 bg-black/20 px-3 py-2 text-xs text-zinc-400",
                summary { class: "cursor-pointer select-none font-mono font-medium text-cyan-300/80", "{name}" }
                pre { class: "mt-2 overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-5", "{args}" }
            }
        },
        AssistantBlock::Diff {
            path,
            old_text,
            new_text,
            ..
        } => rsx! {
            details { class: "rounded-xl border border-violet-400/20 bg-violet-400/[0.055] px-3 py-2 text-xs text-zinc-400",
                summary { class: "cursor-pointer select-none font-mono text-violet-200", "Edited {path}" }
                if let Some(old) = old_text {
                    pre { class: "mt-2 overflow-x-auto whitespace-pre-wrap break-words bg-red-500/[0.06] p-2 font-mono text-[11px] text-red-200/70", "{old}" }
                }
                pre { class: "mt-2 overflow-x-auto whitespace-pre-wrap break-words bg-emerald-500/[0.06] p-2 font-mono text-[11px] text-emerald-100/70", "{new_text}" }
            }
        },
        AssistantBlock::Plan { steps } => rsx! { PlanView { steps } },
        AssistantBlock::Subagent(subagent) => rsx! {
            details { class: "rounded-xl border border-cyan-400/20 bg-cyan-400/[0.045] px-3 py-2 text-xs text-zinc-400",
                summary { class: "cursor-pointer select-none font-medium text-cyan-100", "{subagent.title} · {subagent.status}" }
                if let Some(prompt) = subagent.prompt {
                    div { class: "mt-2 whitespace-pre-wrap break-words leading-5", "{prompt}" }
                }
            }
        },
    }
}

#[derive(Props, Clone, PartialEq)]
struct PlanViewProps {
    steps: Vec<PlanStep>,
}

#[component]
fn PlanView(props: PlanViewProps) -> Element {
    rsx! {
        div { class: "rounded-xl border border-white/10 bg-white/[0.025] px-3 py-3",
            div { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-500", "Plan" }
            div { class: "flex flex-col gap-2",
                for step in props.steps {
                    div { class: "flex items-start gap-2 text-xs leading-5 text-zinc-400",
                        span { class: if step.status == "completed" { "text-emerald-400" } else if step.status == "in_progress" { "text-violet-300" } else { "text-zinc-600" },
                            if step.status == "completed" { "✓" } else if step.status == "in_progress" { "●" } else { "○" }
                        }
                        span { "{step.content}" }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn open_session(
    session: RemoteSession,
    mut current: Signal<Option<RemoteSession>>,
    mut messages: Signal<Vec<Message>>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
    mut connected: Signal<bool>,
    mut drawer: Signal<bool>,
    mut generation: Signal<u64>,
) {
    let sid = session.sid.clone();
    current.set(Some(session.clone()));
    messages.set(Vec::new());
    live_delta.set(String::new());
    status.set(session.status.clone());
    approval.set(session.approval.clone());
    connected.set(false);
    drawer.set(false);
    let next_generation = generation().wrapping_add(1);
    generation.set(next_generation);
    spawn(async move {
        let Ok(mut source) = EventSource::new(&format!("/api/sessions/{sid}/events")) else {
            return;
        };
        let Ok(mut events) = source.subscribe("message") else {
            return;
        };
        connected.set(true);
        while let Some(event) = events.next().await {
            if generation() != next_generation {
                break;
            }
            let Ok((_, event)) = event else {
                connected.set(false);
                continue;
            };
            let Some(data) = event.data().as_string() else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<RemoteEvent>(&data) else {
                continue;
            };
            match event {
                RemoteEvent::Session { session } => {
                    status.set(session.status.clone());
                    approval.set(session.approval.clone());
                    current.set(Some(session));
                }
                RemoteEvent::Snapshot { messages: next } => {
                    messages.set(next);
                    live_delta.set(String::new());
                }
                RemoteEvent::Delta { text } => live_delta.write().push_str(&text),
                RemoteEvent::Status { status: next } => {
                    if !matches!(next, RemoteStatus::Streaming) {
                        approval.set(None);
                    }
                    status.set(next);
                }
                RemoteEvent::Approval { approval: next } => approval.set(next),
            }
        }
        connected.set(false);
        source.close();
    });
}

async fn fetch_sessions() -> Result<Vec<RemoteSession>, ApiError> {
    let response = Request::get("/api/sessions")
        .send()
        .await
        .map_err(|error| ApiError::Message(error.to_string()))?;
    if response.status() == 401 {
        return Err(ApiError::Unauthorized);
    }
    if !response.ok() {
        return Err(ApiError::Message(format!(
            "Mac returned HTTP {}.",
            response.status()
        )));
    }
    response
        .json()
        .await
        .map_err(|error| ApiError::Message(error.to_string()))
}

async fn pair(token: &str) -> Result<(), String> {
    let request = Request::post("/api/pair")
        .json(&PairRequest {
            token: token.to_string(),
        })
        .map_err(|error| error.to_string())?;
    let response = request.send().await.map_err(|error| error.to_string())?;
    if response.ok() {
        Ok(())
    } else {
        Err("Pairing token was rejected.".to_string())
    }
}

async fn post_json<T: serde::Serialize>(path: &str, body: &T) -> Result<(), ApiError> {
    let request = Request::post(path)
        .json(body)
        .map_err(|error| ApiError::Message(error.to_string()))?;
    let response = request
        .send()
        .await
        .map_err(|error| ApiError::Message(error.to_string()))?;
    if response.status() == 401 {
        Err(ApiError::Unauthorized)
    } else if response.ok() {
        Ok(())
    } else {
        Err(ApiError::Message(format!(
            "Mac returned HTTP {}.",
            response.status()
        )))
    }
}

fn token_from_hash() -> Option<String> {
    web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .and_then(|hash| hash.strip_prefix("#token=").map(str::to_string))
        .filter(|token| !token.is_empty())
}

fn clear_hash() {
    if let Some(window) = web_sys::window()
        && let Ok(history) = window.history()
    {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some("/"));
    }
}

fn register_service_worker() {
    if let Some(window) = web_sys::window()
        && window.location().protocol().ok().as_deref() != Some("vmux:")
    {
        let _ = window.navigator().service_worker().register("/sw.js");
    }
}

fn cwd_name(cwd: &str) -> String {
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

fn status_dot(status: &RemoteStatus) -> &'static str {
    match status {
        RemoteStatus::Streaming => {
            "h-2 w-2 shrink-0 animate-pulse rounded-full bg-violet-400 shadow-[0_0_8px_rgba(167,139,250,0.8)]"
        }
        RemoteStatus::Errored(_) => "h-2 w-2 shrink-0 rounded-full bg-red-400",
        RemoteStatus::Interrupted => "h-2 w-2 shrink-0 rounded-full bg-amber-400",
        RemoteStatus::Idle => "h-2 w-2 shrink-0 rounded-full bg-emerald-400",
    }
}
