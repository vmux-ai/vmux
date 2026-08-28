//! The navigation on a simulator, with no Mac: the tabs are canned rather than reported
//! over QUIC, so push, pop, the back-swipe and stacked sheets can be driven by hand.
//!
//! Three pages, so three webviews: the shell draws a tab's root, and every pushed level
//! and sheet is a page of its own that UIKit animates in.

use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{NavPlugin, Report, Route};
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

.bar {
  position: relative; display: flex; align-items: center; justify-content: space-between;
  padding: calc(env(safe-area-inset-top) + 8px) 12px 10px;
  background: rgba(8,10,16,.5); backdrop-filter: blur(28px) saturate(160%);
  border-bottom: 1px solid rgba(255,255,255,.07);
}
.bar .name { position: absolute; left: 50%; transform: translateX(-50%);
  font-size: 16px; font-weight: 600; letter-spacing: -.01em; pointer-events: none; }
.lead { display: flex; align-items: center; min-width: 0; }
.trail { display: flex; gap: 6px; }
.back { display: flex; align-items: center; gap: 3px; padding: 4px 8px 4px 2px;
  border-radius: 10px; color: #6ea8ff; font-size: 16px; }
.back::before { content: "‹"; font-size: 30px; line-height: 22px; font-weight: 300; }
.back:active { background: rgba(255,255,255,.10); }
.act { padding: 6px 12px; border-radius: 11px; font-size: 14px; font-weight: 500;
  background: rgba(255,255,255,.10); border: 1px solid rgba(255,255,255,.14);
  transition: transform .12s ease, background .18s ease; }
.act:active { transform: scale(.94); background: rgba(255,255,255,.2); }

.stage { flex: 1; display: flex; flex-direction: column; padding: 28px 24px; overflow: hidden; }
.eyebrow { font-size: 11px; letter-spacing: .18em; text-transform: uppercase; color: rgba(255,255,255,.45); }
.title { font: 600 40px/1.05 -apple-system, SF Pro Display, sans-serif; letter-spacing: -.02em; margin-top: 10px; }
.meta { margin-top: 10px; font-size: 13px; color: rgba(255,255,255,.55); font-variant-numeric: tabular-nums; }

.rungs { display: flex; gap: 5px; margin-top: 22px; }
.rung { height: 3px; flex: 1; border-radius: 2px; background: rgba(255,255,255,.13); }
.rung.on { background: rgba(255,255,255,.85); }
.rung.sheet { background: #ffcf6b; }

.tabbar {
  margin-top: auto; display: flex; padding: 8px 10px calc(env(safe-area-inset-bottom) + 6px);
  background: rgba(8,10,16,.5); backdrop-filter: blur(28px) saturate(160%);
  border-top: 1px solid rgba(255,255,255,.07);
}
.tab { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 5px;
  padding: 6px 4px; border-radius: 12px; font-size: 11px; font-weight: 500;
  color: rgba(255,255,255,.45); transition: color .2s ease; }
.tab .glyph { width: 20px; height: 20px; border-radius: 7px;
  border: 2px solid currentColor; opacity: .85; }
.tab.here { color: #fff; }
.tab.here .glyph { background: currentColor; }

.add {
  align-self: center; width: 52px; height: 36px; margin: 0 8px 2px; border-radius: 13px;
  display: flex; align-items: center; justify-content: center;
  font-size: 26px; font-weight: 300; line-height: 1; color: #fff;
  background: linear-gradient(180deg, rgba(255,255,255,.26), rgba(255,255,255,.09));
  border: 1px solid rgba(255,255,255,.20);
  box-shadow: 0 6px 18px rgba(0,0,0,.45);
  transition: transform .12s ease, background .18s ease;
}
.add:active { transform: scale(.92); background: rgba(255,255,255,.3); }
</style>"#;

static SHELL: NativePage = Demo::page("vmux://shell/", Shell);
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
                .add_systems(Startup, seed);
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

#[component]
fn Shell() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            TabNavigator {
                Screen::<Page> { name: Name::Note, draws: &NOTE }
                Sheet::<Page> { name: Name::Alert, draws: &ALERT }
                Root {}
            }
        }
    }
}

#[component]
fn Root() -> Element {
    rsx! {
        Stage { near: "#1d4ed8", far: "#0891b2", kind: "tab root", tabs: true, root: true }
    }
}

#[component]
fn NoteScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#7c3aed", far: "#db2777", kind: "pushed", tabs: true, root: false }
        }
    }
}

#[component]
fn AlertScreen() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            Stage { near: "#b45309", far: "#be123c", kind: "presented", tabs: true, root: false }
        }
    }
}

#[component]
fn Stage(near: String, far: String, kind: String, tabs: bool, root: bool) -> Element {
    let seen = use_navigation::<Page>().view();
    let shown = if root { &seen.root } else { &seen.current };
    let title = match shown {
        Some(route) => route.title(),
        None => "Nothing".to_string(),
    };
    let depth = seen.depth;
    let sheet = seen.sheet;

    rsx! {
        div { class: "screen", style: "--near:{near};--far:{far}",
            div { class: "vignette" }

            NavigationBar { title: title.clone(), back: !root, depth }

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

            if tabs {
                TabBar {}
            }
        }
    }
}

#[component]
fn NavigationBar(title: String, back: bool, depth: usize) -> Element {
    let navigation = use_navigation::<Page>();
    rsx! {
        header { class: "bar",
            div { class: "lead",
                if back {
                    button { class: "back", onclick: move |_| navigation.go_back(), "Back" }
                }
            }
            div { class: "name", "{title}" }
            div { class: "trail",
                Action {
                    label: "Push",
                    onpick: move |_| navigation.go(Page::Note(format!("Level {}", depth + 1))),
                }
            }
        }
    }
}

#[component]
fn TabBar() -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.view();
    let depth = seen.depth;
    let split = seen.tabs.len() / 2;
    rsx! {
        nav { class: "tabbar",
            for tab in seen.tabs.iter().take(split) {
                Tab {
                    key: "{tab.id}",
                    label: tab.name.clone(),
                    here: Some(&tab.id) == seen.selected.as_ref(),
                    onpick: {
                        let id = tab.id.clone();
                        move |_| navigation.navigate(&id)
                    },
                }
            }
            button {
                class: "add",
                onclick: move |_| navigation.go(Page::Alert(format!("Sheet {}", depth + 1))),
                "+"
            }
            for tab in seen.tabs.iter().skip(split) {
                Tab {
                    key: "{tab.id}",
                    label: tab.name.clone(),
                    here: Some(&tab.id) == seen.selected.as_ref(),
                    onpick: {
                        let id = tab.id.clone();
                        move |_| navigation.navigate(&id)
                    },
                }
            }
        }
    }
}

#[component]
fn Tab(label: String, here: bool, onpick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if here { "tab here" } else { "tab" },
            onclick: move |_| onpick.call(()),
            div { class: "glyph" }
            "{label}"
        }
    }
}

#[component]
fn Action(label: String, onpick: EventHandler<()>) -> Element {
    rsx! {
        button { class: "act", onclick: move |_| onpick.call(()), "{label}" }
    }
}
