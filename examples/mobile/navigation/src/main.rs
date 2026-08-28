//! The navigation on a simulator, with no Mac: tabs are canned rather than reported, so push,
//! the back-swipe, a dragged tab switch and every `presentation` can be driven by hand.

use bevy_app::{Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Centre, NavPlugin, OpenBlank, Presentation, Report, Route, Tapped};
use vmux_mobile::{Router, Screen, Stack, Tabs, use_router};
use vmux_native::screen;

const BACKDROP: (u8, u8, u8, u8) = (10, 10, 10, 255);
const SEEDED: usize = 1;
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

struct Demo {
    router: Router<Page>,
    hue: usize,
    kind: &'static str,
    title: String,
    trail: String,
    rungs: Vec<(&'static str, String)>,
    card: String,
    modal: String,
    sheet: String,
    full: String,
}

fn use_demo() -> Demo {
    Demo::of(use_router::<Page>())
}

impl Demo {
    fn of(router: Router<Page>) -> Self {
        let here = router.route();
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
        let (mut cards, mut modals, mut sheets, mut full_screens) = (0, 0, 0, 0);
        let mut crumbs = Vec::new();
        for route in router.segments() {
            match route {
                Page::Card(_) => cards += 1,
                Page::Modal(_) => modals += 1,
                Page::FormSheet(_) => sheets += 1,
                Page::FullScreenModal(_) => full_screens += 1,
                Page::Tab(_) => {}
            }
            crumbs.push(route.title());
        }
        Self {
            router,
            hue,
            kind,
            title,
            trail: Self::elide(crumbs),
            rungs: Self::rungs(router.depth(), router.position()),
            card: format!("Card {}", cards + 1),
            modal: format!("Modal {}", modals + 1),
            sheet: format!("Sheet {}", sheets + 1),
            full: format!("Full {}", full_screens + 1),
        }
    }

    fn elide(mut crumbs: Vec<String>) -> String {
        if crumbs.len() > CRUMBS {
            let tail = crumbs.split_off(crumbs.len() - (CRUMBS - 1));
            crumbs.truncate(1);
            crumbs.push("\u{2026}".to_string());
            for crumb in tail {
                crumbs.push(crumb);
            }
        }
        crumbs.join(" \u{203a} ")
    }

    fn rungs(depth: usize, at: usize) -> Vec<(&'static str, String)> {
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
                rungs.push(("mx-0 h-1 w-0 flex-none bg-border opacity-0", String::new()));
                continue;
            };
            let Some(level) = level else {
                rungs.push((
                    "mx-1.5 flex-none text-xs font-medium leading-none text-muted-foreground",
                    format!("+{}", levels - (RUNGS - 1)),
                ));
                continue;
            };
            if depth == 0 {
                rungs.push(("mx-0.5 h-1 flex-1 bg-border", String::new()));
            } else if *level == at {
                rungs.push(("mx-0.5 h-1 flex-1 bg-chart-3", String::new()));
            } else {
                rungs.push(("mx-0.5 h-1 flex-1 bg-foreground", String::new()));
            }
        }
        rungs
    }
}

#[screen]
#[component]
fn TabScreen() -> Element {
    let demo = use_demo();
    let (hue, kind, title, trail) = (demo.hue, demo.kind, demo.title.clone(), demo.trail.clone());
    let rungs = demo.rungs.clone();
    let router = demo.router;

    rsx! {
    div {
        class: "relative isolate flex min-h-0 flex-1 flex-col overflow-hidden bg-background text-foreground",
            style: "--near:hsl({hue} 78% 42%);--far:hsl({(hue + 40) % 360} 74% 38%)",

            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-2xl bg-[radial-gradient(60%_60%_at_30%_20%,var(--near),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-3xl bg-[radial-gradient(50%_50%_at_80%_0%,var(--far),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(120%_90%_at_50%_0%,transparent_40%,rgba(0,0,0,0.75)_100%)]" }

            div { class: "flex min-h-0 flex-1 flex-col overflow-hidden px-6 pt-[calc(env(safe-area-inset-top)+1.5rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)]",
                div { class: "text-xs uppercase tracking-widest text-muted-foreground", "{kind}" }
                div { class: "mt-2 text-4xl font-semibold leading-tight tracking-tight", "{title}" }
                div { class: "-mx-0.5 mt-4 flex h-4 items-center",
                    for (step , rung) in rungs.iter().enumerate() {
                        Rung { key: "{step}", tone: rung.0, label: rung.1.clone() }
                    }
                }
                div { class: "mt-2 truncate text-sm text-muted-foreground", "{trail}" }

                div { class: "my-auto flex flex-wrap justify-center gap-2",
                    Key {
                        label: "Card",
                        onpick: move |_| router.push(Page::Card(demo.card.clone())),
                    }
                    Key {
                        label: "Modal",
                        onpick: move |_| router.push(Page::Modal(demo.modal.clone())),
                    }
                    Key {
                        label: "Form Sheet",
                        onpick: move |_| {
                            router.push(Page::FormSheet(demo.sheet.clone()))
                        },
                    }
                    Key {
                        label: "Full Screen Modal",
                        onpick: move |_| {
                            router.push(Page::FullScreenModal(demo.full.clone()))
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn Rung(tone: &'static str, label: String) -> Element {
    rsx! {
        div { class: "rounded-sm transition-all duration-300 ease-out {tone}", "{label}" }
    }
}

#[component]
fn Key(label: String, onpick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "rounded-lg border border-border bg-card px-4 py-3 text-base font-medium backdrop-blur-lg transition active:scale-95 active:bg-accent",
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}
