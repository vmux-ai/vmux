//! The navigation on a simulator, with no Mac: tabs are canned rather than reported, so push,
//! the back-swipe, a dragged tab switch and every `presentation` can be driven by hand.

use bevy_app::{Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Depth, NavPlugin, Report, Route, Shows, Trail};
use vmux_mobile::{Router, Stack, Tabs, use_router};
use vmux_native::{screen, screens};

const BACKDROP: (u8, u8, u8, u8) = (10, 10, 10, 255);
const SEEDED: usize = 1;
const RUNGS: usize = 8;
const HEAD: usize = 3;
const CRUMBS: usize = 4;
const HUES: usize = 360;
const TAB_HUE: usize = 185;
const TAB_STEP: usize = 37;
const CARD_HUE: usize = 285;
const MODAL_HUE: usize = 30;
const SHEET_HUE: usize = 175;
const FULL_HUE: usize = 255;

fn main() {
    bevy_app::App::new()
        .add_plugins(MobilePlugin::showing(&APP).serving(|world| {
            world
                .add_plugins(NavPlugin::<Page>::default())
                .add_systems(Startup, setup)
                .add_systems(Update, (sketch, describe, tally, trace).chain());
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
                Tab {}
                Card {}
                Modal {}
                FormSheet {}
                FullScreenModal {}
            }
        }
    }
}

#[derive(Component, Clone, Default, PartialEq)]
struct Look {
    hue: usize,
    kind: &'static str,
    title: String,
}

#[derive(Component, Clone, Default, PartialEq)]
struct Ahead {
    card: String,
    modal: String,
    sheet: String,
    full: String,
}

#[derive(Component, Clone, Default, PartialEq)]
struct Crumbs(String);

#[derive(Component, Clone, Default, PartialEq)]
struct Rungs(Vec<Step>);

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Spare,
    Gap(usize),
    Walked,
    Here,
    Lone,
}

type Unseen = (With<Depth>, With<Shows<Page>>, Without<Look>);

fn sketch(fresh: Query<Entity, Unseen>, mut commands: Commands) {
    for entity in fresh.iter() {
        commands.entity(entity).insert((
            Look::default(),
            Ahead::default(),
            Crumbs::default(),
            Rungs::default(),
        ));
    }
}

fn describe(mut levels: Query<(&Shows<Page>, &mut Look)>) {
    for (shows, mut look) in levels.iter_mut() {
        let (hue, kind) = match &shows.0 {
            Page::Tab(number) => ((TAB_HUE + number * TAB_STEP) % HUES, "tab root"),
            Page::Card(_) => (CARD_HUE, "card"),
            Page::Modal(_) => (MODAL_HUE, "modal"),
            Page::FormSheet(_) => (SHEET_HUE, "form sheet"),
            Page::FullScreenModal(_) => (FULL_HUE, "full screen modal"),
        };
        let title = shows.0.title();
        if (look.hue, look.kind, &look.title) != (hue, kind, &title) {
            look.hue = hue;
            look.kind = kind;
            look.title = title;
        }
    }
}

fn tally(trail: Res<Trail>, routes: Query<&Shows<Page>>, mut ahead: Query<&mut Ahead>) {
    let (mut cards, mut modals, mut sheets, mut fulls) = (0, 0, 0, 0);
    for entity in trail.0.iter().copied() {
        let Ok(shows) = routes.get(entity) else {
            continue;
        };
        match &shows.0 {
            Page::Card(_) => cards += 1,
            Page::Modal(_) => modals += 1,
            Page::FormSheet(_) => sheets += 1,
            Page::FullScreenModal(_) => fulls += 1,
            Page::Tab(_) => {}
        }
        let Ok(mut next) = ahead.get_mut(entity) else {
            continue;
        };
        let named = (
            format!("Card {}", cards + 1),
            format!("Modal {}", modals + 1),
            format!("Sheet {}", sheets + 1),
            format!("Full {}", fulls + 1),
        );
        if (&next.card, &next.modal, &next.sheet, &next.full)
            != (&named.0, &named.1, &named.2, &named.3)
        {
            (next.card, next.modal, next.sheet, next.full) = named;
        }
    }
}

fn trace(
    trail: Res<Trail>,
    routes: Query<&Shows<Page>>,
    mut drawn: Query<(&mut Crumbs, &mut Rungs)>,
) {
    let mut crumbs = Vec::new();
    for entity in trail.0.iter().copied() {
        if let Ok(shows) = routes.get(entity) {
            crumbs.push(shows.0.title());
        }
    }
    let elided = Crumbs::elide(crumbs);
    let deepest = trail.0.len().saturating_sub(1);
    for (at, entity) in trail.0.iter().copied().enumerate() {
        let Ok((mut crumbs, mut steps)) = drawn.get_mut(entity) else {
            continue;
        };
        let rungs = Rungs::over(deepest, at);
        if crumbs.0 != elided {
            crumbs.0 = elided.clone();
        }
        if steps.0 != rungs {
            steps.0 = rungs;
        }
    }
}

impl Crumbs {
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
}

impl Rungs {
    fn over(depth: usize, at: usize) -> Vec<Step> {
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

struct Shown {
    look: Look,
    ahead: Ahead,
    crumbs: String,
    rungs: Vec<Step>,
}

fn use_shown() -> (Router<Page>, Shown) {
    let router = use_router::<Page>();
    let shown = Shown {
        look: router.attached().unwrap_or_default(),
        ahead: router.attached().unwrap_or_default(),
        crumbs: router.attached::<Crumbs>().unwrap_or_default().0,
        rungs: router.attached::<Rungs>().unwrap_or_default().0,
    };
    (router, shown)
}

#[screens]
mod page {
    #[screen(holds = usize, title = "Tab {0}", blank)]
    #[component]
    fn Tab() -> Element {
        rsx! {
            Body {}
        }
    }

    #[screen(holds = String, title = "{0}")]
    #[component]
    fn Card() -> Element {
        rsx! {
            Body {}
        }
    }

    #[screen(holds = String, title = "{0}", presentation = Modal)]
    #[component]
    fn Modal() -> Element {
        rsx! {
            Body {}
        }
    }

    #[screen(holds = String, title = "{0}", presentation = FormSheet, detents = [0.75, 1.0])]
    #[component]
    fn FormSheet() -> Element {
        rsx! {
            Body {}
        }
    }

    #[screen(holds = String, title = "{0}", presentation = FullScreenModal)]
    #[component]
    fn FullScreenModal() -> Element {
        rsx! {
            Body {}
        }
    }
}

#[component]
fn Body() -> Element {
    let (router, shown) = use_shown();
    let (hue, kind) = (shown.look.hue, shown.look.kind);
    let (title, trail) = (shown.look.title, shown.crumbs);
    let rungs = shown.rungs;

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
                        onpress: move |_| router.push(Page::Card(shown.ahead.card.clone())),
                    }
                    Button {
                        label: "Modal",
                        onpress: move |_| router.push(Page::Modal(shown.ahead.modal.clone())),
                    }
                    Button {
                        label: "Form Sheet",
                        onpress: move |_| {
                            router.push(Page::FormSheet(shown.ahead.sheet.clone()))
                        },
                    }
                    Button {
                        label: "Full Screen Modal",
                        onpress: move |_| {
                            router.push(Page::FullScreenModal(shown.ahead.full.clone()))
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
