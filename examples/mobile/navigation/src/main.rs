//! The navigation on a simulator, with no Mac: tabs are canned rather than reported, so push,
//! the back-swipe, a dragged tab switch and every `presentation` can be driven by hand.

use bevy_app::{Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Centre, NavPlugin, OpenBlank, Presentation, Report, Route, Tapped};
use vmux_mobile::navigator::{Screen, Stack, Tabs, use_navigation, use_route};
use vmux_native::screen;

const BACKDROP: (u8, u8, u8, u8) = (5, 6, 10, 255);
const SEEDED: usize = 2;
const RUNGS: usize = 8;
const HEAD: usize = 3;
const CRUMBS: usize = 4;
const PLUS: &str = "+";

#[derive(Clone, PartialEq, Route)]
enum Page {
    #[route("Tab {0}")]
    Tab(usize),
    #[route("{0}")]
    Card(String),
    #[route("{0}")]
    Modal(String),
    #[route("{0}")]
    FormSheet(String),
    #[route("{0}")]
    FullScreenModal(String),
}

fn main() {
    bevy_app::App::new()
        .add_plugins(MobilePlugin::showing(&APP).serving(|world| {
            world
                .add_plugins(NavPlugin::<Page>::default())
                .insert_resource(Centre(PLUS))
                .add_systems(Startup, setup)
                .add_systems(Update, open_new_tab);
        }))
        .run();
}

fn setup(mut reported: MessageWriter<Report<Page>>) {
    let mut tabs = Vec::new();
    for at in 1..=SEEDED {
        tabs.push((format!("tab:{at}"), Page::Tab(at)));
    }
    reported.write(Report {
        tabs,
        focused: Some("tab:1".to_string()),
    });
}

fn open_new_tab(
    mut tapped: MessageReader<Tapped>,
    mut opened: Local<usize>,
    mut opening: MessageWriter<OpenBlank<Page>>,
) {
    for Tapped(action) in tapped.read() {
        if *action != PLUS {
            continue;
        }
        *opened += 1;
        opening.write(OpenBlank(Page::Tab(SEEDED + *opened)));
    }
}

#[screen(background = BACKDROP)]
#[component]
fn App() -> Element {
    rsx! {
        Stack::<Page> {
            Tabs {
                Screen::<Page> { name: PageName::Tab, component: &TAB_SCREEN }
                Screen::<Page> { name: PageName::Card, component: &TAB_SCREEN }
                Screen::<Page> {
                    name: PageName::Modal,
                    component: &TAB_SCREEN,
                    presentation: Presentation::Modal,
                }
                Screen::<Page> {
                    name: PageName::FormSheet,
                    component: &TAB_SCREEN,
                    presentation: Presentation::FormSheet,
                    detents: &[0.75, 1.0],
                }
                Screen::<Page> {
                    name: PageName::FullScreenModal,
                    component: &TAB_SCREEN,
                    presentation: Presentation::FullScreenModal,
                }
            }
        }
    }
}

#[screen]
#[component]
fn TabScreen() -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.state();
    let here = use_route::<Page>();
    let (hue, kind) = match &here {
        Some(Page::Tab(at)) => ((at * 37 + 185) % 360, "tab root"),
        Some(Page::Card(_)) => (285, "card"),
        Some(Page::Modal(_)) => (30, "modal"),
        Some(Page::FormSheet(_)) => (175, "form sheet"),
        Some(Page::FullScreenModal(_)) => (255, "full screen modal"),
        None => (185, "nowhere"),
    };
    let title = match &here {
        Some(route) => route.title(),
        None => "Nothing".to_string(),
    };
    let depth = seen.depth;
    let mut at = depth;
    let (mut cards, mut modals, mut sheets, mut full_screens) = (0, 0, 0, 0);
    for (index, route) in seen.trail.iter().enumerate() {
        if Some(route) == here.as_ref() {
            at = index;
        }
        match route {
            Page::Card(_) => cards += 1,
            Page::Modal(_) => modals += 1,
            Page::FormSheet(_) => sheets += 1,
            Page::FullScreenModal(_) => full_screens += 1,
            Page::Tab(_) => {}
        }
    }
    let mut trail = Vec::new();
    for route in &seen.trail {
        trail.push(route.title());
    }
    if trail.len() > CRUMBS {
        let tail = trail.split_off(trail.len() - (CRUMBS - 1));
        let hidden = trail.len() - 1;
        trail.truncate(1);
        trail.push(format!("+{hidden}"));
        for crumb in tail {
            trail.push(crumb);
        }
    }
    let trail = trail.join(" \u{203a} ");

    let levels = depth + 1;
    let mut slots = Vec::new();
    if levels <= RUNGS {
        for level in 0..levels {
            slots.push(Some(level));
        }
    } else {
        for level in 0..HEAD {
            slots.push(Some(level));
        }
        slots.push(None);
        for level in (levels - (RUNGS - HEAD - 1))..levels {
            slots.push(Some(level));
        }
    }
    let mut rungs = Vec::new();
    for slot in 0..RUNGS {
        let Some(level) = slots.get(slot) else {
            rungs.push("mx-0 w-0 flex-none bg-white/20 opacity-0");
            continue;
        };
        let Some(level) = level else {
            rungs.push("mx-[1.5px] w-[6px] flex-none bg-white/25");
            continue;
        };
        if depth == 0 {
            rungs.push("mx-[1.5px] flex-1 bg-white/20");
        } else if *level == at {
            rungs.push("mx-[1.5px] flex-1 bg-[#ffcf6b]");
        } else {
            rungs.push("mx-[1.5px] flex-1 bg-white/85");
        }
    }

    rsx! {
    div {
        class: "relative isolate flex min-h-0 flex-1 flex-col overflow-hidden bg-[#05060a] text-white",
            style: "--near:hsl({hue} 78% 42%);--far:hsl({(hue + 40) % 360} 74% 38%)",

            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-[40px] bg-[radial-gradient(60%_60%_at_30%_20%,var(--near),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-[60px] bg-[radial-gradient(50%_50%_at_80%_0%,var(--far),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(120%_90%_at_50%_0%,transparent_40%,rgba(0,0,0,0.75)_100%)]" }

            div { class: "flex min-h-0 flex-1 flex-col overflow-hidden px-6 pt-[calc(env(safe-area-inset-top)+1.5rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)]",
                div { class: "text-[11px] uppercase tracking-[0.18em] text-white/45", "{kind}" }
                div { class: "mt-2.5 text-[40px] font-semibold leading-[1.05] tracking-[-0.02em]",
                    "{title}"
                }
                div { class: "-mx-[1.5px] mt-4 flex h-[3px]",
                    for (step , tone) in rungs.iter().enumerate() {
                        div {
                            key: "{step}",
                            class: "rounded-sm transition-all duration-300 ease-out {tone}",
                        }
                    }
                }
                div { class: "mt-2.5 truncate text-[13px] text-white/55", "{trail}" }

                div { class: "my-auto flex flex-wrap justify-center gap-2",
                    Key {
                        label: "Card",
                        onpick: move |_| navigation.go(Page::Card(format!("Card {}", cards + 1))),
                    }
                    Key {
                        label: "Modal",
                        onpick: move |_| navigation.go(Page::Modal(format!("Modal {}", modals + 1))),
                    }
                    Key {
                        label: "Form Sheet",
                        onpick: move |_| {
                            navigation.go(Page::FormSheet(format!("Sheet {}", sheets + 1)))
                        },
                    }
                    Key {
                        label: "Full Screen Modal",
                        onpick: move |_| {
                            navigation.go(Page::FullScreenModal(format!(
                                "Full {}",
                                full_screens + 1
                            )))
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn Key(label: String, onpick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "rounded-[13px] border border-white/15 bg-white/10 px-[18px] py-[11px] text-[15px] font-medium backdrop-blur-lg transition active:scale-95 active:bg-white/20",
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}
