#![allow(non_snake_case)]

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use dioxus::prelude::*;
use futures_util::StreamExt;
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;
use vmux_chat_ui::{
    AssistantTurn, DiffBlock, PlanBlock, PlanItem, SubagentActivity, TextBlock, ThinkingBlock,
    ToolResultBlock, ToolUseBlock, UserBubble,
};
use vmux_remote::{
    ApprovalRequest, AssistantBlock, Message, NewChatRequest, PromptRequest, RemoteApproval,
    RemoteEvent, RemoteSession, RemoteStatus,
};

const STORAGE_KEY: &str = "vmux.remote.credentials";
const MAX_SSE_BUFFER: usize = 2 * 1024 * 1024;
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.out.css");
static OPENED_URLS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn main() {
    let config = dioxus::mobile::Config::new().with_custom_event_handler(|event, _| {
        if let dioxus::mobile::tao::event::Event::Opened { urls } = event {
            let mut opened = OPENED_URLS
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            opened.extend(
                urls.iter()
                    .filter(|url| url.scheme() == "vmuxremote")
                    .map(ToString::to_string),
            );
        }
    });
    dioxus::LaunchBuilder::mobile().with_cfg(config).launch(App);
}

#[derive(Clone, Copy, PartialEq)]
enum AuthState {
    Loading,
    Paired,
    Unpaired,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Credentials {
    base_url: String,
    token: String,
}

#[derive(Clone)]
struct Api {
    client: Client,
    credentials: Credentials,
}

enum ApiError {
    Unauthorized,
    Message(String),
}

impl Api {
    fn new(credentials: Credentials) -> Self {
        Self {
            client: Client::new(),
            credentials,
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.credentials.base_url.trim_end_matches('/'))
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, self.endpoint(path))
            .bearer_auth(&self.credentials.token)
    }

    async fn sessions(&self) -> Result<Vec<RemoteSession>, ApiError> {
        let response = self
            .request(Method::GET, "/api/sessions")
            .send()
            .await
            .map_err(|error| ApiError::Message(error.to_string()))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        if !response.status().is_success() {
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

    async fn post_json<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> Result<(), ApiError> {
        let response = self
            .request(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|error| ApiError::Message(error.to_string()))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            Err(ApiError::Unauthorized)
        } else if response.status().is_success() {
            Ok(())
        } else {
            Err(ApiError::Message(format!(
                "Mac returned HTTP {}.",
                response.status()
            )))
        }
    }

    async fn events(&self, sid: &str) -> Result<reqwest::Response, ApiError> {
        let response = self
            .request(Method::GET, &format!("/api/sessions/{sid}/events"))
            .send()
            .await
            .map_err(|error| ApiError::Message(error.to_string()))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            Err(ApiError::Unauthorized)
        } else if response.status().is_success() {
            Ok(response)
        } else {
            Err(ApiError::Message(format!(
                "Mac returned HTTP {}.",
                response.status()
            )))
        }
    }
}

#[component]
fn App() -> Element {
    let mut auth = use_signal(|| AuthState::Loading);
    let mut pair_url = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut api = use_signal(|| None::<Api>);
    let mut sessions = use_signal(Vec::<RemoteSession>::new);
    let mut current = use_signal(|| None::<RemoteSession>);
    let mut messages = use_signal(Vec::<Message>::new);
    let mut live_delta = use_signal(String::new);
    let mut status = use_signal(|| RemoteStatus::Idle);
    let mut approval = use_signal(|| None::<RemoteApproval>);
    let mut draft = use_signal(String::new);
    let connected = use_signal(|| false);
    let mut drawer = use_signal(|| false);
    let mut stream_generation = use_signal(|| 0_u64);
    let mut pending_pair_url = use_signal(|| None::<String>);
    let mut deep_link_received = use_signal(|| false);
    let mut pairing = use_signal(|| false);
    let mut new_chat_open = use_signal(|| false);
    let mut new_chat_draft = use_signal(String::new);
    let mut new_chat_error = use_signal(String::new);
    let mut creating_chat = use_signal(|| false);

    use_effect(move || {
        let _ = messages.read().len();
        let _ = live_delta.read().len();
        let _ = document::eval(
            "const el = document.getElementById('remote-chat-scroll'); if (el) el.scrollTop = el.scrollHeight;",
        );
    });

    use_future(move || async move {
        if let Some(opened) = take_opened_url() {
            deep_link_received.set(true);
            pair_url.set(opened.clone());
            pending_pair_url.set(Some(opened));
            auth.set(AuthState::Unpaired);
            return;
        }
        let Some(credentials) = load_credentials().await else {
            if deep_link_received() {
                return;
            }
            auth.set(AuthState::Unpaired);
            return;
        };
        if deep_link_received() {
            return;
        }
        pair_url.set(pairing_url(&credentials));
        let client = Api::new(credentials);
        match client.sessions().await {
            Ok(next) => {
                api.set(Some(client.clone()));
                sessions.set(next.clone());
                auth.set(AuthState::Paired);
                if let Some(first) = next.first().cloned() {
                    open_session(
                        client,
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
                clear_credentials();
                error.set("Pairing expired. Scan the QR on your Mac again.".to_string());
                auth.set(AuthState::Unpaired);
            }
            Err(ApiError::Message(message)) => {
                error.set(message);
                auth.set(AuthState::Unpaired);
            }
        }
    });

    use_future(move || async move {
        loop {
            if let Some(opened) = take_opened_url() {
                deep_link_received.set(true);
                pair_url.set(opened.clone());
                pending_pair_url.set(Some(opened));
                error.set(String::new());
                auth.set(AuthState::Unpaired);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let pending = pending_pair_url.write().take();
            let Some(input) = pending else {
                continue;
            };
            let credentials = match parse_pairing_url(&input) {
                Ok(credentials) => credentials,
                Err(message) => {
                    pairing.set(false);
                    error.set(message);
                    auth.set(AuthState::Unpaired);
                    continue;
                }
            };
            pairing.set(true);
            error.set(String::new());
            let client = Api::new(credentials.clone());
            match client.sessions().await {
                Ok(next) => {
                    save_credentials(&credentials);
                    pair_url.set(pairing_url(&credentials));
                    api.set(Some(client.clone()));
                    sessions.set(next.clone());
                    auth.set(AuthState::Paired);
                    if let Some(first) = next.first().cloned() {
                        open_session(
                            client,
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
                    error.set("Pairing token was rejected.".to_string());
                    auth.set(AuthState::Unpaired);
                }
                Err(ApiError::Message(message)) => {
                    error.set(message);
                    auth.set(AuthState::Unpaired);
                }
            }
            pairing.set(false);
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if auth() != AuthState::Paired {
                continue;
            }
            let Some(client) = api() else {
                continue;
            };
            match client.sessions().await {
                Ok(next) => {
                    sessions.set(next.clone());
                    if current().is_none()
                        && let Some(first) = next.first().cloned()
                    {
                        open_session(
                            client,
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
                    clear_credentials();
                    api.set(None);
                    error.set("Pairing expired. Scan the QR on your Mac again.".to_string());
                    auth.set(AuthState::Unpaired);
                }
                Err(ApiError::Message(_)) => {}
            }
        }
    });

    if auth() == AuthState::Loading {
        return rsx! {
            AppHead {}
            div { class: "flex h-dvh items-center justify-center bg-zinc-950 text-white",
                div { class: "h-8 w-8 animate-spin rounded-full border-2 border-white/20 border-t-white" }
            }
        };
    }

    if auth() == AuthState::Unpaired {
        return rsx! {
            AppHead {}
            PairScreen {
                value: pair_url(),
                error: error(),
                pairing: pairing(),
                on_value: move |value| pair_url.set(value),
                on_pair: move |_| {
                    pending_pair_url.set(Some(pair_url()));
                },
            }
        };
    }

    let current_value = current();
    let selected_sid = current_value
        .as_ref()
        .map(|session| session.sid.clone())
        .unwrap_or_default();
    let is_streaming = matches!(status(), RemoteStatus::Streaming);
    let can_send = current_value.is_some() && !draft().trim().is_empty();
    let submit_sid = selected_sid.clone();
    let cancel_sid = selected_sid.clone();
    let approval_sid = selected_sid.clone();
    let approval_value = approval();

    rsx! {
        AppHead {}
        div { class: "flex h-dvh min-h-0 flex-col bg-zinc-950 text-zinc-100",
            header { class: "flex shrink-0 items-center gap-3 border-b border-white/10 bg-zinc-950/95 px-3 pb-2 pt-[calc(0.5rem+env(safe-area-inset-top))] backdrop-blur-xl",
                button {
                    class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-white/[0.06] text-lg text-zinc-300 active:bg-white/10",
                    onclick: move |_| drawer.set(true),
                    aria_label: "Open sessions",
                    "☰"
                }
                div { class: "min-w-0 flex-1",
                    if let Some(session) = current_value.as_ref() {
                        div { class: "truncate text-sm font-semibold", "{session.name}" }
                        div { class: "mt-1 flex items-center gap-1.5 truncate text-[11px] text-zinc-500",
                            span { class: status_dot(&status()) }
                            span { "{session.runtime}" }
                            if let Some(model) = session.model.as_ref() {
                                span { "· {model}" }
                            }
                            span { "· {cwd_name(&session.cwd)}" }
                        }
                    } else {
                        div { class: "text-sm font-semibold", "Vmux Remote" }
                        div { class: "mt-1 text-[11px] text-zinc-500", "No active session" }
                    }
                }
                div { class: if connected() { "h-2 w-2 rounded-full bg-emerald-400" } else { "h-2 w-2 rounded-full bg-zinc-700" } }
            }

            main { id: "remote-chat-scroll", class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-5",
                if messages().is_empty() && live_delta().is_empty() {
                    div { class: "flex h-full items-center justify-center px-8 text-center text-sm leading-6 text-zinc-600",
                        if current_value.is_some() { "No messages yet." } else { "Open the menu to start a new chat." }
                    }
                }
                div { class: "mx-auto flex w-full max-w-3xl flex-col",
                    for (index, item) in group_messages(messages()).into_iter().enumerate() {
                        MessageView { key: "{index}", item }
                    }
                    if !live_delta().is_empty() {
                        div { class: "mb-4 flex flex-col",
                            AssistantTurn {
                                TextBlock { text: live_delta() }
                                span { class: "ml-0.5 h-3.5 w-1.5 animate-pulse bg-violet-400" }
                            }
                        }
                    }
                    if let RemoteStatus::Errored(message) = status() {
                        div { class: "mb-4 rounded-xl border border-red-400/20 bg-red-400/[0.06] px-3 py-2 text-xs text-red-200", "{message}" }
                    }
                }
            }

            if let Some(pending) = approval_value {
                div { class: "shrink-0 border-t border-amber-300/10 bg-amber-300/[0.04] px-3 py-3",
                    div { class: "mx-auto max-w-3xl rounded-2xl border border-amber-300/20 bg-amber-300/[0.04] p-3",
                        div { class: "text-sm font-semibold text-amber-100", "Allow {pending.name}?" }
                        pre { class: "mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-zinc-500", "{pending.args_json}" }
                        div { class: "mt-3 flex gap-2",
                            button {
                                class: "h-10 flex-1 rounded-xl bg-white font-semibold text-black active:scale-[0.99]",
                                onclick: {
                                    let call_id = pending.call_id.clone();
                                    let sid = approval_sid.clone();
                                    move |_| {
                                        let Some(client) = api() else { return };
                                        approval.set(None);
                                        let call_id = call_id.clone();
                                        let sid = sid.clone();
                                        spawn(async move {
                                            let _ = client.post_json(
                                                &format!("/api/sessions/{sid}/approval"),
                                                &ApprovalRequest { call_id, allow: true },
                                            ).await;
                                        });
                                    }
                                },
                                "Allow"
                            }
                            button {
                                class: "h-10 flex-1 rounded-xl bg-white/10 font-semibold text-zinc-200 active:scale-[0.99]",
                                onclick: {
                                    let call_id = pending.call_id.clone();
                                    let sid = approval_sid.clone();
                                    move |_| {
                                        let Some(client) = api() else { return };
                                        approval.set(None);
                                        let call_id = call_id.clone();
                                        let sid = sid.clone();
                                        spawn(async move {
                                            let _ = client.post_json(
                                                &format!("/api/sessions/{sid}/approval"),
                                                &ApprovalRequest { call_id, allow: false },
                                            ).await;
                                        });
                                    }
                                },
                                "Deny"
                            }
                        }
                    }
                }
            }

            form {
                class: "shrink-0 border-t border-white/10 bg-zinc-950/95 px-2.5 pb-[calc(0.625rem+env(safe-area-inset-bottom))] pt-2.5 backdrop-blur-xl",
                onsubmit: move |event| {
                    event.prevent_default();
                    let text = draft().trim().to_string();
                    let Some(client) = api() else { return };
                    if text.is_empty() || submit_sid.is_empty() {
                        return;
                    }
                    draft.set(String::new());
                    status.set(RemoteStatus::Streaming);
                    let sid = submit_sid.clone();
                    spawn(async move {
                        if let Err(ApiError::Message(message)) = client.post_json(
                            &format!("/api/sessions/{sid}/messages"),
                            &PromptRequest { text },
                        ).await {
                            status.set(RemoteStatus::Errored(message));
                        }
                    });
                },
                div { class: "mx-auto flex max-w-3xl items-end gap-2",
                    textarea {
                        class: "min-h-12 max-h-32 flex-1 resize-none rounded-2xl border border-white/10 bg-black/30 px-3.5 py-3 text-base leading-5 text-white outline-none placeholder:text-zinc-600 focus:border-violet-400/60",
                        rows: "1",
                        placeholder: if current_value.is_some() { "Message agent…" } else { "No active session" },
                        disabled: current_value.is_none(),
                        value: "{draft}",
                        oninput: move |event| draft.set(event.value()),
                    }
                    if is_streaming {
                        button {
                            class: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-zinc-800 text-lg active:scale-95",
                            r#type: "button",
                            onclick: move |_| {
                                let Some(client) = api() else { return };
                                let sid = cancel_sid.clone();
                                spawn(async move {
                                    let _ = client.post_json(&format!("/api/sessions/{sid}/cancel"), &serde_json::json!({})).await;
                                });
                            },
                            "■"
                        }
                    } else {
                        button {
                            class: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white text-xl font-bold text-black active:scale-95 disabled:opacity-30",
                            r#type: "submit",
                            disabled: !can_send,
                            "↑"
                        }
                    }
                }
            }
        }

        if drawer() {
            div {
                class: "fixed inset-0 z-50 bg-black/65 backdrop-blur-sm",
                onclick: move |_| drawer.set(false),
                aside {
                    class: "flex h-full w-[88%] max-w-sm flex-col border-r border-white/10 bg-[#111114] pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)] shadow-2xl",
                    onclick: move |event| event.stop_propagation(),
                    div { class: "flex h-14 shrink-0 items-center border-b border-white/10 px-4",
                        span { class: "text-sm font-semibold", "Sessions" }
                        span { class: "ml-2 rounded-full bg-white/10 px-2 py-0.5 text-[10px] text-zinc-400", "{sessions().len()}" }
                        button {
                            class: "ml-auto rounded-lg px-2 py-1 text-xs text-zinc-500 active:bg-white/10",
                            onclick: move |_| {
                                clear_credentials();
                                let next = stream_generation().wrapping_add(1);
                                stream_generation.set(next);
                                api.set(None);
                                sessions.set(Vec::new());
                                current.set(None);
                                messages.set(Vec::new());
                                live_delta.set(String::new());
                                drawer.set(false);
                                auth.set(AuthState::Unpaired);
                            },
                            "Disconnect"
                        }
                    }
                    button {
                        class: "mx-2 mt-2 flex h-11 shrink-0 items-center justify-center rounded-xl bg-white text-sm font-semibold text-black active:scale-[0.99]",
                        r#type: "button",
                        onclick: move |_| {
                            new_chat_error.set(String::new());
                            new_chat_open.set(true);
                        },
                        "+ New chat"
                    }
                    if new_chat_open() {
                        form {
                            class: "mx-2 mt-2 rounded-2xl border border-white/10 bg-black/20 p-3",
                            onsubmit: move |event| {
                                event.prevent_default();
                                let text = new_chat_draft().trim().to_string();
                                let Some(client) = api() else { return };
                                if text.is_empty() || creating_chat() {
                                    return;
                                }
                                let known = sessions()
                                    .into_iter()
                                    .map(|session| session.sid)
                                    .collect::<std::collections::HashSet<_>>();
                                creating_chat.set(true);
                                new_chat_error.set(String::new());
                                spawn(async move {
                                    match client.post_json("/api/chats", &NewChatRequest { text }).await {
                                        Ok(()) => {
                                            for _ in 0..40 {
                                                tokio::time::sleep(Duration::from_millis(250)).await;
                                                if let Ok(next) = client.sessions().await {
                                                    let created = next
                                                        .iter()
                                                        .find(|session| !known.contains(&session.sid))
                                                        .cloned();
                                                    sessions.set(next);
                                                    if let Some(created) = created {
                                                        new_chat_draft.set(String::new());
                                                        new_chat_open.set(false);
                                                        creating_chat.set(false);
                                                        open_session(
                                                            client,
                                                            created,
                                                            current,
                                                            messages,
                                                            live_delta,
                                                            status,
                                                            approval,
                                                            connected,
                                                            drawer,
                                                            stream_generation,
                                                        );
                                                        return;
                                                    }
                                                }
                                            }
                                            new_chat_error.set("The desktop opened the chat, but its session did not appear.".to_string());
                                        }
                                        Err(ApiError::Unauthorized) => {
                                            new_chat_error.set("Pairing expired. Pair with the Mac again.".to_string());
                                        }
                                        Err(ApiError::Message(message)) => new_chat_error.set(message),
                                    }
                                    creating_chat.set(false);
                                });
                            },
                            textarea {
                                class: "min-h-24 w-full resize-none rounded-xl border border-white/10 bg-zinc-950 px-3 py-3 text-base leading-5 text-white outline-none placeholder:text-zinc-600 focus:border-violet-400/60",
                                rows: "3",
                                autofocus: true,
                                placeholder: "What should the agent do?",
                                value: "{new_chat_draft}",
                                oninput: move |event| new_chat_draft.set(event.value()),
                            }
                            if !new_chat_error().is_empty() {
                                div { class: "mt-2 text-xs leading-5 text-red-300", "{new_chat_error}" }
                            }
                            div { class: "mt-3 flex gap-2",
                                button {
                                    class: "h-10 flex-1 rounded-xl bg-white/10 text-sm font-semibold text-zinc-300 active:scale-[0.99]",
                                    r#type: "button",
                                    disabled: creating_chat(),
                                    onclick: move |_| new_chat_open.set(false),
                                    "Cancel"
                                }
                                button {
                                    class: "h-10 flex-1 rounded-xl bg-white text-sm font-semibold text-black active:scale-[0.99] disabled:opacity-40",
                                    r#type: "submit",
                                    disabled: creating_chat() || new_chat_draft().trim().is_empty(),
                                    if creating_chat() { "Starting…" } else { "Start chat" }
                                }
                            }
                        }
                    }
                    div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                        if sessions().is_empty() {
                            div { class: "px-3 py-8 text-center text-sm text-zinc-600", "No active sessions" }
                        }
                        for session in sessions() {
                            button {
                                key: "{session.sid}",
                                class: if session.sid == selected_sid { "mb-1 block w-full rounded-xl bg-white/[0.08] px-3 py-3 text-left" } else { "mb-1 block w-full rounded-xl px-3 py-3 text-left active:bg-white/[0.06]" },
                                onclick: {
                                    let next = session.clone();
                                    move |_| {
                                        let Some(client) = api() else { return };
                                        open_session(
                                            client,
                                            next.clone(),
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
                                },
                                div { class: "flex items-center gap-2",
                                    span { class: status_dot(&session.status) }
                                    span { class: "min-w-0 flex-1 truncate text-sm font-medium", "{session.name}" }
                                }
                                div { class: "mt-1.5 truncate pl-3.5 text-[11px] text-zinc-600", "{session.runtime} · {cwd_name(&session.cwd)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AppHead() -> Element {
    rsx! {
        document::Title { "Vmux Remote" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Meta { name: "theme-color", content: "#09090b" }
        document::Stylesheet { href: TAILWIND_CSS }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PairScreenProps {
    value: String,
    error: String,
    pairing: bool,
    on_value: EventHandler<String>,
    on_pair: EventHandler<()>,
}

#[component]
fn PairScreen(props: PairScreenProps) -> Element {
    rsx! {
        div { class: "flex h-dvh items-center justify-center bg-[radial-gradient(circle_at_10%_0%,rgba(124,58,237,0.2),transparent_38%),radial-gradient(circle_at_90%_100%,rgba(6,182,212,0.12),transparent_35%),#09090b] px-5 pb-[calc(1.5rem+env(safe-area-inset-bottom))] pt-[calc(1.5rem+env(safe-area-inset-top))] text-zinc-100",
            div { class: "w-full max-w-sm rounded-3xl border border-white/10 bg-zinc-900/90 p-6 shadow-2xl shadow-black/50",
                div { class: "mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-violet-500 to-cyan-400 text-xl font-bold text-white shadow-lg shadow-violet-500/20", "V" }
                h1 { class: "text-2xl font-semibold tracking-tight", "Pair with your Mac" }
                p { class: "mt-2 text-sm leading-6 text-zinc-500", "Enable Remote on your Mac and scan its QR code. You can also paste the pairing URL." }
                form {
                    class: "mt-6 flex flex-col gap-3",
                    onsubmit: move |event| {
                        event.prevent_default();
                        props.on_pair.call(());
                    },
                    input {
                        class: "h-14 rounded-xl border border-white/10 bg-black/30 px-4 font-mono text-base text-white outline-none placeholder:text-zinc-600 focus:border-violet-400/60",
                        r#type: "url",
                        inputmode: "url",
                        autocomplete: "off",
                        autocapitalize: "none",
                        placeholder: "http://127.0.0.1:54821/#token=…",
                        value: "{props.value}",
                        oninput: move |event| props.on_value.call(event.value()),
                    }
                    if !props.error.is_empty() {
                        p { class: "text-sm leading-5 text-red-300", "{props.error}" }
                    }
                    button {
                        class: "h-13 rounded-xl bg-white font-semibold text-black disabled:opacity-50 active:scale-[0.99]",
                        r#type: "submit",
                        disabled: props.pairing,
                        if props.pairing { "Pairing…" } else { "Pair" }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum MobileChatItem {
    User { text: String },
    Turn { blocks: Vec<MobileChatBlock> },
}

#[derive(Clone, PartialEq)]
enum MobileChatBlock {
    Assistant(AssistantBlock),
    ToolResult { content: String, is_error: bool },
}

#[derive(Props, Clone, PartialEq)]
struct MessageViewProps {
    item: MobileChatItem,
}

#[component]
fn MessageView(props: MessageViewProps) -> Element {
    match props.item {
        MobileChatItem::User { text } => rsx! {
            div { class: "mb-4 flex flex-col",
                UserBubble {
                    div { class: "whitespace-pre-wrap break-words", "{text}" }
                }
            }
        },
        MobileChatItem::Turn { blocks } => rsx! {
            div { class: "mb-4 flex flex-col",
                AssistantTurn {
                    for (index, block) in blocks.into_iter().enumerate() {
                        match block {
                            MobileChatBlock::Assistant(block) => rsx! {
                                AssistantBlockView { key: "{index}", block }
                            },
                            MobileChatBlock::ToolResult { content, is_error } => rsx! {
                                ToolResultBlock { key: "{index}", content, is_error }
                            },
                        }
                    }
                }
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
        AssistantBlock::Text(text) => rsx! { TextBlock { text } },
        AssistantBlock::Thinking(text) => rsx! { ThinkingBlock { text } },
        AssistantBlock::ToolUse { name, args, .. } => rsx! { ToolUseBlock { name, args } },
        AssistantBlock::Diff {
            path,
            old_text,
            new_text,
            ..
        } => rsx! { DiffBlock { path, old_text, new_text } },
        AssistantBlock::Plan { steps } => rsx! {
            PlanBlock {
                steps: steps.into_iter().map(|step| PlanItem {
                    content: step.content,
                    status: step.status,
                }).collect()
            }
        },
        AssistantBlock::Subagent(subagent) => rsx! {
            SubagentActivity {
                title: subagent.title,
                status: subagent.status,
                provider: subagent.provider,
                action: subagent.action,
                agent_name: subagent.agent_name,
                model: subagent.model,
                reasoning_effort: subagent.reasoning_effort,
                prompt: subagent.prompt,
                thread_id: subagent.thread_id,
                parent_thread_id: subagent.parent_thread_id,
                child_thread_ids: subagent.child_thread_ids,
                call_id: subagent.call_id,
                raw_input: subagent.raw_input,
            }
        },
    }
}

fn group_messages(messages: Vec<Message>) -> Vec<MobileChatItem> {
    let mut items = Vec::new();
    let mut turn = Vec::new();
    for message in messages {
        match message {
            Message::User { text, .. } => {
                if !turn.is_empty() {
                    items.push(MobileChatItem::Turn {
                        blocks: std::mem::take(&mut turn),
                    });
                }
                items.push(MobileChatItem::User { text });
            }
            Message::Assistant { blocks } => {
                turn.extend(blocks.into_iter().map(MobileChatBlock::Assistant))
            }
            Message::ToolResult {
                content, is_error, ..
            } => turn.push(MobileChatBlock::ToolResult { content, is_error }),
        }
    }
    if !turn.is_empty() {
        items.push(MobileChatItem::Turn { blocks: turn });
    }
    items
}

#[allow(clippy::too_many_arguments)]
fn open_session(
    api: Api,
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
        loop {
            if generation() != next_generation {
                return;
            }
            let response = match api.events(&sid).await {
                Ok(response) => response,
                Err(ApiError::Unauthorized) => return,
                Err(ApiError::Message(_)) => {
                    connected.set(false);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            connected.set(true);
            let mut chunks = response.bytes_stream();
            let mut buffer = Vec::new();
            while let Some(chunk) = chunks.next().await {
                if generation() != next_generation {
                    return;
                }
                let Ok(chunk) = chunk else {
                    break;
                };
                buffer.extend_from_slice(&chunk);
                if buffer.len() > MAX_SSE_BUFFER {
                    break;
                }
                while let Some(frame) = take_sse_frame(&mut buffer) {
                    let Some(event) = parse_sse_event(&frame) else {
                        continue;
                    };
                    apply_remote_event(event, current, messages, live_delta, status, approval);
                }
            }
            connected.set(false);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

fn apply_remote_event(
    event: RemoteEvent,
    mut current: Signal<Option<RemoteSession>>,
    mut messages: Signal<Vec<Message>>,
    mut live_delta: Signal<String>,
    mut status: Signal<RemoteStatus>,
    mut approval: Signal<Option<RemoteApproval>>,
) {
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

fn take_sse_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let delimiter = match (crlf, lf) {
        (Some(crlf), Some(lf)) => Some(if crlf.0 < lf.0 { crlf } else { lf }),
        (Some(delimiter), None) | (None, Some(delimiter)) => Some(delimiter),
        (None, None) => None,
    }?;
    let frame = buffer[..delimiter.0].to_vec();
    buffer.drain(..delimiter.0 + delimiter.1);
    Some(frame)
}

fn parse_sse_event(frame: &[u8]) -> Option<RemoteEvent> {
    let text = std::str::from_utf8(frame).ok()?;
    let data = text
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        None
    } else {
        serde_json::from_str(&data).ok()
    }
}

fn parse_pairing_url(input: &str) -> Result<Credentials, String> {
    let input = input.trim();
    if input.starts_with("vmuxremote://") {
        let parsed = Url::parse(input).map_err(|_| "Pairing URL is invalid.".to_string())?;
        if parsed.scheme() != "vmuxremote" || parsed.host_str() != Some("pair") {
            return Err("Pairing URL is invalid.".to_string());
        }
        let params = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        let base_url = params
            .get("base")
            .map(|value| value.to_string())
            .ok_or_else(|| "Pairing URL has no server address.".to_string())?;
        let token = params
            .get("token")
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Pairing URL has no token.".to_string())?;
        let base = Url::parse(&base_url)
            .map_err(|_| "Pairing URL has an invalid server address.".to_string())?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err("Pairing URL must use HTTPS or HTTP.".to_string());
        }
        let base_url = base.origin().ascii_serialization();
        if base_url == "null" {
            return Err("Pairing URL has no server address.".to_string());
        }
        return Ok(Credentials { base_url, token });
    }
    let start = input
        .find("https://")
        .or_else(|| input.find("http://"))
        .ok_or_else(|| "Paste the full pairing URL shown by Vmux on your Mac.".to_string())?;
    let candidate = input[start..].split_whitespace().next().unwrap_or_default();
    let parsed = Url::parse(candidate).map_err(|_| "Pairing URL is invalid.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Pairing URL must use HTTPS or HTTP.".to_string());
    }
    let token = parsed
        .fragment()
        .and_then(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes())
                .find(|(name, _)| name == "token")
                .map(|(_, value)| value.into_owned())
        })
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "Pairing URL has no token.".to_string())?;
    let base_url = parsed.origin().ascii_serialization();
    if base_url == "null" {
        return Err("Pairing URL has no server address.".to_string());
    }
    Ok(Credentials { base_url, token })
}

fn take_opened_url() -> Option<String> {
    OPENED_URLS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .pop()
}

fn pairing_url(credentials: &Credentials) -> String {
    format!("{}/#token={}", credentials.base_url, credentials.token)
}

async fn load_credentials() -> Option<Credentials> {
    let mut evaluator = document::eval(&format!(
        "dioxus.send(window.localStorage.getItem({}));",
        serde_json::to_string(STORAGE_KEY).ok()?
    ));
    let value: Option<String> = evaluator.recv().await.ok()?;
    serde_json::from_str(&value?).ok()
}

fn save_credentials(credentials: &Credentials) {
    let Ok(value) = serde_json::to_string(credentials) else {
        return;
    };
    let Ok(key) = serde_json::to_string(STORAGE_KEY) else {
        return;
    };
    let Ok(value) = serde_json::to_string(&value) else {
        return;
    };
    let _ = document::eval(&format!("window.localStorage.setItem({key}, {value});"));
}

fn clear_credentials() {
    if let Ok(key) = serde_json::to_string(STORAGE_KEY) {
        let _ = document::eval(&format!("window.localStorage.removeItem({key});"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairing_url() {
        assert_eq!(
            parse_pairing_url("paste into Vmux Remote: https://mac.example.ts.net/#token=secret")
                .unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net".to_string(),
                token: "secret".to_string(),
            }
        );
    }

    #[test]
    fn parses_pairing_deep_link() {
        assert_eq!(
            parse_pairing_url(
                "vmuxremote://pair?base=https%3A%2F%2Fmac.example.ts.net%3A54821&token=secret"
            )
            .unwrap(),
            Credentials {
                base_url: "https://mac.example.ts.net:54821".to_string(),
                token: "secret".to_string(),
            }
        );
    }

    #[test]
    fn parses_sse_frames() {
        let mut buffer = b"data: {\"type\":\"delta\",\"text\":\"hi\"}\r\n\r\n".to_vec();
        let frame = take_sse_frame(&mut buffer).unwrap();
        assert_eq!(
            parse_sse_event(&frame),
            Some(RemoteEvent::Delta {
                text: "hi".to_string()
            })
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn groups_agent_activity_into_one_turn() {
        let items = group_messages(vec![
            Message::user("hello"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Thinking("working".to_string())],
            },
            Message::ToolResult {
                call_id: "tool-1".to_string(),
                content: "done".to_string(),
                is_error: false,
            },
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("answer".to_string())],
            },
        ]);

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], MobileChatItem::User { .. }));
        assert!(matches!(
            &items[1],
            MobileChatItem::Turn { blocks } if blocks.len() == 3
        ));
    }
}
