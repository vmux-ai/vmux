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
pub mod root;
mod runtime;
mod session;
mod surface;
mod transition;

use crate::logs::Logs;
use crate::pairing::{Credentials, PairCard};
use crate::plugins::PagePlugins;
use crate::remote::{Api, ApiError};
use crate::runtime::World;
use crate::session::{AuthState, use_session};
use vmux_chat::room::Agents;
use vmux_start::roster::Roster;

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use bevy_a11y::AccessibilityPlugin;
use bevy_app::{Plugin, TaskPoolPlugin};
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

type Pages = Box<dyn Fn(&mut bevy_app::App) + Send + Sync>;

pub struct MobilePlugin {
    root: &'static vmux_native::NativePage,
    pages: Pages,
}

impl Default for MobilePlugin {
    fn default() -> Self {
        Self {
            root: &root::APP_PAGE,
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

    pub fn serving(mut self, pages: impl Fn(&mut bevy_app::App) + Send + Sync + 'static) -> Self {
        self.pages = Box::new(pages);
        self
    }
}

impl Plugin for MobilePlugin {
    fn build(&self, app: &mut bevy_app::App) {
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
        .add_plugins(root::RootPlugin(self.root));
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
pub fn App() -> Element {
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
    let mut team_open = use_signal(|| false);

    use_context_provider(|| {
        PageBack::new(EventHandler::new(move |()| {
            team_open.set(false);
            session.leave();
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

    if team_open() {
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

    if session.is_open() {
        return rsx! {
            vmux_chat::page::Page {}
        };
    }

    rsx! {
        div { class: "relative h-dvh bg-background",
            div { class: "flex h-full flex-col py-[calc(3rem+env(safe-area-inset-top))]",
                vmux_start::page::Page {}
            }
            LinkStatus {
                reachable: reachable(),
                on_team: move |_| team_open.set(true),
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

#[component]
fn LinkStatus(
    reachable: bool,
    on_team: EventHandler<()>,
    on_disconnect: EventHandler<()>,
) -> Element {
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
            span { class: "text-sm font-semibold tracking-tight text-foreground", "Vmux" }
            span { class: "pointer-events-auto ml-auto {pill}",
                span { class: "{dot}" }
                {label}
            }
            button {
                class: "pointer-events-auto ml-2 rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                r#type: "button",
                onclick: move |_| on_team.call(()),
                {translate("mobile-start-team")}
            }
            button {
                class: "pointer-events-auto rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                r#type: "button",
                onclick: move |_| on_disconnect.call(()),
                {translate("mobile-pair-disconnect")}
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
