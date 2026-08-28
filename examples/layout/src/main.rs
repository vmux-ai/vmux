//! The navigation on a simulator, with no Mac: the tabs are canned rather than reported
//! over QUIC, so push, pop, the back-swipe and stacked sheets can be driven by hand.
//!
//! Three pages, so three webviews: the shell draws a tab's root, and every pushed level
//! and sheet is a page of its own that UIKit animates in.
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
    Inbox,
    Note(String),
    Alert(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    Inbox,
    Note,
    Alert,
}

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::Inbox => Name::Inbox,
            Self::Note(_) => Name::Note,
            Self::Alert(_) => Name::Alert,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Inbox => "Inbox".to_string(),
            Self::Note(name) | Self::Alert(name) => name.clone(),
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

.rungs { display: flex; gap: 5px; margin-top: 22px; }
.rung { height: 3px; flex: 1; border-radius: 2px; background: rgba(255,255,255,.13); }
.rung.on { background: rgba(255,255,255,.85); }
.rung.sheet { background: #ffcf6b; }

</style>"#;

static SHELL: NativePage = Demo::page("vmux://shell/", Shell);
static INBOX: NativePage = Demo::page("vmux://inbox/", InboxScreen);
static NOTE: NativePage = Demo::page("vmux://note/", NoteScreen);
static ALERT: NativePage = Demo::page("vmux://alert/", AlertScreen);

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
            ("inbox".to_string(), Page::Inbox),
            ("notes".to_string(), Page::Note("Notes".to_string())),
        ],
        focused: Some("notes".to_string()),
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
                pushes.write(Push(Page::Note(format!("Level {at}"))));
            }
            "+" => {
                presents.write(Present(Page::Alert(format!("Sheet {at}"))));
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
                Screen::<Page> { name: Name::Inbox, draws: &INBOX, action: "Push" }
                Screen::<Page> { name: Name::Note, draws: &NOTE, action: "Push" }
                Sheet::<Page> { name: Name::Alert, draws: &ALERT, action: "Push" }
            }
        }
    }
}

#[component]
fn InboxScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#1d4ed8", far: "#0891b2", kind: "inbox" }
        }
    }
}

#[component]
fn NoteScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#7c3aed", far: "#db2777", kind: "notes" }
        }
    }
}

#[component]
fn AlertScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#b45309", far: "#be123c", kind: "presented" }
        }
    }
}

#[component]
fn Stage(near: String, far: String, kind: String) -> Element {
    let seen = use_navigation::<Page>().view();
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
