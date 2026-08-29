//! The navigation on a simulator, with no Mac: tabs are canned rather than reported, so push,
//! the back-swipe, a dragged tab switch and every `presentation` can be driven by hand.

use bevy_app::{Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Depth, NavPlugin, Report, Route, Shows};
use vmux_mobile::{Router, Stack, Tabs, use_router};
use vmux_native::screen;

const BACKDROP: (u8, u8, u8, u8) = (10, 10, 10, 255);
const SEEDED: usize = 1;
const RUNGS: usize = 8;
const HEAD: usize = 3;
const CRUMBS: usize = 4;

#[derive(Clone, PartialEq, Route)]
enum Page {
    #[blank]
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
                .add_systems(Startup, setup)
                .add_systems(Update, chart);
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

#[screen(background = BACKDROP)]
#[component]
fn App() -> Element {
    rsx! {
        Stack {
            Tabs {
                TabScreen {}
                CardScreen {}
                ModalScreen {}
                SheetScreen {}
                FullScreen {}
            }
        }
    }
}

#[derive(Component, Clone, Default, PartialEq)]
struct Panel {
    hue: usize,
    kind: &'static str,
    title: String,
    trail: String,
    rungs: Vec<Step>,
    card: String,
    modal: String,
    sheet: String,
    full: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Spare,
    Gap(usize),
    Walked,
    Here,
    Lone,
}

fn chart(levels: Query<(Entity, &Shows<Page>, &Depth)>, mut commands: Commands) {
    let mut trail: Vec<(usize, Entity, Page)> = Vec::new();
    for (entity, shows, depth) in levels.iter() {
        trail.push((depth.0, entity, shows.0.clone()));
    }
    trail.sort_by_key(|(depth, _, _)| *depth);

    let mut routes = Vec::new();
    for (_, _, route) in &trail {
        routes.push(route.clone());
    }
    for ((_, entity, _), panel) in trail.iter().zip(Panel::over(&routes)) {
        commands.entity(*entity).insert(panel);
    }
}

impl Panel {
    fn over(trail: &[Page]) -> Vec<Panel> {
        let (mut cards, mut modals, mut sheets, mut fulls) = (0, 0, 0, 0);
        let mut crumbs = Vec::new();
        let mut panels = Vec::new();
        for (at, route) in trail.iter().enumerate() {
            let (hue, kind) = match route {
                Page::Tab(number) => ((number * 37 + 185) % 360, "tab root"),
                Page::Card(_) => (285, "card"),
                Page::Modal(_) => (30, "modal"),
                Page::FormSheet(_) => (175, "form sheet"),
                Page::FullScreenModal(_) => (255, "full screen modal"),
            };
            match route {
                Page::Card(_) => cards += 1,
                Page::Modal(_) => modals += 1,
                Page::FormSheet(_) => sheets += 1,
                Page::FullScreenModal(_) => fulls += 1,
                Page::Tab(_) => {}
            }
            crumbs.push(route.title());
            panels.push(Panel {
                hue,
                kind,
                title: route.title(),
                trail: String::new(),
                rungs: Self::rungs(trail.len() - 1, at),
                card: format!("Card {}", cards + 1),
                modal: format!("Modal {}", modals + 1),
                sheet: format!("Sheet {}", sheets + 1),
                full: format!("Full {}", fulls + 1),
            });
        }
        let trail = Self::elide(crumbs);
        for panel in &mut panels {
            panel.trail = trail.clone();
        }
        panels
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

    fn rungs(depth: usize, at: usize) -> Vec<Step> {
        let levels = depth + 1;
        let mut steps = Vec::new();
        if levels <= RUNGS {
            for level in 0..levels {
                steps.push(Some(level));
            }
        } else {
            for level in 0..HEAD {
                steps.push(Some(level));
            }
            steps.push(None);
            for level in (levels - (RUNGS - HEAD - 1))..levels {
                steps.push(Some(level));
            }
        }
        let mut rungs = Vec::new();
        for slot in 0..RUNGS {
            rungs.push(match steps.get(slot) {
                None => Step::Spare,
                Some(None) => Step::Gap(levels - (RUNGS - 1)),
                Some(Some(_)) if depth == 0 => Step::Lone,
                Some(Some(level)) if *level == at => Step::Here,
                Some(Some(_)) => Step::Walked,
            });
        }
        rungs
    }
}

fn use_panel() -> (Router<Page>, Panel) {
    let router = use_router::<Page>();
    (router, router.attached().unwrap_or_default())
}

#[screen(PageName::Tab)]
#[component]
fn TabScreen() -> Element {
    rsx! {
        Body {}
    }
}

#[screen(PageName::Card)]
#[component]
fn CardScreen() -> Element {
    rsx! {
        Body {}
    }
}

#[screen(PageName::Modal, presentation = Modal)]
#[component]
fn ModalScreen() -> Element {
    rsx! {
        Body {}
    }
}

#[screen(PageName::FormSheet, presentation = FormSheet, detents = [0.75, 1.0])]
#[component]
fn SheetScreen() -> Element {
    rsx! {
        Body {}
    }
}

#[screen(PageName::FullScreenModal, presentation = FullScreenModal)]
#[component]
fn FullScreen() -> Element {
    rsx! {
        Body {}
    }
}

#[component]
fn Body() -> Element {
    let (router, panel) = use_panel();
    let (hue, kind, title, trail) = (panel.hue, panel.kind, panel.title, panel.trail);
    let rungs = panel.rungs;

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
                        Rung { key: "{step}", step: *rung }
                    }
                }
                div { class: "mt-2 truncate text-sm text-muted-foreground", "{trail}" }

                div { class: "my-auto flex flex-wrap justify-center gap-2",
                    Button {
                        label: "Card",
                        onpress: move |_| router.push(Page::Card(panel.card.clone())),
                    }
                    Button {
                        label: "Modal",
                        onpress: move |_| router.push(Page::Modal(panel.modal.clone())),
                    }
                    Button {
                        label: "Form Sheet",
                        onpress: move |_| {
                            router.push(Page::FormSheet(panel.sheet.clone()))
                        },
                    }
                    Button {
                        label: "Full Screen Modal",
                        onpress: move |_| {
                            router.push(Page::FullScreenModal(panel.full.clone()))
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn Rung(step: Step) -> Element {
    let (tone, label) = match step {
        Step::Spare => ("mx-0 h-1 w-0 flex-none bg-border opacity-0", String::new()),
        Step::Gap(hidden) => (
            "mx-1.5 flex-none text-xs font-medium leading-none text-muted-foreground",
            format!("+{hidden}"),
        ),
        Step::Lone => ("mx-0.5 h-1 flex-1 bg-border", String::new()),
        Step::Here => ("mx-0.5 h-1 flex-1 bg-chart-3", String::new()),
        Step::Walked => ("mx-0.5 h-1 flex-1 bg-foreground", String::new()),
    };
    rsx! {
        div { class: "rounded-sm transition-all duration-300 ease-out {tone}", "{label}" }
    }
}

#[component]
fn Button(label: String, onpress: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "rounded-lg border border-border bg-card px-4 py-3 text-base font-medium backdrop-blur-lg transition active:scale-95 active:bg-accent",
            onclick: move |_| onpress.call(()),
            "{label}"
        }
    }
}
