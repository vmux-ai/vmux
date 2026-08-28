//! The navigation on a simulator, with no Mac: the tabs are canned rather than reported
//! over QUIC, so push, pop, the back-swipe and stacked sheets can be driven by hand.
//!
//! A page per route, so a webview per route: every tab root, every pushed level and every
//! sheet is its own document that UIKit animates in.
//!
//! The header and the tab bar are UIKit, not HTML, so iOS 26 draws them in Liquid Glass.
//! A bar button arrives back as a `Tapped` message, which is what `act` below reads.
//!
//! How a route arrives is an option on its `Screen`, the way Expo Router spells it: a card
//! pushes, a form sheet slides up over its detents. Callers only ever say `go(route)`.

use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::Presentation;
use vmux_mobile::nav::{Centre, NavPlugin, OpenBlank, Report, Route, Tapped};
use vmux_mobile::navigator::{
    NavigationContainer, Screen, TabNavigator, use_navigation, use_route,
};
use vmux_native::NativePage;

#[derive(Clone, PartialEq)]
enum Page {
    Tab(usize),
    Pushed(String),
    Presented(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    Tab,
    Pushed,
    Presented,
}

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::Tab(_) => Name::Tab,
            Self::Pushed(_) => Name::Pushed,
            Self::Presented(_) => Name::Presented,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Tab(at) => format!("Tab {at}"),
            Self::Pushed(name) | Self::Presented(name) => name.clone(),
        }
    }
}

const HEAD: &str = r#"<base href="/"/>
<title>Layout</title>
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"/>
<meta name="color-scheme" content="dark"/>
<style>
html, body { height: 100%; margin: 0; background: #05060a; color: #f2f3f7; }
body {
  display: flex; flex-direction: column; overflow: hidden;
  font: 400 16px/1.4 -apple-system, SF Pro Text, sans-serif;
  -webkit-font-smoothing: antialiased;
}
button { font: inherit; color: inherit; border: 0; background: none; }
#main { display: flex; flex-direction: column; flex: 1; min-height: 0; }

.screen { position: relative; flex: 1; display: flex; flex-direction: column;
  isolation: isolate; overflow: hidden; }
.screen::before, .screen::after { content: ""; position: absolute; inset: -30% -30% auto -30%; height: 90%; z-index: -1; }
.screen::before { background: radial-gradient(60% 60% at 30% 20%, var(--near), transparent 70%); filter: blur(40px); }
.screen::after  { background: radial-gradient(50% 50% at 80% 0%, var(--far), transparent 70%); filter: blur(60px); }
.vignette { position: absolute; inset: 0; z-index: -1;
  background: radial-gradient(120% 90% at 50% 0%, transparent 40%, rgba(0,0,0,.75) 100%); }

.stage { flex: 1; display: flex; flex-direction: column; overflow: hidden;
  padding: calc(env(safe-area-inset-top) + 24px) 24px calc(env(safe-area-inset-bottom) + 24px); }
.eyebrow { font-size: 11px; letter-spacing: .18em; text-transform: uppercase; color: rgba(255,255,255,.45); }
.title { font: 600 40px/1.05 -apple-system, SF Pro Display, sans-serif; letter-spacing: -.02em; margin-top: 10px; }
.meta { margin-top: 10px; font-size: 13px; color: rgba(255,255,255,.55); font-variant-numeric: tabular-nums; }

.keys { display: flex; gap: 10px; justify-content: center; margin: auto 0; }
.key {
  padding: 11px 22px; border-radius: 13px; font-size: 15px; font-weight: 500;
  background: rgba(255,255,255,.10); border: 1px solid rgba(255,255,255,.16);
  backdrop-filter: blur(18px);
  transition: transform .12s ease, background .18s ease;
}
.key:active { transform: scale(.94); background: rgba(255,255,255,.22); }

.rungs { display: flex; gap: 3px; margin-top: 22px; height: 3px; }
.rung { flex: 1; min-width: 1px; border-radius: 2px; background: rgba(255,255,255,.85); }
.rung.sheet { background: #ffcf6b; }

</style>"#;

static SHELL: NativePage = Demo::page("vmux://shell/", Shell);
static TAB: NativePage = Demo::page("vmux://tab/", TabScreen);
static PUSHED: NativePage = Demo::page("vmux://pushed/", PushedScreen);
static PRESENTED: NativePage = Demo::page("vmux://presented/", PresentedScreen);

struct Demo;

impl Demo {
    const fn page(url: &'static str, component: vmux_native::PageComponent) -> NativePage {
        NativePage {
            url,
            document_url: None,
            component,
            root_id: "main",
            root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
            head: HEAD,
            html_attributes: r#"lang="en" class="h-full""#,
            body_class: "",
            transparent: false,
            background: Some((5, 6, 10, 255)),
            owns_subtree: false,
        }
    }
}

fn main() {
    App::new()
        .add_plugins(MobilePlugin::showing(&SHELL).serving(|world| {
            world
                .add_plugins(NavPlugin::<Page>::default())
                .insert_resource(Centre("+"))
                .add_systems(Startup, seed)
                .add_systems(Update, act);
        }))
        .run();
}

fn seed(mut reported: MessageWriter<Report<Page>>) {
    reported.write(Report {
        tabs: vec![
            ("tab:1".to_string(), Page::Tab(1)),
            ("tab:2".to_string(), Page::Tab(2)),
        ],
        focused: Some("tab:1".to_string()),
    });
}

fn act(
    mut tapped: MessageReader<Tapped>,
    mut opened: Local<usize>,
    mut blanks: MessageWriter<OpenBlank<Page>>,
) {
    for Tapped(action) in tapped.read() {
        if *action != "+" {
            continue;
        }
        *opened += 1;
        blanks.write(OpenBlank(Page::Tab(*opened + 2)));
    }
}

#[component]
fn Shell() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            TabNavigator {
                Screen::<Page> { name: Name::Tab, draws: &TAB }
                Screen::<Page> { name: Name::Pushed, draws: &PUSHED }
                Screen::<Page> {
                    name: Name::Presented,
                    draws: &PRESENTED,
                    presentation: Presentation::FormSheet,
                    detents: &[0.5, 1.0],
                }
            }
        }
    }
}

#[component]
fn TabScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            TabStage {}
        }
    }
}

#[component]
fn TabStage() -> Element {
    let at = match use_route::<Page>() {
        Some(Page::Tab(at)) => at,
        _ => 1,
    };
    let hue = (at * 37 + 185) % 360;
    rsx! {
        Stage {
            near: "hsl({hue} 78% 42%)",
            far: "hsl({(hue + 40) % 360} 74% 38%)",
            kind: "tab root",
        }
    }
}

#[component]
fn PushedScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#7c3aed", far: "#db2777", kind: "pushed" }
        }
    }
}

#[component]
fn PresentedScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#b45309", far: "#be123c", kind: "presented" }
        }
    }
}

#[component]
fn Stage(near: String, far: String, kind: String) -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.view();
    let title = match use_route::<Page>() {
        Some(route) => route.title(),
        None => "Nothing".to_string(),
    };
    let depth = seen.depth;
    let sheet = seen.sheet;

    rsx! {
        div { class: "screen", style: "--near:{near};--far:{far}",
            div { class: "vignette" }

            div { class: "stage",
                div { class: "eyebrow", "{kind}" }
                div { class: "title", "{title}" }
                div { class: "meta", "depth {depth} · {seen.tabs.len()} tabs open" }
                div { class: "keys",
                    button {
                        class: "key",
                        onclick: move |_| navigation.go(Page::Pushed(format!("Level {}", depth + 1))),
                        "Stack"
                    }
                    button {
                        class: "key",
                        onclick: move |_| navigation.go(Page::Presented(format!("Sheet {}", depth + 1))),
                        "Sheet"
                    }
                }
                div { class: "rungs",
                    for step in 0..depth {
                        div {
                            key: "{step}",
                            class: if sheet && step == depth - 1 { "rung sheet" } else { "rung" },
                        }
                    }
                }
            }
        }
    }
}
