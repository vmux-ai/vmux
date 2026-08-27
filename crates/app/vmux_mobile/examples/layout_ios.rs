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
<meta name="color-scheme" content="light dark"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; font: 16px -apple-system, sans-serif; }
body { display: flex; flex-direction: column; overflow: hidden; background: #101014; color: #e8e8ea; }
button { font: inherit; color: inherit; }
</style>"#,
    html_attributes: r#"lang="en" class="h-full""#,
    body_class: "",
    transparent: false,
    owns_subtree: false,
};

fn main() {
    App::new()
        .add_plugins(MobilePlugin::showing(&DEMO_PAGE))
        .add_plugins(NavPlugin::<Page>::default())
        .add_systems(Startup, seed)
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
                Screen::<Page> { name: Name::Inbox, Body { tint: "#1b2a4a" } }
                Screen::<Page> { name: Name::Note, Body { tint: "#2a1b3d" } }
            }
        }
    }
}

#[component]
fn Body(tint: String) -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.view();
    let title = match navigation.route() {
        Some(route) => route.title(),
        None => "nothing".to_string(),
    };
    let depth = seen.depth;
    let kind = if seen.sheet { "sheet" } else { "screen" };

    rsx! {
        div {
            style: "flex:1;display:flex;flex-direction:column;background:{tint};padding:calc(env(safe-area-inset-top) + 24px) 20px 20px",
            div { style: "font-size:26px;font-weight:600", "{title}" }
            div { style: "opacity:.6;margin-top:4px", "{kind} · depth {depth}" }

            div { style: "display:flex;flex-wrap:wrap;gap:10px;margin-top:28px",
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

            div { style: "margin-top:auto;display:flex;gap:8px",
                for tab in seen.tabs.iter() {
                    Tap {
                        key: "{tab.id}",
                        label: tab.name.clone(),
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
fn Tap(label: String, onpick: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "border:0;border-radius:12px;padding:12px 18px;background:rgba(255,255,255,.12)",
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}
