//! The navigation on a simulator, with no Mac: tabs are canned rather than reported, so push,
//! the back-swipe, a dragged tab switch and every `presentation` can be driven by hand.

use bevy_app::{Startup, Update};
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_mobile::MobilePlugin;
use vmux_mobile::nav::{Centre, NavPlugin, OpenBlank, Presentation, Report, Route, Tapped};
use vmux_mobile::navigator::{Screen, Stack, Tabs, use_navigation, use_route};
use vmux_native::NativePage;

const BACKDROP: (u8, u8, u8, u8) = (5, 6, 10, 255);

#[derive(Clone, PartialEq)]
enum Page {
    Tab(usize),
    Card(String),
    Modal(String),
    FormSheet(String),
    FullScreenModal(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    Tab,
    Card,
    Modal,
    FormSheet,
    FullScreenModal,
}

impl Route for Page {
    type Name = Name;

    fn name(&self) -> Name {
        match self {
            Self::Tab(_) => Name::Tab,
            Self::Card(_) => Name::Card,
            Self::Modal(_) => Name::Modal,
            Self::FormSheet(_) => Name::FormSheet,
            Self::FullScreenModal(_) => Name::FullScreenModal,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Tab(at) => format!("Tab {at}"),
            Self::Card(name)
            | Self::Modal(name)
            | Self::FormSheet(name)
            | Self::FullScreenModal(name) => name.clone(),
        }
    }
}

macro_rules! screens {
    ($($page:ident $url:literal $component:ident $near:literal $far:literal $kind:literal;)*) => {
        $(
            static $page: NativePage = NativePage::pane($url, $component).painted(BACKDROP);

            #[component]
            fn $component() -> Element {
                rsx! {
                    Stack::<Page> {
                        Body { near: $near, far: $far, kind: $kind }
                    }
                }
            }
        )*
    };
}

screens! {
    CARD "vmux://card/" CardScreen "#7c3aed" "#db2777" "card";
    MODAL "vmux://modal/" ModalScreen "#b45309" "#be123c" "modal";
    FORM_SHEET "vmux://form-sheet/" FormSheetScreen "#0f766e" "#0369a1" "form sheet";
    FULL_SCREEN_MODAL "vmux://full-screen-modal/" FullScreenModalScreen "#4338ca" "#7e22ce" "full screen modal";
}

static APP: NativePage = NativePage::pane("vmux://app/", App).painted(BACKDROP);
static TAB: NativePage = NativePage::pane("vmux://tab/", TabScreen).painted(BACKDROP);

fn main() {
    bevy_app::App::new()
        .add_plugins(MobilePlugin::showing(&APP).serving(|world| {
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
fn App() -> Element {
    rsx! {
        Stack::<Page> {
            Tabs {
                Screen::<Page> { name: Name::Tab, component: &TAB }
                Screen::<Page> { name: Name::Card, component: &CARD }
                Screen::<Page> {
                    name: Name::Modal,
                    component: &MODAL,
                    presentation: Presentation::Modal,
                }
                Screen::<Page> {
                    name: Name::FormSheet,
                    component: &FORM_SHEET,
                    presentation: Presentation::FormSheet,
                    detents: &[0.4, 1.0],
                }
                Screen::<Page> {
                    name: Name::FullScreenModal,
                    component: &FULL_SCREEN_MODAL,
                    presentation: Presentation::FullScreenModal,
                }
            }
        }
    }
}

#[component]
fn TabScreen() -> Element {
    rsx! {
        Stack::<Page> {
            TabBody {}
        }
    }
}

#[component]
fn TabBody() -> Element {
    let at = match use_route::<Page>() {
        Some(Page::Tab(at)) => at,
        _ => 1,
    };
    let hue = (at * 37 + 185) % 360;
    rsx! {
        Body {
            near: "hsl({hue} 78% 42%)",
            far: "hsl({(hue + 40) % 360} 74% 38%)",
            kind: "tab root",
        }
    }
}

#[component]
fn Body(near: String, far: String, kind: String) -> Element {
    let navigation = use_navigation::<Page>();
    let seen = navigation.state();
    let title = match use_route::<Page>() {
        Some(route) => route.title(),
        None => "Nothing".to_string(),
    };
    let depth = seen.depth;
    let sheet = seen.sheet;

    rsx! {
        div {
            class: "relative isolate flex min-h-0 flex-1 flex-col overflow-hidden bg-[#05060a] text-white",
            style: "--near:{near};--far:{far}",

            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-[40px] bg-[radial-gradient(60%_60%_at_30%_20%,var(--near),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-x-[-30%] top-[-30%] -z-10 h-[90%] blur-[60px] bg-[radial-gradient(50%_50%_at_80%_0%,var(--far),transparent_70%)]" }
            div { class: "pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(120%_90%_at_50%_0%,transparent_40%,rgba(0,0,0,0.75)_100%)]" }

            div { class: "flex min-h-0 flex-1 flex-col overflow-hidden px-6 pt-[calc(env(safe-area-inset-top)+1.5rem)] pb-[calc(env(safe-area-inset-bottom)+1.5rem)]",
                div { class: "text-[11px] uppercase tracking-[0.18em] text-white/45", "{kind}" }
                div { class: "mt-2.5 text-[40px] font-semibold leading-[1.05] tracking-[-0.02em]",
                    "{title}"
                }
                div { class: "mt-2.5 text-[13px] tabular-nums text-white/55",
                    "depth {depth} · {seen.tabs.len()} tabs open"
                }

                div { class: "my-auto flex flex-wrap justify-center gap-2",
                    Key {
                        label: "Card",
                        onpick: move |_| navigation.go(Page::Card(format!("Card {}", depth + 1))),
                    }
                    Key {
                        label: "Modal",
                        onpick: move |_| navigation.go(Page::Modal(format!("Modal {}", depth + 1))),
                    }
                    Key {
                        label: "Form Sheet",
                        onpick: move |_| {
                            navigation.go(Page::FormSheet(format!("Sheet {}", depth + 1)))
                        },
                    }
                    Key {
                        label: "Full Screen Modal",
                        onpick: move |_| {
                            navigation.go(Page::FullScreenModal(format!("Full {}", depth + 1)))
                        },
                    }
                }

                div { class: "mt-[22px] flex h-[3px] gap-[3px]",
                    for step in 0..depth {
                        div {
                            key: "{step}",
                            class: if sheet && step == depth - 1 {
                                "min-w-px flex-1 rounded-sm bg-[#ffcf6b]"
                            } else {
                                "min-w-px flex-1 rounded-sm bg-white/85"
                            },
                        }
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
