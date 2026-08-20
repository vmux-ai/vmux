#![allow(non_snake_case)]

mod api;
mod composer;
mod credentials;
mod logs;
mod native_transition;
mod page_host;
mod pairing;
mod qr_scanner;
mod quic_api;
mod session;
mod start;

use crate::api::{Api, ApiError};
use crate::composer::{
    ComposerOptions, insert_media_token, select_remote_media_entry, submit_remote_prompt,
    use_remote_model_state,
};
use crate::logs::Logs;
use crate::pairing::Credentials;
use crate::session::{
    AuthState, MobileRoomProjection, leave_session, open_session, start_new_chat,
};
use crate::start::MobileStartPage;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::*;
use vmux_chat::format::composer::{SelectorMode, filter_models, selector_mode};
use vmux_chat::page::agent::StatusDot;
use vmux_chat::page::approval::ApprovalPanel;
use vmux_chat::page::composer::ComposerStatus;
use vmux_chat::page::composer::options::ModelMenu;
use vmux_chat::transcript::{AssistantTurn, ChatItemRow, MD_CSS, WorkingIndicator};
use vmux_ui::components::prompt_box::{PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_composer::{
    PromptComposer, PromptComposerAction, PromptComposerAttachment,
};
use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::favicon::Favicon;
use vmux_ui::file_icon::FilePath;
use vmux_ui::hooks::{MenuDirection, move_selection};
use vmux_ui::i18n::translate;
use vmux_wire::chat::latest_tool_location;
use vmux_wire::prompt_media::ChatAttachment;
use vmux_wire::room::{
    ApprovalRequest, ModelOptionEntry, RemoteAgent, RemoteApproval, RemoteMediaEntry,
    RemoteSession, RemoteStatus, inline_media_query, replace_inline_media_query,
};

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.out.css");
static OPENED_URLS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Set when the app comes back to the foreground.
///
/// iOS tears down the UDP socket while suspended without closing the QUIC connection, so a
/// connection that still looks alive after a resume usually is not. Reconnecting on the next call
/// is cheaper than discovering it through a stalled request.
static RESUMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `background` from `crates/vmux_ui/assets/theme.css`, as the webview wants it.
///
/// oklch(0.88 0 0) and oklch(0.145 0 0), converted to sRGB — the same two values
/// `packaging/ios/Assets.xcassets/LaunchBackground.colorset` carries for the launch screen, so
/// the launch screen and the first webview frame are the same colour.
const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

/// The webview's own background, which is what shows before the document loads at all.
///
/// Without this the first frame after the launch screen is plain white — a stylesheet cannot
/// reach it, because there is no document yet. It has to be one colour decided up front, before
/// there is any UIKit environment to consult, so this reads the appearance from
/// `currentTraitCollection`, which UIKit documents as meaningful only inside a trait-environment
/// callback and this is not one.
///
/// It does answer correctly here, checked both ways: a dark cold start produced a dark first
/// frame, where an `Unspecified` reading would have fallen through to a light one, and forcing
/// the reading to light produced a light frame in dark mode. There is no second line of defence
/// behind it — an inline media query in the document was tried and does not repaint over this —
/// so if the reading is ever wrong, the wrong colour shows until the app paints.
#[cfg(target_os = "ios")]
fn webview_background() -> (u8, u8, u8, u8) {
    use objc2_ui_kit::{UITraitCollection, UIUserInterfaceStyle};

    let style = unsafe { UITraitCollection::currentTraitCollection().userInterfaceStyle() };
    if style == UIUserInterfaceStyle::Dark {
        DARK_BACKGROUND
    } else {
        LIGHT_BACKGROUND
    }
}

#[cfg(not(target_os = "ios"))]
fn webview_background() -> (u8, u8, u8, u8) {
    LIGHT_BACKGROUND
}

fn main() {
    Logs::start();

    let config = dioxus::mobile::Config::new()
        .with_background_color(webview_background())
        .with_custom_event_handler(|event, _| {
            use dioxus::mobile::tao::event::Event;
            match event {
                Event::Opened { urls } => {
                    let mut opened = OPENED_URLS
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    opened.extend(
                        urls.iter()
                            .filter(|url| url.scheme() == "vmux" && url.host_str() == Some("pair"))
                            .map(ToString::to_string),
                    );
                }
                Event::Resumed => RESUMED.store(true, std::sync::atomic::Ordering::Release),
                _ => {}
            }
        });
    dioxus::LaunchBuilder::mobile().with_cfg(config).launch(App);
}

/// Whether the app has resumed since this was last asked.
pub(crate) fn take_resumed() -> bool {
    RESUMED.swap(false, std::sync::atomic::Ordering::AcqRel)
}

#[component]
fn App() -> Element {
    rsx! {
        AppHead {}
        AppBody {}
    }
}

/// Everything below the head.
///
/// Split out so [`AppHead`] mounts exactly once. Dioxus records an inserted stylesheet href in a
/// root context and skips that href ever after, so a head rendered inside a branch loses its
/// stylesheet the first time the branch changes, and nothing puts it back.
#[component]
fn AppBody() -> Element {
    native_transition::install(&dioxus::mobile::window());
    qr_scanner::install(&dioxus::mobile::window());
    let mut auth = use_signal(|| AuthState::Loading);
    let mut pair_url = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut api = use_signal(|| None::<Api>);
    let mut sessions = use_signal(Vec::<RemoteSession>::new);
    let mut agents = use_signal(Vec::<RemoteAgent>::new);
    let selected_agent = use_signal(|| Option::<String>::None);
    let current = use_signal(|| None::<RemoteSession>);
    let room = use_signal(MobileRoomProjection::default);
    let live_delta = use_signal(String::new);
    let status = use_signal(|| RemoteStatus::Idle);
    let mut approval = use_signal(|| None::<RemoteApproval>);
    let mut draft = use_signal(String::new);
    let mut attachments = use_signal(Vec::<RemoteMediaEntry>::new);
    let mut media_entries = use_signal(Vec::<RemoteMediaEntry>::new);
    let mut media_loading = use_signal(|| false);
    let mut media_generation = use_signal(|| 0_u64);
    let mut media_selected = use_signal(|| 0_usize);
    let mut attachment_sid = use_signal(String::new);
    let connected = use_signal(|| false);
    // Whether the Mac is answering, as opposed to whether this device is paired.
    // Conflating the two let the header claim Connected while every request timed out.
    let mut reachable = use_signal(|| false);
    let mut stream_generation = use_signal(|| 0_u64);
    let mut pending_pair_url = use_signal(|| None::<String>);
    let mut deep_link_received = use_signal(|| false);
    let mut pairing = use_signal(|| false);
    let mut team_open = use_signal(|| false);
    let mut new_chat_draft = use_signal(String::new);
    let new_chat_error = use_signal(String::new);
    let creating_chat = use_signal(|| false);

    // Pinned to the bottom as the transcript grows. Through the mounted handle rather than by
    // reaching into the DOM, so the renderer decides how a scroll actually happens.
    let mut transcript_view = use_signal(|| None::<Event<MountedData>>);
    use_effect(move || {
        let _ = room.read().event_count();
        let _ = live_delta.read().len();
        let Some(view) = transcript_view() else {
            return;
        };
        spawn(async move {
            let Ok(size) = view.get_scroll_size().await else {
                return;
            };
            let bottom = PixelsVector2D::new(0.0, size.height);
            let _ = view.scroll(bottom, ScrollBehavior::Instant).await;
        });
    });

    use_effect(move || {
        let sid = current().map(|session| session.sid).unwrap_or_default();
        if *attachment_sid.peek() == sid {
            return;
        }
        attachment_sid.set(sid);
        attachments.set(Vec::new());
        media_entries.set(Vec::new());
        media_loading.set(false);
    });

    use_effect(move || {
        let value = draft();
        let query = inline_media_query(&value).map(|query| query.query.to_string());
        let sid = current().map(|session| session.sid).unwrap_or_default();
        let client = api();
        let generation = media_generation.peek().wrapping_add(1);
        media_generation.set(generation);
        media_selected.set(0);
        let (Some(query), Some(client)) = (query, client) else {
            media_entries.set(Vec::new());
            media_loading.set(false);
            return;
        };
        if sid.is_empty() {
            media_entries.set(Vec::new());
            media_loading.set(false);
            return;
        }
        media_loading.set(true);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            if *media_generation.peek() != generation {
                return;
            }
            let result = client.media(&sid, &query).await;
            if *media_generation.peek() != generation {
                return;
            }
            match result {
                Ok(entries) => media_entries.set(entries),
                Err(_) => media_entries.set(Vec::new()),
            }
            media_loading.set(false);
        });
    });

    // Shared pages reach the desktop through the installed host, so it has to exist before one
    // mounts. Keying off the signal covers every path that pairs, not just the resume-on-launch one.
    use_effect(move || {
        if let Some(client) = api() {
            page_host::install(client);
        }
    });

    use_future(move || async move {
        if let Some(opened) = take_opened_url() {
            deep_link_received.set(true);
            pair_url.set(opened.clone());
            pending_pair_url.set(Some(opened));
            auth.set(AuthState::Unpaired);
            return;
        }
        let Some(credentials) = credentials::StoredCredentials::load() else {
            if deep_link_received() {
                return;
            }
            auth.set(AuthState::Unpaired);
            return;
        };
        if deep_link_received() {
            return;
        }
        pair_url.set(credentials.pairing_url());
        let client = match Api::new(credentials) {
            Ok(client) => client,
            Err(reason) => {
                credentials::StoredCredentials::clear();
                error.set(reason.to_string());
                auth.set(AuthState::Unpaired);
                return;
            }
        };
        // Stored credentials already answer "is this paired?", so paint the start page now and let
        // reachability resolve behind it. Waiting on the first round trip meant a spinner for as
        // long as the dial takes to give up, which is the whole dial timeout when the Mac is off.
        let displaced = api.peek().clone();
        api.set(Some(client.clone()));
        if let Some(displaced) = displaced {
            displaced.close();
        }
        auth.set(AuthState::Paired);
        match client.sessions().await {
            Ok(next) => {
                sessions.set(next);
                agents.set(client.agents().await.unwrap_or_default());
                reachable.set(true);
            }
            Err(ApiError::Unauthorized) => {
                credentials::StoredCredentials::clear();
                error.set(translate("mobile-error-pairing-expired"));
                auth.set(AuthState::Unpaired);
            }
            Err(other) => error.set(other.to_string()),
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
            if let Some(result) = qr_scanner::take_result() {
                match result {
                    Ok(scanned) => {
                        pair_url.set(scanned.clone());
                        error.set(String::new());
                        pending_pair_url.set(Some(scanned));
                    }
                    Err(message) => error.set(message),
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let pending = pending_pair_url.write().take();
            let Some(input) = pending else {
                continue;
            };
            let credentials = match Credentials::parse(&input) {
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
            let client = match Api::new(credentials.clone()) {
                Ok(client) => client,
                Err(reason) => {
                    pairing.set(false);
                    error.set(reason.to_string());
                    auth.set(AuthState::Unpaired);
                    continue;
                }
            };
            match client.sessions().await {
                Ok(next) => {
                    credentials::StoredCredentials::save(&credentials);
                    pair_url.set(credentials.pairing_url());
                    let displaced = api.peek().clone();
                    api.set(Some(client.clone()));
                    if let Some(displaced) = displaced {
                        displaced.close();
                    }
                    sessions.set(next);
                    auth.set(AuthState::Paired);
                }
                Err(ApiError::Unauthorized) => {
                    error.set(translate("mobile-error-token-rejected"));
                    auth.set(AuthState::Unpaired);
                }
                Err(other) => {
                    error.set(other.to_string());
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
                    sessions.set(next);
                    reachable.set(true);
                    error.set(String::new());
                }
                Err(ApiError::Unauthorized) => {
                    reachable.set(false);
                    credentials::StoredCredentials::clear();
                    let displaced = api.peek().clone();
                    api.set(None);
                    if let Some(displaced) = displaced {
                        displaced.close();
                    }
                    error.set(translate("mobile-error-pairing-expired"));
                    auth.set(AuthState::Unpaired);
                }
                Err(other) => {
                    reachable.set(false);
                    error.set(other.to_string());
                }
            }
        }
    });

    if auth() == AuthState::Loading {
        return rsx! {
            div { class: "flex h-dvh items-center justify-center bg-background text-foreground",
                div { class: "h-8 w-8 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            }
        };
    }

    if team_open() {
        // vmux_team::page::Page is the desktop's team page, unmodified — it reads TEAM_EVENT off
        // the installed host exactly as it does inside CEF. Only the way back is ours.
        return rsx! {
            div { class: "flex h-dvh flex-col bg-background text-foreground",
                div { class: "flex items-center gap-1 border-b border-border px-2 pt-[env(safe-area-inset-top)]",
                    button {
                        class: "rounded-lg px-3 py-2 text-sm text-muted-foreground active:bg-accent",
                        r#type: "button",
                        onclick: move |_| team_open.set(false),
                        {translate("mobile-chat-back")}
                    }
                }
                div { class: "min-h-0 flex-1", vmux_team::page::Page {} }
            }
        };
    }

    if current().is_none() {
        return rsx! {
            MobileStartPage {
                paired: auth() == AuthState::Paired,
                reachable: reachable(),
                sessions: sessions(),
                agents: agents(),
                draft: new_chat_draft(),
                error: new_chat_error(),
                creating: creating_chat(),
                pair_value: pair_url(),
                pair_error: error(),
                pairing: pairing(),
                on_draft: move |value| new_chat_draft.set(value),
                on_submit: move |_| start_new_chat(
                    api,
                    sessions,
                    current,
                    room,
                    live_delta,
                    status,
                    approval,
                    connected,
                    stream_generation,
                    new_chat_draft,
                    new_chat_error,
                    creating_chat,
                    selected_agent(),
                ),
                on_start_agent: move |url: String| start_new_chat(
                    api,
                    sessions,
                    current,
                    room,
                    live_delta,
                    status,
                    approval,
                    connected,
                    stream_generation,
                    new_chat_draft,
                    new_chat_error,
                    creating_chat,
                    Some(url),
                ),
                on_open: move |session| {
                    let Some(client) = api() else { return };
                    open_session(
                        client,
                        session,
                        current,
                        room,
                        live_delta,
                        status,
                        approval,
                        connected,
                        stream_generation,
                    );
                },
                on_pair_value: move |value| pair_url.set(value),
                on_pair: move |_| {
                    pending_pair_url.set(Some(pair_url()));
                },
                on_scan: move |_| {
                    error.set(String::new());
                    if let Err(message) = qr_scanner::open() {
                        error.set(message);
                    }
                },
                on_disconnect: move |_| {
                    credentials::StoredCredentials::clear();
                    stream_generation.set(stream_generation().wrapping_add(1));
                    let displaced = api.peek().clone();
                    api.set(None);
                    if let Some(displaced) = displaced {
                        displaced.close();
                    }
                    sessions.set(Vec::new());
                    auth.set(AuthState::Unpaired);
                },
                on_open_team: move |_| team_open.set(true),
            }
        };
    }

    let current_value = current();
    // The session says which agent it is by name; the icon lives on the agent list the phone
    // already fetches, so no extra round trip and nothing new on the wire.
    let matched_agent = current_value.as_ref().and_then(|session| {
        agents()
            .into_iter()
            .find(|agent| agent.name == session.name)
    });
    let agent_icon = matched_agent
        .as_ref()
        .map(|agent| agent.icon.clone())
        .unwrap_or_default();
    // Derived rather than sent: agent_accent is a pure function of the agent id and already lives
    // in the shared crate, so the phone reaches the same colours the desktop does without the
    // wire carrying a theme.
    let agent_segment = matched_agent
        .as_ref()
        .map(|agent| agent.id.as_str())
        .unwrap_or_default();
    let accent = vmux_ui::agent_accent::agent_accent(agent_segment);
    // What the desktop paints --agent-accent with: the agent's avatar colour, which is a pure
    // function of its URL segment. Deriving it rather than sending it keeps the two in step.
    let accent_css = vmux_wire::avatar::agent_color(agent_segment);
    let selected_sid = current_value
        .as_ref()
        .map(|session| session.sid.clone())
        .unwrap_or_default();
    let is_streaming = matches!(status(), RemoteStatus::Streaming);
    // The words ComposerStatus matches on. Same component as the desktop, so the mapping has to
    // land on the same strings rather than on RemoteStatus's own names.
    let status_word = match status() {
        RemoteStatus::Streaming => "streaming",
        RemoteStatus::Errored(_) => "errored",
        RemoteStatus::Interrupted => "interrupted",
        RemoteStatus::Idle => "idle",
    };
    let draft_value = draft();
    let can_send = current_value.is_some()
        && (!draft_value.trim().is_empty() || !attachments.read().is_empty());
    let prompt_action = if is_streaming {
        PromptComposerAction::Stop
    } else {
        PromptComposerAction::Send
    };
    let prompt_attachments = attachments
        .read()
        .iter()
        .enumerate()
        .map(|(index, attachment)| PromptComposerAttachment {
            key: format!("remote-attachment-{}", attachment.path),
            name: attachment.name.clone(),
            label: FilePath(&attachment.name).extension_label(),
            preview_data_url: attachment.preview_data_url.clone(),
            remove_index: Some(index),
        })
        .collect::<Vec<_>>();
    let prompt_media_options = media_entries
        .read()
        .iter()
        .map(|entry| PromptMediaOption {
            key: format!("remote-media-{}", entry.path),
            name: entry.name.clone(),
            display_path: entry.display_path(),
            preview_data_url: entry.preview_data_url.clone(),
            label: FilePath(&entry.name).extension_label(),
            is_dir: entry.is_dir,
        })
        .collect::<Vec<_>>();
    let media_menu_open = inline_media_query(&draft_value).is_some();
    let mut model_state = use_remote_model_state(selected_sid.clone(), api);
    let mut model_selected = use_signal(|| 0usize);
    let model_matches = match selector_mode(&draft_value) {
        SelectorMode::Models(query) => Some(filter_models(&model_state().models, query)),
        _ => None,
    };
    let submit_sid = selected_sid.clone();
    let cancel_sid = selected_sid.clone();
    let approval_sid = selected_sid.clone();
    let approval_value = approval();
    let live_delta_value = live_delta();
    let room_value = room();
    let transcript_items = room_value.chat_items(&live_delta_value, is_streaming);
    let latest_tool = latest_tool_location(&transcript_items);
    let activity = vmux_wire::chat::activity_counts(&transcript_items);
    let attachment_previews = use_signal(HashMap::<String, ChatAttachment>::new);

    rsx! {
        div {
            class: "flex h-dvh min-h-0 flex-col bg-background text-foreground",
            style: "--agent-accent:{accent_css};",
            style { dangerous_inner_html: MD_CSS }
            header { class: "flex shrink-0 items-center gap-3 border-b border-border bg-background/95 px-3 pb-2 pt-[calc(0.5rem+env(safe-area-inset-top))] backdrop-blur-xl sm:px-5",
                button {
                    class: "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-accent text-lg text-accent-foreground active:bg-accent/70",
                    onclick: move |_| leave_session(
                        current,
                        room,
                        live_delta,
                        status,
                        approval,
                        connected,
                        stream_generation,
                    ),
                    aria_label: translate("mobile-chat-back-to-stacks"),
                    svg {
                        class: "h-5 w-5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "m15 18-6-6 6-6" }
                    }
                }
                if !agent_icon.is_empty() {
                    Favicon {
                        favicon_url: agent_icon.clone(),
                        url: String::new(),
                        class: "h-8 w-8 shrink-0 rounded-lg".to_string(),
                        globe_class: "h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                    }
                }
                div { class: "min-w-0 flex-1",
                    if let Some(session) = current_value.as_ref() {
                        div { class: "truncate text-sm font-semibold", "{session.name}" }
                        div { class: "mt-1 flex items-center gap-1.5 truncate text-[11px] text-muted-foreground",
                            StatusDot {
                                status: status_word.to_string(),
                                size_class: "h-2 w-2 shrink-0".to_string(),
                            }
                            span { "{session.runtime}" }
                            if let Some(model) = session.model.as_ref() {
                                span { "· {model}" }
                            }
                            span { "· {FilePath(&session.cwd).name()}" }
                        }
                    } else {
                        div { class: "text-sm font-semibold", "Vmux" }
                        div { class: "mt-1 text-[11px] text-muted-foreground", {translate("mobile-chat-no-session")} }
                    }
                }
                div { class: if connected() { "h-2 w-2 rounded-full bg-success" } else { "h-2 w-2 rounded-full bg-muted-foreground/50" } }
            }

            main {
                class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-5 sm:px-4 md:px-6",
                onmounted: move |event| transcript_view.set(Some(event)),
                if transcript_items.is_empty() && !is_streaming {
                    div { class: "flex h-full items-center justify-center px-8 text-center text-sm leading-6 text-muted-foreground",
                        {translate("mobile-chat-no-messages")}
                    }
                }
                div { class: "mx-auto flex w-full max-w-none flex-col gap-5 md:max-w-3xl",
                    for (index, item) in transcript_items.iter().cloned().enumerate() {
                        ChatItemRow {
                            key: "{index}",
                            absolute_index: index,
                            item,
                            attachment_previews,
                            latest_tool_block: (latest_tool.map(|(i, _)| i) == Some(index))
                                .then(|| latest_tool.map(|(_, b)| b))
                                .flatten(),
                        }
                    }
                    if is_streaming {
                        div { class: "flex flex-col",
                            AssistantTurn { WorkingIndicator {} }
                        }
                    }
                    if let RemoteStatus::Errored(message) = status() {
                        div { class: "mb-4 rounded-xl border border-destructive/20 bg-destructive/[0.06] px-3 py-2 text-xs text-destructive", "{message}" }
                    }
                }
            }

            if let Some(pending) = approval_value {
                div { class: "shrink-0",
                    ApprovalPanel {
                        tool: pending.name.clone(),
                        args_json: pending.args_json.clone(),
                        on_answer: move |decision| {
                            let Some(client) = api() else { return };
                            approval.set(None);
                            let call_id = pending.call_id.clone();
                            let sid = approval_sid.clone();
                            spawn(async move {
                                let _ = client
                                    .approve(&sid, &ApprovalRequest { call_id, decision })
                                    .await;
                            });
                        },
                    }
                }
            }

            div {
                class: "shrink-0 border-t border-border bg-background/95 px-2.5 pb-[calc(0.625rem+env(safe-area-inset-bottom))] pt-2.5 backdrop-blur-xl sm:px-4 md:px-6",
                div { class: "relative mx-auto w-full max-w-none md:max-w-3xl",
                    if let Some(models) = model_matches {
                        PromptPopup {
                            placement: PromptPopupPlacement::Upward,
                            ModelMenu {
                                models,
                                current_model_id: model_state().selected_id.clone(),
                                selected: model_selected(),
                                on_hover: move |index| model_selected.set(index),
                                on_select: move |model: ModelOptionEntry| {
                                    let sid = selected_sid.clone();
                                    let Some(client) = api.peek().clone() else { return };
                                    model_state.write().selected_id = model.id.clone();
                                    draft.set(String::new());
                                    model_selected.set(0);
                                    spawn(async move {
                                        let _ = client.select_model(&sid, &model.id).await;
                                    });
                                },
                            }
                        }
                    }
                    if media_menu_open {
                        PromptPopup {
                            placement: PromptPopupPlacement::Upward,
                            PromptMediaOptions {
                                items: prompt_media_options,
                                selected: media_selected(),
                                loading: media_loading(),
                                on_hover: move |index| media_selected.set(index),
                                on_select: move |index| {
                                    if let Some(entry) = media_entries.peek().get(index).cloned() {
                                        select_remote_media_entry(
                                            &entry,
                                            draft,
                                            attachments,
                                            media_selected,
                                        );
                                    }
                                },
                            }
                        }
                    }
                    PromptComposer {
                        value: draft_value.clone(),
                        attachments: prompt_attachments,
                        footer: rsx! {
                            div { class: "flex min-w-0 items-center justify-between gap-1",
                                ComposerOptions {
                                    state: model_state,
                                    sid: submit_sid.clone(),
                                    api,
                                    draft,
                                }
                                ComposerStatus {
                                    status: status_word.to_string(),
                                    active_subagents: activity.0,
                                    active_tasks: activity.1,
                                }
                            }
                        },
                        placeholder: if current_value.is_some() { translate("mobile-chat-placeholder") } else { translate("mobile-chat-no-session") },
                        accent_bg: accent.accent_bg.to_string(),
                        accent_color: accent_css.clone(),
                        accent_gradient: accent.grad.to_string(),
                        autofocus: true,
                        disabled: current_value.is_none(),
                        action: prompt_action,
                        action_title: if is_streaming { translate("mobile-chat-stop") } else { translate("mobile-chat-send") },
                        action_enabled: if is_streaming { true } else { can_send },
                        on_input: move |value| draft.set(value),
                        on_keydown: {
                            let sid = submit_sid.clone();
                            move |event: KeyboardEvent| {
                                let value = draft.peek().clone();
                                // The model picker is a draft-filtered popup like the media one, so
                                // it has to claim the same keys before Enter reaches the submit
                                // path below and sends "/model …" to the agent as a prompt.
                                if let SelectorMode::Models(query) = selector_mode(&value) {
                                    let matches = filter_models(&model_state.peek().models, query);
                                    if let Some(direction) = MenuDirection::of(&event.data()) {
                                        event.prevent_default();
                                        model_selected.set(move_selection(
                                            model_selected(),
                                            matches.len(),
                                            direction,
                                        ));
                                        return;
                                    }
                                    match event.key() {
                                        Key::Enter if !event.modifiers().shift() => {
                                            event.prevent_default();
                                            if let Some(model) =
                                                matches.get(model_selected()).cloned()
                                            {
                                                let sid = sid.clone();
                                                if let Some(client) = api.peek().clone() {
                                                    model_state.write().selected_id =
                                                        model.id.clone();
                                                    draft.set(String::new());
                                                    model_selected.set(0);
                                                    spawn(async move {
                                                        let _ = client
                                                            .select_model(&sid, &model.id)
                                                            .await;
                                                    });
                                                }
                                            }
                                            return;
                                        }
                                        Key::Escape => {
                                            event.prevent_default();
                                            draft.set(String::new());
                                            model_selected.set(0);
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                let media_open = inline_media_query(&value).is_some();
                                if media_open {
                                    match event.key() {
                                        Key::ArrowDown => {
                                            event.prevent_default();
                                            let len = media_entries.peek().len();
                                            if len > 0 {
                                                media_selected.set((media_selected() + 1) % len);
                                            }
                                            return;
                                        }
                                        Key::ArrowUp => {
                                            event.prevent_default();
                                            let len = media_entries.peek().len();
                                            if len > 0 {
                                                media_selected.set((media_selected() + len - 1) % len);
                                            }
                                            return;
                                        }
                                        Key::Enter if !event.modifiers().shift() => {
                                            event.prevent_default();
                                            if let Some(entry) = media_entries
                                                .peek()
                                                .get(media_selected())
                                                .cloned()
                                            {
                                                select_remote_media_entry(
                                                    &entry,
                                                    draft,
                                                    attachments,
                                                    media_selected,
                                                );
                                            }
                                            return;
                                        }
                                        Key::Escape => {
                                            event.prevent_default();
                                            if let Some(query) = inline_media_query(&value) {
                                                draft.set(replace_inline_media_query(
                                                    &value,
                                                    query,
                                                    "",
                                                ));
                                            }
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                if event.key() == Key::Enter
                                    && !event.modifiers().shift()
                                    && !is_streaming
                                {
                                    event.prevent_default();
                                    submit_remote_prompt(
                                        api,
                                        sid.clone(),
                                        draft,
                                        attachments,
                                        status,
                                    );
                                }
                            }
                        },
                        on_paste: move |_| {},
                        on_attach: move |_| insert_media_token(draft),
                        on_remove_attachment: move |index| {
                            let mut next = attachments.peek().clone();
                            if index < next.len() {
                                next.remove(index);
                                attachments.set(next);
                            }
                        },
                        on_action: {
                            let send_sid = submit_sid.clone();
                            let stop_sid = cancel_sid.clone();
                            move |_| {
                                if is_streaming {
                                    let Some(client) = api() else { return };
                                    let sid = stop_sid.clone();
                                    spawn(async move {
                                        let _ = client.cancel(&sid).await;
                                    });
                                } else {
                                    submit_remote_prompt(
                                        api,
                                        send_sid.clone(),
                                        draft,
                                        attachments,
                                        status,
                                    );
                                }
                            }
                        },
                    }
                }
            }
        }

    }
}

#[component]
fn AppHead() -> Element {
    rsx! {
        document::Title { "Vmux" }
        // This replaces the shell's own viewport tag, so it has to carry that tag's zoom lock
        // forward as well as viewport-fit. Dropping maximum-scale is what let focusing an input
        // zoom the page, which no font size on the input can prevent.
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: TAILWIND_CSS }
    }
}

fn take_opened_url() -> Option<String> {
    OPENED_URLS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .pop()
}
