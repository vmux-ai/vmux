#![allow(non_snake_case)]

use bevy_app::{App as BevyApp, AppExit, Plugin, PreUpdate};
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use dioxus::mobile::tao::event::Event;
use dioxus::prelude::*;
#[cfg(target_os = "ios")]
use objc2_ui_kit::{UITraitCollection, UIUserInterfaceStyle};
use vmux_ui::back::PageBack;
use vmux_ui::components::start_hero::{START_BACKDROP_CLASS, StartBackdrop, StartHero};
use vmux_ui::hooks::use_ui_state;
use vmux_ui::i18n::translate;

use crate::connection::{DisconnectRequest, PairCard, PairLinkChanged, PairRequest, ScanRequest};
use crate::logs::Logs;
use crate::presentation::{
    MobileAuthStage, MobileConnectionUiState, MobileScreenRoute, MobileSessionUiState,
};
use crate::runtime::RuntimeHandle;
use crate::session::{CloseTeam, LeaveScreen, OpenSession, OpenTeam, RestartSession};

mod actions;
mod catalog;
mod connection;
mod credentials;
mod host;
mod io;
mod lifecycle;
mod logs;
mod presentation;
mod qr_scanner;
mod quic;
mod remote;
mod runtime;
mod session;
mod stream;
#[cfg(test)]
mod tests;
mod transition;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.out.css");
const LIGHT_BACKGROUND: (u8, u8, u8, u8) = (215, 215, 215, 255);
#[cfg(target_os = "ios")]
const DARK_BACKGROUND: (u8, u8, u8, u8) = (10, 10, 10, 255);

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AdmitSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InvalidateSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CollectSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MaterializeSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DecideSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SubmitSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProjectSet;

fn main() {
    Logs::start();
    BevyApp::new()
        .add_plugins((
            vmux_app::VmuxPlugin::builder().mobile().build(),
            MobilePlugin,
        ))
        .run();
}

pub(crate) struct MobileDomainPlugin;

impl Plugin for MobileDomainPlugin {
    fn build(&self, app: &mut BevyApp) {
        if !app.is_plugin_added::<vmux_ecs::PrimitivesPlugin>() {
            app.add_plugins(vmux_ecs::PrimitivesPlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.configure_sets(
            PreUpdate,
            (
                AdmitSet,
                InvalidateSet,
                CollectSet,
                MaterializeSet,
                DecideSet,
                SubmitSet,
                ProjectSet,
            )
                .chain(),
        )
        .add_plugins((
            runtime::RuntimePlugin,
            connection::MobileConnectionPlugin,
            catalog::MobileCatalogPlugin,
            session::MobileSessionPlugin,
            stream::MobileStreamPlugin,
            actions::MobileActionsPlugin,
            presentation::MobilePresentationPlugin,
        ));
    }
}

struct MobilePlugin;

impl Plugin for MobilePlugin {
    fn build(&self, app: &mut BevyApp) {
        app.add_plugins((MobileDomainPlugin, io::MobileIoPlugin))
            .set_runner(MobileRunner::run);
    }
}

struct MobileRunner;

impl MobileRunner {
    fn run(app: BevyApp) -> AppExit {
        let runtime = RuntimeHandle::from_app(app);
        let _lifecycle = runtime.install_lifecycle();
        let event_runtime = runtime.clone();
        let config = dioxus::mobile::Config::new()
            .with_background_color(Self::webview_background())
            .with_custom_event_handler(move |event, _| match event {
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
            });
        dioxus::LaunchBuilder::mobile()
            .with_cfg(config)
            .launch(AppBody);
        AppExit::Success
    }

    #[cfg(target_os = "ios")]
    fn webview_background() -> (u8, u8, u8, u8) {
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
}

#[component]
fn AppBody() -> Element {
    let runtime = use_context_provider(RuntimeHandle::ui);
    use_hook({
        let runtime = runtime.clone();
        move || {
            let window = dioxus::mobile::window();
            OpenSession::install_sheet(&window);
            ScanRequest::install_host(&window, runtime);
        }
    });
    let lease = use_hook({
        let runtime = runtime.clone();
        move || {
            let (binding, lease) = host::MobileHost::mount(runtime);
            provide_context(binding);
            std::rc::Rc::new(std::cell::RefCell::new(Some(lease)))
        }
    });
    use_drop(move || {
        let _ = lease.borrow_mut().take();
    });
    let connection = use_ui_state::<MobileConnectionUiState>();
    let session = use_ui_state::<MobileSessionUiState>();
    let page_back_runtime = runtime.clone();
    use_context_provider(|| {
        PageBack::new(EventHandler::new(move |()| {
            page_back_runtime.send(LeaveScreen);
        }))
    });

    let connection_view = connection.state.read().clone();
    let session_view = session.state.read().clone();

    let content = match connection_view.auth {
        MobileAuthStage::Loading => rsx! {
            div { class: "flex h-dvh items-center justify-center bg-background text-foreground",
                div { class: "h-8 w-8 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" }
            }
        },
        MobileAuthStage::Unpaired => {
            let value_runtime = runtime.clone();
            let pair_runtime = runtime.clone();
            let scan_runtime = runtime.clone();
            let pair_url = connection_view.pair_url.clone();
            rsx! {
                PairScreen {
                    value: connection_view.pair_url,
                    error: connection_view.error,
                    pairing: connection_view.pairing,
                    on_value: move |value| {
                        value_runtime.send(PairLinkChanged(value));
                    },
                    on_pair: move |_| {
                        pair_runtime.send(PairRequest(pair_url.clone()));
                    },
                    on_scan: move |_| {
                        scan_runtime.send(ScanRequest);
                    },
                }
            }
        }
        MobileAuthStage::Paired => match session_view.route {
            MobileScreenRoute::Team => {
                let back_runtime = runtime.clone();
                rsx! {
                    TeamScreen {
                        on_back: move |_| back_runtime.send(CloseTeam),
                    }
                }
            }
            MobileScreenRoute::Session => rsx! {
                vmux_session::ui::Page {}
            },
            MobileScreenRoute::Start => {
                let team_runtime = runtime.clone();
                let disconnect_runtime = runtime.clone();
                rsx! {
                    StartScreen {
                        reachable: connection_view.reachable,
                        on_team: move |_| team_runtime.send(OpenTeam),
                        on_disconnect: move |_| disconnect_runtime.send(DisconnectRequest),
                    }
                }
            }
        },
    };
    rsx! {
        document::Title { {vmux_app::host::Product::name()} }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: TAILWIND_CSS }
        {content}
    }
}

#[component]
fn TeamScreen(on_back: EventHandler<()>) -> Element {
    rsx! {
        div { class: "flex h-dvh flex-col bg-background text-foreground",
            div { class: "flex items-center gap-1 border-b border-border px-2 pt-[env(safe-area-inset-top)]",
                button {
                    class: "rounded-lg px-3 py-2 text-sm text-muted-foreground active:bg-accent",
                    r#type: "button",
                    onclick: move |_| on_back.call(()),
                    {translate("mobile-chat-back")}
                }
            }
            div { class: "min-h-0 flex-1", vmux_team::ui::Page {} }
        }
    }
}

#[component]
fn StartScreen(
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
        div { class: "relative h-dvh bg-background",
            div { class: "flex h-full flex-col py-[calc(3rem+env(safe-area-inset-top))]",
                vmux_start::ui::Page {}
            }
            header { class: "pointer-events-none absolute inset-x-0 top-0 z-20 flex items-center gap-2 px-4 pb-3 pt-[calc(0.75rem+env(safe-area-inset-top))] sm:px-6",
                span { class: "text-sm font-semibold tracking-tight text-foreground", {vmux_app::host::Product::name()} }
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
