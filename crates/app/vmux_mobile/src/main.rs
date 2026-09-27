#![allow(non_snake_case)]

mod credentials;
mod lifecycle;
mod logs;
mod page_host;
mod pairing;
mod qr_scanner;
mod quic;
mod remote;
mod runtime;
mod session;
mod transition;

use crate::logs::Logs;
use crate::pairing::{
    AuthState, DisconnectRequest, PairCard, PairLinkChanged, PairRequest, PairingFailure,
    PairingPlugin,
};
use crate::runtime::RuntimeHandle;
use crate::session::{LeaveSession, RestartSession, SessionPlugin, use_session};
use bevy_app::{App as BevyApp, AppExit, Plugin};
use vmux_chat::room::Agents;
use vmux_start::roster::Roster;

use dioxus::prelude::*;
use vmux_ui::back::PageBack;
use vmux_ui::components::start_hero::{START_BACKDROP_CLASS, StartBackdrop, StartHero};
use vmux_ui::i18n::translate;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.out.css");

const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
#[cfg(target_os = "ios")]
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

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
    BevyApp::new()
        .add_plugins((
            vmux_app::VmuxPlugin::builder().mobile().build(),
            MobilePlugin,
        ))
        .run();
}

struct MobilePlugin;

impl Plugin for MobilePlugin {
    fn build(&self, app: &mut BevyApp) {
        app.add_plugins((runtime::MobileRuntimePlugin, PairingPlugin, SessionPlugin))
            .set_runner(MobileRunner::run);
    }
}

struct MobileRunner;

impl MobileRunner {
    fn run(app: BevyApp) -> AppExit {
        let runtime = RuntimeHandle::from_app(app);
        lifecycle::install(runtime.clone());

        let event_runtime = runtime.clone();
        let config = dioxus::mobile::Config::new()
            .with_background_color(webview_background())
            .with_custom_event_handler(move |event, _| {
                use dioxus::mobile::tao::event::Event;
                match event {
                    Event::Opened { urls } => {
                        for url in urls {
                            if url.scheme() == "vmux" && url.host_str() == Some("pair") {
                                event_runtime.send(PairRequest(url.to_string()));
                            }
                        }
                    }
                    Event::Resumed => {
                        event_runtime.send(RestartSession);
                    }
                    Event::MainEventsCleared => event_runtime.update(),
                    _ => {}
                }
            });
        dioxus::LaunchBuilder::mobile()
            .with_cfg(config)
            .launch(MobileApp);
        AppExit::Success
    }
}

#[component]
fn MobileApp() -> Element {
    use_context_provider(RuntimeHandle::ui);
    rsx! {
        AppHead {}
        AppBody {}
    }
}

#[component]
fn AppBody() -> Element {
    let runtime = use_context::<RuntimeHandle>();
    transition::install(&dioxus::mobile::window());
    qr_scanner::install(&dioxus::mobile::window(), runtime.clone());
    let connection = pairing::use_connection(runtime.clone());
    let api = connection.api;
    let sessions = connection.sessions;
    let agents = connection.agents;
    let session = use_session(runtime.clone());
    let composer = page_host::use_composer_exchange();
    let mut team_open = use_signal(|| false);

    let page_back_runtime = runtime.clone();
    use_context_provider(|| {
        PageBack::new(EventHandler::new(move |()| {
            team_open.set(false);
            page_back_runtime.send(LeaveSession);
        }))
    });

    let host_runtime = runtime.clone();
    use_effect(move || {
        if let Some(client) = api() {
            page_host::install(host_runtime.clone(), client, sessions, session, composer);
        }
    });

    let roster_runtime = runtime.clone();
    use_effect(move || {
        let roster = Roster {
            sessions: sessions(),
            agents: agents(),
        };
        roster_runtime.send(roster);
    });

    let agents_runtime = runtime.clone();
    use_effect(move || {
        agents_runtime.send(Agents(agents()));
    });

    let view = (connection.view)();

    if view.auth == AuthState::Loading {
        return rsx! {
            div { class: "flex h-dvh items-center justify-center bg-background text-foreground",
                div { class: "h-8 w-8 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            }
        };
    }

    if view.auth == AuthState::Unpaired {
        let value_runtime = runtime.clone();
        let pair_runtime = runtime.clone();
        let scan_runtime = runtime.clone();
        let pair_url = view.pair_url.clone();
        return rsx! {
            PairScreen {
                value: view.pair_url,
                error: view.error,
                pairing: view.pairing,
                on_value: move |value| {
                    value_runtime.send(PairLinkChanged(value));
                },
                on_pair: move |_| {
                    pair_runtime.send(PairRequest(pair_url.clone()));
                },
                on_scan: move |_| {
                    if let Err(message) = qr_scanner::open() {
                        scan_runtime.send(PairingFailure(message));
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
                div { class: "min-h-0 flex-1", vmux_team::ui::Page {} }
            }
        };
    }

    if session.is_open() {
        return rsx! {
            vmux_chat::ui::Page {}
        };
    }

    rsx! {
        div { class: "relative h-dvh bg-background",
            div { class: "flex h-full flex-col py-[calc(3rem+env(safe-area-inset-top))]",
                vmux_start::ui::Page {}
            }
            LinkStatus {
                reachable: view.reachable,
                on_team: move |_| team_open.set(true),
                on_disconnect: move |_| {
                    runtime.send(DisconnectRequest);
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
            class: "relative isolate flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground {START_BACKDROP_CLASS}",
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
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: TAILWIND_CSS }
    }
}
