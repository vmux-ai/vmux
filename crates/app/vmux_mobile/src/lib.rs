#![allow(non_snake_case)]

mod credentials;
mod deep_link;
mod logs;
pub mod nav;
pub mod navigator;
mod page_host;
mod pairing;
mod plugins;
mod qr_scanner;
mod quic;
mod remote;
mod runtime;
pub mod screen;

mod session;
mod shell;
mod surface;
mod transition;

use crate::logs::Logs;
use crate::nav::{OpenBlank, Push, Report, Select};
use crate::navigator::{Screen, Stack, Tabs, use_navigation};
use crate::pairing::{Credentials, PairCard};
use crate::plugins::PagePlugins;
use crate::remote::{Api, ApiError};
use crate::runtime::World;
use crate::screen::{Mac, Name, Shown};
use crate::session::{AuthState, use_session};
use vmux_chat::room::Agents;
use vmux_start::roster::Roster;

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use bevy_a11y::AccessibilityPlugin;
use bevy_app::{App, Plugin, TaskPoolPlugin};
use bevy_input::InputPlugin;
use bevy_time::TimePlugin;
use bevy_window::WindowPlugin;
use bevy_winit::{UpdateMode, WinitPlugin, WinitSettings};
use dioxus::prelude::*;
use vmux_ui::back::PageBack;
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::i18n::translate;
use vmux_wire::room::{RemoteAgent, RemoteSession};

static OPENED_URLS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

static RESUMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

type Pages = Box<dyn Fn(&mut App) + Send + Sync>;

pub struct MobilePlugin {
    root: &'static vmux_native::NativePage,
    pages: Pages,
}

impl Default for MobilePlugin {
    fn default() -> Self {
        Self {
            root: &shell::SHELL_PAGE,
            pages: Box::new(|world| {
                world.add_plugins(PagePlugins);
            }),
        }
    }
}

impl MobilePlugin {
    pub fn showing(root: &'static vmux_native::NativePage) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn serving(mut self, pages: impl Fn(&mut App) + Send + Sync + 'static) -> Self {
        self.pages = Box::new(pages);
        self
    }
}

impl Plugin for MobilePlugin {
    fn build(&self, app: &mut App) {
        Logs::start();
        deep_link::install();

        World::new(|world| (self.pages)(world)).install();

        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            InputPlugin,
            WindowPlugin {
                primary_window: Some(bevy_window::Window {
                    mode: bevy_window::WindowMode::BorderlessFullscreen(
                        bevy_window::MonitorSelection::Primary,
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            },
            AccessibilityPlugin,
            WinitPlugin::default(),
        ))
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Reactive {
                wait: Duration::from_millis(250),
                react_to_device_events: false,
                react_to_user_events: true,
                react_to_window_events: true,
            },
            unfocused_mode: UpdateMode::reactive_low_power(Duration::from_secs(1)),
        })
        .add_plugins(shell::ShellPlugin(self.root));
    }
}

#[cfg(target_os = "ios")]
pub(crate) fn offer_opened_url(url: String) {
    OPENED_URLS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(url);
}

pub(crate) fn mark_resumed() {
    RESUMED.store(true, std::sync::atomic::Ordering::Release);
}

pub(crate) fn take_resumed() -> bool {
    RESUMED.swap(false, std::sync::atomic::Ordering::AcqRel)
}

#[component]
pub fn Shell() -> Element {
    let mut auth = use_signal(|| AuthState::Loading);
    let mut pair_url = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut api = use_signal(|| None::<Api>);
    let mut sessions = use_signal(Vec::<RemoteSession>::new);
    let mut agents = use_signal(Vec::<RemoteAgent>::new);
    let session = use_session();
    let composer = page_host::use_composer_exchange();
    let mut reachable = use_signal(|| false);
    let mut pending_pair_url = use_signal(|| None::<String>);
    let mut deep_link_received = use_signal(|| false);
    let mut pairing = use_signal(|| false);

    use_context_provider(|| {
        PageBack::new(EventHandler::new(move |()| {
            World::with(|world| world.send(crate::nav::GoBack));
        }))
    });

    use_effect(move || {
        if let Some(client) = api() {
            page_host::install(client, sessions, session, composer);
        }
    });

    use_effect(move || {
        let roster = Roster {
            sessions: sessions(),
            agents: agents(),
        };
        World::with(|world| world.insert(roster));
    });

    use_effect(move || {
        World::with(|world| world.insert(Agents(agents())));
    });

    let _room = use_resource(move || {
        let client = api();
        let sid = session.sid();
        let generation = (session.generation)();
        async move {
            let Some(client) = client else {
                return;
            };
            if sid.is_empty() {
                return;
            }
            session.stream(client, sid, generation).await;
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
        let displaced = api.peek().clone();
        api.set(Some(client.clone()));
        if let Some(displaced) = displaced {
            displaced.close();
        }
        auth.set(AuthState::Paired);
        match client.sessions().await {
            Ok(next) => {
                sessions.set(next);
                if let Ok(next) = client.agents().await {
                    agents.set(next);
                }
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
                    if let Ok(next) = client.agents().await {
                        agents.set(next);
                    }
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
            match client.layout().await {
                Ok(snapshot) => {
                    let focused = snapshot.focused.stack.clone();
                    let tabs = Mac::tabs(&snapshot);
                    World::with(|world| world.send(Report { tabs, focused }));
                }
                Err(error) => tracing::warn!(%error, "the Mac would not report its layout"),
            }
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
                    if let Ok(next) = client.agents().await {
                        agents.set(next);
                    }
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

    if auth() == AuthState::Unpaired {
        return rsx! {
            PairScreen {
                value: pair_url(),
                error: error(),
                pairing: pairing(),
                on_value: move |value| pair_url.set(value),
                on_pair: move |_| pending_pair_url.set(Some(pair_url())),
                on_scan: move |_| {
                    error.set(String::new());
                    if let Err(message) = qr_scanner::open() {
                        error.set(message);
                    }
                },
            }
        };
    }

    rsx! {
        Stack::<Shown> {
            Tabs {
                Paired { api, sessions, agents, session, reachable, auth }
            }
        }
    }
}

#[component]
fn Paired(
    api: Signal<Option<Api>>,
    sessions: Signal<Vec<RemoteSession>>,
    agents: Signal<Vec<RemoteAgent>>,
    session: crate::session::Session,
    reachable: Signal<bool>,
    auth: Signal<AuthState>,
) -> Element {
    let navigation = use_navigation::<Shown>();
    use_effect(move || {
        let Some(Shown::Chat { sid: Some(sid), .. }) = navigation.route() else {
            return;
        };
        if session.sid() == sid {
            return;
        }
        for known in sessions.read().iter() {
            if known.sid == sid {
                session.open(known.clone());
                return;
            }
        }
    });

    let seen = navigation.state();
    let at_root = seen.depth == 0;
    let wants_a_bar = at_root && !seen.current.as_ref().is_some_and(Shown::has_own_input);
    rsx! {
        div { class: "relative flex h-dvh flex-col bg-background text-foreground",
            div { class: "flex min-h-0 flex-1 flex-col", CurrentScreen { api } }
            div { class: "shrink-0 pb-[calc(0.5rem+env(safe-area-inset-bottom))]",
                if wants_a_bar {
                    CommandBar {
                        on_submit: move |typed: String| {
                            let typed = typed.trim().to_string();
                            if typed.is_empty() {
                                return;
                            }
                            match Destination::of(&typed) {
                                Destination::Url(screen) => {
                                    World::with(|world| world.send(Push(screen)));
                                }
                                Destination::Prompt(text) => {
                                    let Some(client) = api() else { return };
                                    session.start_chat(client, sessions, text, None);
                                }
                            }
                        },
                    }
                }
                TabBar {}
            }
            if at_root {
                LinkStatus {
                    reachable: reachable(),
                    on_disconnect: move |_| {
                        credentials::StoredCredentials::clear();
                        session.leave();
                        let displaced = api.peek().clone();
                        api.set(None);
                        if let Some(displaced) = displaced {
                            displaced.close();
                        }
                        sessions.set(Vec::new());
                        agents.set(Vec::new());
                        auth.set(AuthState::Unpaired);
                    },
                }
            }
        }
    }
}

#[component]
fn CurrentScreen(api: Signal<Option<Api>>) -> Element {
    rsx! {
        Screen::<Shown> { name: Name::Chat, component: &surface::AGENT }
        Screen::<Shown> { name: Name::Launcher, component: &surface::START }
        Screen::<Shown> { name: Name::Team, component: &surface::TEAM }
        RootScreen { api }
    }
}

#[component]
fn RootScreen(api: Signal<Option<Api>>) -> Element {
    let Some(root) = use_navigation::<Shown>().state().root else {
        return rsx! {};
    };
    match root {
        Shown::Chat { sid: Some(_), .. } => rsx! {
            vmux_chat::page::Page {}
        },
        Shown::Chat { .. } | Shown::Launcher => rsx! {
            div { class: "flex min-h-0 flex-1 flex-col pt-[calc(3rem+env(safe-area-inset-top))] pb-2",
                vmux_start::page::Page {}
            }
        },
        Shown::Team => rsx! {
            div { class: "flex min-h-0 flex-1 flex-col pt-[env(safe-area-inset-top)]", vmux_team::page::Page {} }
        },
        Shown::Mirror(stack) => rsx! {
            MirrorScreen { stack, api }
        },
    }
}

enum Destination {
    Url(Shown),
    Prompt(String),
}

impl Destination {
    fn of(typed: &str) -> Self {
        if !typed.contains("://") || typed.split_whitespace().count() > 1 {
            return Self::Prompt(typed.to_string());
        }
        Self::Url(Shown::addressed(typed))
    }
}

#[component]
fn LinkStatus(reachable: bool, on_disconnect: EventHandler<()>) -> Element {
    let (dot, pill, label) = if reachable {
        (
            "h-1.5 w-1.5 rounded-full bg-success",
            "flex items-center gap-1.5 rounded-full border border-success/20 bg-success/[0.08] px-2.5 py-1 text-[10px] font-medium text-success",
            translate("mobile-status-connected"),
        )
    } else {
        (
            "h-1.5 w-1.5 rounded-full bg-muted-foreground",
            "flex items-center gap-1.5 rounded-full border border-border bg-muted px-2.5 py-1 text-[10px] font-medium text-muted-foreground",
            translate("mobile-status-reaching"),
        )
    };
    rsx! {
        header { class: "pointer-events-none absolute inset-x-0 top-0 z-20 flex items-center gap-2 px-4 pb-3 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-6",
            span { class: "pointer-events-auto ml-auto {pill}",
                span { class: "{dot}" }
                {label}
            }
            button {
                class: "pointer-events-auto ml-2 rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                r#type: "button",
                onclick: move |_| on_disconnect.call(()),
                {translate("mobile-pair-disconnect")}
            }
        }
    }
}

#[component]
fn TabBar() -> Element {
    let seen = use_navigation::<Shown>().state();
    let tabs = seen.tabs.clone();
    let current = seen.selected.clone();
    rsx! {
        nav { class: "flex shrink-0 items-stretch gap-1 overflow-x-auto border-t border-border px-2 py-1",
            for tab in tabs {
                {
                    let selected = current.as_deref() == Some(tab.id.as_str());
                    let tone = if selected {
                        "text-foreground"
                    } else {
                        "text-muted-foreground"
                    };
                    let id = tab.id.clone();
                    rsx! {
                        button {
                            key: "{tab.id}",
                            class: "min-w-0 flex-1 truncate rounded-lg px-2 py-2 text-xs font-medium active:bg-accent {tone}",
                            r#type: "button",
                            onclick: move |_| { World::with(|world| world.send(Select(id.clone()))); },
                            "{tab.name}"
                        }
                    }
                }
            }
            button {
                class: "shrink-0 rounded-lg px-3 py-2 text-base leading-none font-medium text-muted-foreground active:bg-accent",
                r#type: "button",
                "aria-label": translate("mobile-nav-new-tab"),
                onclick: move |_| { World::with(|world| world.send(OpenBlank(Shown::Launcher))); },
                "+"
            }
        }
    }
}

#[component]
fn CommandBar(on_submit: EventHandler<String>) -> Element {
    let mut typed = use_signal(String::new);
    rsx! {
        div { class: "flex shrink-0 items-center gap-2 px-3 py-2",
            input {
                class: "min-w-0 flex-1 rounded-full border border-border bg-muted px-4 py-2.5 text-base text-foreground shadow-sm placeholder:text-muted-foreground focus:outline-none",
                r#type: "text",
                autocapitalize: "none",
                autocorrect: "off",
                spellcheck: "false",
                enterkeyhint: "go",
                value: "{typed}",
                placeholder: translate("mobile-nav-url-placeholder"),
                oninput: move |event| typed.set(event.value()),
                onkeydown: move |event| {
                    if event.key() != Key::Enter {
                        return;
                    }
                    let value = typed();
                    typed.set(String::new());
                    on_submit.call(value);
                },
            }
        }
    }
}

const MIRROR_ATTEMPTS: u8 = 5;

const MIRROR_RETRY: Duration = Duration::from_secs(2);

#[component]
fn MirrorScreen(stack: vmux_wire::protocol::layout::Stack, api: Signal<Option<Api>>) -> Element {
    let title = if stack.title.is_empty() {
        stack.url.clone()
    } else {
        stack.title.clone()
    };
    let process_id = stack.process_id.clone();
    let screen = use_resource(move || {
        let (client, process_id) = (api(), process_id.clone());
        async move {
            let (Some(api), Some(process_id)) = (client, process_id) else {
                return None;
            };
            let mut remaining = MIRROR_ATTEMPTS;
            loop {
                match api.terminal(&process_id).await {
                    Ok(text) => return Some(text),
                    Err(ApiError::NotFound) => return None,
                    Err(error) => tracing::warn!("mirroring the terminal failed: {error:?}"),
                }
                remaining -= 1;
                if remaining == 0 {
                    return None;
                }
                tokio::time::sleep(MIRROR_RETRY).await;
            }
        }
    });
    let mirrored = screen.read().clone().flatten();
    rsx! {
        div { class: "flex min-h-0 flex-1 flex-col gap-3 overflow-hidden px-6 pb-8 pt-[calc(1.5rem+env(safe-area-inset-top))]",
            div { class: "flex shrink-0 items-center gap-2",
                vmux_ui::back::BackButton {}
                span { class: "truncate text-sm font-semibold text-foreground", "{title}" }
            }
            match mirrored {
                Some(text) => rsx! {
                    div { class: "min-h-0 flex-1 overflow-auto rounded-2xl border border-border bg-muted/40 p-3",
                        pre { class: "whitespace-pre font-mono text-[11px] leading-tight text-foreground", "{text}" }
                    }
                    p { class: "shrink-0 text-center text-[11px] text-muted-foreground", {translate("mobile-nav-read-only")} }
                },
                None => rsx! {
                    div { class: "shrink-0 rounded-2xl border border-border bg-muted/40 p-4",
                        p { class: "text-sm font-medium text-foreground", {translate("mobile-nav-unsupported")} }
                        p { class: "mt-1 text-xs text-muted-foreground", {translate("mobile-nav-open-on-mac")} }
                        if !stack.url.is_empty() {
                            p { class: "mt-3 break-all font-mono text-[11px] text-muted-foreground", "{stack.url}" }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn PairScreen(
    value: String,
    error: String,
    pairing: bool,
    on_value: EventHandler<String>,
    on_pair: EventHandler<()>,
    on_scan: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "relative isolate flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground",
            style: START_BACKDROP_STYLE,
            StartBackdrop {}
            main { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-[calc(2rem+env(safe-area-inset-bottom))] pt-[calc(3.5rem+env(safe-area-inset-top))] sm:px-6 md:pt-20",
                StartHero {
                    mark: rsx! {
                        div { class: "flex h-11 w-11 items-center justify-center rounded-2xl border border-border bg-gradient-to-br from-violet-500/80 to-cyan-400/80 text-sm font-bold text-white shadow-lg shadow-violet-950/40", "V" }
                    },
                    PairCard { value, error, pairing, on_value, on_pair, on_scan }
                }
            }
        }
    }
}

fn take_opened_url() -> Option<String> {
    OPENED_URLS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .pop()
}
