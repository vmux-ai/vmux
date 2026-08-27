//! The navigation on a simulator, with no Mac: the tabs are canned rather than reported
//! over QUIC, so push, pop, the back-swipe and stacked sheets can be driven by hand.

use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{NavPlugin, Report, Route};
use vmux_mobile::navigator::{NavigationContainer, Screen, TabNavigator, use_navigation};
use vmux_native::NativePage;

#[derive(Clone, PartialEq)]
enum Page {
    Inbox,
    Note(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    Inbox,
    Note,
}

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::Inbox => Name::Inbox,
            Self::Note(_) => Name::Note,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Inbox => "Inbox".to_string(),
            Self::Note(name) => name.clone(),
        }
    }
}

static DEMO_PAGE: NativePage = NativePage {
    url: "vmux://shell/",
    document_url: None,
    component: Demo,
    root_id: "main",
    root_class: "flex min-h-0 min-w-0 flex-1 flex-col",
    head: r#"<base href="/"/>
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

.stage {
  position: relative; flex: 1; display: flex; flex-direction: column;
  padding: calc(env(safe-area-inset-top) + 32px) 24px calc(env(safe-area-inset-bottom) + 20px);
  isolation: isolate; overflow: hidden;
}
.stage::before, .stage::after { content: ""; position: absolute; inset: -30% -30% auto -30%; height: 90%; z-index: -1; }
.stage::before { background: radial-gradient(60% 60% at 30% 20%, var(--near), transparent 70%); filter: blur(40px); }
.stage::after  { background: radial-gradient(50% 50% at 80% 0%, var(--far), transparent 70%); filter: blur(60px); }
.vignette { position: absolute; inset: 0; z-index: -1;
  background: radial-gradient(120% 90% at 50% 0%, transparent 40%, rgba(0,0,0,.75) 100%); }

.eyebrow { font-size: 11px; letter-spacing: .18em; text-transform: uppercase; color: rgba(255,255,255,.45); }
.title { font: 600 40px/1.05 -apple-system, SF Pro Display, sans-serif; letter-spacing: -.02em; margin-top: 10px; }
.meta { margin-top: 10px; font-size: 13px; color: rgba(255,255,255,.55); font-variant-numeric: tabular-nums; }

.rungs { display: flex; gap: 5px; margin-top: 22px; }
.rung { height: 3px; flex: 1; border-radius: 2px; background: rgba(255,255,255,.13); }
.rung.on { background: rgba(255,255,255,.85); }
.rung.sheet { background: #ffcf6b; }

.actions { display: flex; gap: 10px; margin-top: 30px; flex-wrap: wrap; }
.key {
  padding: 13px 20px; border-radius: 14px; font-weight: 500;
  background: rgba(255,255,255,.10); border: 1px solid rgba(255,255,255,.14);
  backdrop-filter: blur(18px); transition: transform .12s ease, background .18s ease;
}
.key:active { transform: scale(.95); background: rgba(255,255,255,.2); }

.tabs {
  margin-top: auto; display: flex; gap: 6px; padding: 6px; border-radius: 20px;
  background: rgba(12,14,22,.6); border: 1px solid rgba(255,255,255,.10);
  backdrop-filter: blur(28px) saturate(160%);
}
.tab { flex: 1; padding: 11px 14px; border-radius: 15px; font-size: 14px;
  color: rgba(255,255,255,.55); transition: background .2s ease, color .2s ease; }
.tab.here { background: rgba(255,255,255,.14); color: #fff; font-weight: 600; }
</style>"#,
    html_attributes: r#"lang="en" class="h-full""#,
    body_class: "",
    transparent: false,
    owns_subtree: false,
};

fn main() {
    App::new()
        .add_plugins(MobilePlugin::showing(&DEMO_PAGE).serving(|world| {
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
fn Demo() -> Element {
    rsx! {
        NavigationContainer::<Page> {
            TabNavigator {
                Screen::<Page> { name: Name::Inbox, Body { near: "#1d4ed8", far: "#0891b2" } }
                Screen::<Page> { name: Name::Note, Body { near: "#7c3aed", far: "#db2777" } }
            }
        }
    }
}

#[component]
fn Body(near: String, far: String) -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.view();
    let title = match navigation.route() {
        Some(route) => route.title(),
        None => "Nothing".to_string(),
    };
    let depth = seen.depth;
    let sheet = seen.sheet;

    rsx! {
        div { class: "stage", style: "--near:{near};--far:{far}",
            div { class: "vignette" }
            div { class: "eyebrow", if sheet { "presented" } else { "pushed" } }
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

            div { class: "actions",
                Tap {
                    label: "Push",
                    onpick: move |_| navigation.push(Page::Note(format!("Level {}", depth + 1))),
                }
                Tap {
                    label: "Sheet",
                    onpick: move |_| navigation.present(Page::Note(format!("Sheet {}", depth + 1))),
                }
                Tap { label: "Back", onpick: move |_| navigation.go_back() }
            }

            div { class: "tabs",
                for tab in seen.tabs.iter() {
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
}

#[component]
fn Tab(label: String, here: bool, onpick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if here { "tab here" } else { "tab" },
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}

#[component]
fn Tap(label: String, onpick: EventHandler<()>) -> Element {
    rsx! {
        button { class: "key", onclick: move |_| onpick.call(()), "{label}" }
    }
}
