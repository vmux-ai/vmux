//! The navigation on a simulator, with no Mac: the tabs are canned rather than reported
//! over QUIC, so push, pop, the back-swipe and stacked sheets can be driven by hand.
//!
//! A page per route, so a webview per route: every tab root, every pushed level and every
//! sheet is its own document that UIKit animates in.
//!
//! The header and the tab bar are UIKit, not HTML, so iOS 26 draws them in Liquid Glass.
//! A bar button arrives back as a `Tapped` message, which is what `act` below reads.

use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Centre, NavPlugin, Present, Push, Report, Route, Tapped};
use vmux_mobile::navigator::{NavigationContainer, Screen, Sheet, TabNavigator, use_navigation};
use vmux_native::NativePage;

#[derive(Clone, PartialEq)]
enum Page {
    One,
    Two,
    Pushed(String),
    Presented(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    One,
    Two,
    Pushed,
    Presented,
}

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::One => Name::One,
            Self::Two => Name::Two,
            Self::Pushed(_) => Name::Pushed,
            Self::Presented(_) => Name::Presented,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::One => "Tab 1".to_string(),
            Self::Two => "Tab 2".to_string(),
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

.push {
  align-self: center; margin: auto 0; padding: 18px 44px; border-radius: 20px;
  font-size: 18px; font-weight: 600;
  background: linear-gradient(180deg, rgba(255,255,255,.24), rgba(255,255,255,.09));
  border: 1px solid rgba(255,255,255,.20);
  box-shadow: 0 10px 30px rgba(0,0,0,.5);
  transition: transform .12s ease, background .18s ease;
}
.push:active { transform: scale(.94); background: rgba(255,255,255,.3); }

.rungs { display: flex; gap: 5px; margin-top: 22px; }
.rung { height: 3px; flex: 1; border-radius: 2px; background: rgba(255,255,255,.13); }
.rung.on { background: rgba(255,255,255,.85); }
.rung.sheet { background: #ffcf6b; }

</style>"#;

static SHELL: NativePage = Demo::page("vmux://shell/", Shell);
static ONE: NativePage = Demo::page("vmux://one/", OneScreen);
static TWO: NativePage = Demo::page("vmux://two/", TwoScreen);
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
            ("tab:1".to_string(), Page::One),
            ("tab:2".to_string(), Page::Two),
        ],
        focused: Some("tab:1".to_string()),
    });
}

fn act(
    mut tapped: MessageReader<Tapped>,
    mut opened: Local<usize>,
    mut pushes: MessageWriter<Push<Page>>,
    mut presents: MessageWriter<Present<Page>>,
) {
    for Tapped(action) in tapped.read() {
        *opened += 1;
        let at = *opened;
        match *action {
            "Push" => {
                pushes.write(Push(Page::Pushed(format!("Level {at}"))));
            }
            "+" => {
                presents.write(Present(Page::Presented(format!("Sheet {at}"))));
            }
            _ => {}
        }
    }
}

#[component]
fn Shell() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            TabNavigator {
                Screen::<Page> { name: Name::One, draws: &ONE }
                Screen::<Page> { name: Name::Two, draws: &TWO }
                Screen::<Page> { name: Name::Pushed, draws: &PUSHED }
                Sheet::<Page> { name: Name::Presented, draws: &PRESENTED }
            }
        }
    }
}

#[component]
fn OneScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#1d4ed8", far: "#0891b2", kind: "tab root" }
        }
    }
}

#[component]
fn TwoScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#047857", far: "#0891b2", kind: "tab root" }
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
    let title = match &seen.current {
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
                button {
                    class: "push",
                    onclick: move |_| navigation.go(Page::Pushed(format!("Level {}", depth + 1))),
                    "Push"
                }
                div { class: "rungs",
                    for step in 0..6usize {
                        div {
                            key: "{step}",
                            class: if step >= depth { "rung" } else if sheet && step == depth - 1 { "rung on sheet" } else { "rung on" },
                        }
                    }
                }
            }
        }
    }
}
