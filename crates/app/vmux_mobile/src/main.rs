#![allow(non_snake_case)]

//! The phone app: a QUIC link to one Mac, and the desktop's own pages drawn over it.
//!
//! Nothing here draws a conversation or a launcher. Those are [`vmux_chat`] and [`vmux_start`],
//! the same crates the desktop mounts, reaching this app through [`page_host`] instead of through
//! Bevy. What is left is the shell: finding a Mac, holding the link, and deciding which page is on
//! screen.

mod api;
mod credentials;
mod lifecycle;
mod logs;
mod native_transition;
mod page_host;
mod pairing;
mod qr_scanner;
mod quic_api;
mod session;
mod start_page;
mod world;

use crate::api::{Api, ApiError};
use crate::logs::Logs;
use crate::pairing::{Credentials, PairCard};
use crate::session::{AuthState, use_session};
use crate::start_page::StartPagePlugin;
use crate::world::World;
use vmux_start::roster::Roster;

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use dioxus::prelude::*;
use vmux_ui::back::PageBack;
use vmux_ui::components::start_hero::{START_BACKDROP_STYLE, StartBackdrop, StartHero};
use vmux_ui::i18n::translate;
use vmux_wire::room::{RemoteAgent, RemoteSession};

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

    // The world rides in the event handler rather than on a thread of its own. Dioxus owns the
    // loop here — `LaunchBuilder::mobile()` never returns — and two loops was never an option:
    // `UIApplicationMain` may be called once per process, and both tao and winit assert on it. So
    // the world runs on the thread the pages do, and nothing has to cross one to reach it.
    World::new(|app| {
        app.add_plugins(StartPagePlugin);
    })
    .install();
    lifecycle::install();

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
                // Every event for this turn has been dealt with by the time this arrives, which is
                // what makes it the turn boundary rather than an arbitrary moment inside one.
                Event::MainEventsCleared => {
                    World::with(World::tick);
                }
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

/// Everything below the head: the link's state, and which page it is showing.
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
    let session = use_session();
    let composer = page_host::use_composer_exchange();
    // Whether the Mac is answering, as opposed to whether this device is paired.
    // Conflating the two let the header claim Connected while every request timed out.
    let mut reachable = use_signal(|| false);
    let mut pending_pair_url = use_signal(|| None::<String>);
    let mut deep_link_received = use_signal(|| false);
    let mut pairing = use_signal(|| false);
    let mut team_open = use_signal(|| false);

    // A page hosted here is the whole window, so the way out has to come from inside it. Provided
    // unconditionally because it is a hook; leaving nothing is a no-op.
    use_context_provider(|| {
        PageBack::new(EventHandler::new(move |()| {
            team_open.set(false);
            session.leave();
        }))
    });

    // Shared pages reach the desktop through the installed host, so it has to exist before one
    // mounts. Keying off the signal covers every path that pairs, not just the resume-on-launch one.
    use_effect(move || {
        if let Some(client) = api() {
            page_host::install(client, sessions, agents, session, composer);
        }
    });

    // The launcher is projected in the world from what the link last reported, so the roster has
    // to get there. Reading both signals inside the effect is what subscribes it to their changes.
    use_effect(move || {
        let roster = Roster {
            sessions: sessions(),
            agents: agents(),
        };
        World::with(|world| world.insert(roster));
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
        // The desktop's team page, unmodified — it reads its roster off the installed host exactly
        // as it does inside CEF.
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
        // The desktop's chat page, unmodified. Everything it renders — transcript, approvals,
        // model picker, composer — is fed by `page_host` off the QUIC link.
        return rsx! {
            vmux_chat::page::Page {}
        };
    }

    // The desktop's launcher, unmodified, under the link's own state.
    //
    // The header has its own row rather than floating over the page. Overlaying kept the hero
    // centred against the whole screen, but a keyboard shrinks the viewport until the content no
    // longer fits, and the moment centring gives up the hero starts at the top — behind the
    // header. A row cannot be overlapped.
    rsx! {
        div { class: "flex h-dvh flex-col bg-background",
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
            // A flex column, because that is how the page expects to be given its height — the
            // desktop's page root is one too. `min-h-0` because a flex child defaults to its
            // content's minimum height, which for a page that scrolls is the whole scrollable
            // length: it would push the row taller than the screen instead of scrolling inside it.
            div { class: "flex min-h-0 flex-1 flex-col",
                vmux_start::page::Page {}
            }
        }
    }
}

/// Whether the Mac is answering, and the two things that can be done about it.
///
/// The launcher has nowhere to say any of this: on the desktop the link is the machine it is
/// running on. So the phone gives it a row of its own above the page.
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
        header { class: "flex shrink-0 items-center gap-2 px-4 pb-3 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-6",
            span { class: "text-sm font-semibold tracking-tight text-foreground", "Vmux" }
            span { class: "ml-auto {pill}",
                span { class: "{dot}" }
                {label}
            }
            button {
                class: "ml-2 rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                r#type: "button",
                onclick: move |_| on_team.call(()),
                {translate("mobile-start-team")}
            }
            button {
                class: "rounded-lg px-2 py-1 text-xs text-muted-foreground active:bg-accent",
                r#type: "button",
                onclick: move |_| on_disconnect.call(()),
                {translate("mobile-pair-disconnect")}
            }
        }
    }
}

/// Finding a Mac in the first place, which is the one thing the desktop never has to do.
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
