use dioxus::prelude::*;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::{ProjectBranch, ProjectRow};

use crate::components::agent_menu::{AgentMenu, ComposerAgentOption};
use crate::components::effort_menu::EffortMenu;
use crate::components::model_menu::ModelMenu;
use crate::components::project_picker::{BranchPicker, ProjectPick, ProjectPicker};
use crate::components::prompt_box::PromptPopupPlacement;

const COMPOSER_CHIP: &str = "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground";
const COMPOSER_CHIP_INTERACTIVE: &str =
    "transition hover:bg-foreground/[0.08] hover:text-foreground";
const COMPOSER_CHIP_OPEN: &str = "transition bg-foreground/[0.12] text-foreground";
const COMPOSER_CHIP_SKELETON: &str = "h-7 shrink-0 animate-pulse rounded-lg bg-foreground/[0.06]";

#[derive(Clone, PartialEq, Props)]
pub struct ComposerBarProps {
    pub menu: ComposerMenu,
    #[props(default)]
    pub agent: Option<ComposerChip>,
    #[props(default)]
    pub model: Option<ComposerChip>,
    #[props(default)]
    pub effort: Option<ComposerChip>,
    #[props(default)]
    pub project: Option<ComposerChip>,
    #[props(default)]
    pub branch: Option<ComposerChip>,
    #[props(default)]
    pub badges: Option<Element>,
    #[props(default)]
    pub status: Option<Element>,
}

#[component]
pub fn ComposerBar(props: ComposerBarProps) -> Element {
    let ComposerBarProps {
        menu,
        agent,
        model,
        effort,
        project,
        branch,
        badges,
        status,
    } = props;
    rsx! {
        div { class: "flex min-w-0 items-center justify-between gap-1",
            div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
                if let Some(chip) = agent {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Agent,
                        chip,
                        open: menu.is(ComposerMenuKind::Agent),
                    }
                }
                if let Some(chip) = model {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Model,
                        chip,
                        open: menu.is(ComposerMenuKind::Model),
                    }
                }
                if let Some(chip) = effort {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Effort,
                        chip,
                        open: menu.is(ComposerMenuKind::Effort),
                    }
                }
                if let Some(chip) = project {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Project,
                        chip,
                        open: menu.is(ComposerMenuKind::Project),
                    }
                }
                if let Some(chip) = branch {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Branch,
                        chip,
                        open: menu.is(ComposerMenuKind::Branch),
                    }
                }
                if let Some(badges) = badges {
                    {badges}
                }
            }
            if let Some(status) = status {
                {status}
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
pub struct ComposerMenusProps {
    pub menu: ComposerMenu,
    #[props(default)]
    pub placement: PromptPopupPlacement,
    #[props(default)]
    pub agent: Option<AgentMenuData>,
    #[props(default)]
    pub model: Option<ModelMenuData>,
    #[props(default)]
    pub effort: Option<EffortMenuData>,
    #[props(default)]
    pub project: Option<ProjectMenuData>,
    #[props(default)]
    pub branch: Option<BranchMenuData>,
}

#[component]
pub fn ComposerMenus(props: ComposerMenusProps) -> Element {
    let ComposerMenusProps {
        menu,
        placement,
        agent,
        model,
        effort,
        project,
        branch,
    } = props;
    rsx! {
        if menu.is(ComposerMenuKind::Agent) {
            if let Some(data) = agent {
                AgentMenu {
                    placement,
                    options: data.options,
                    selected_url: data.selected_url,
                    on_select: move |url: String| {
                        menu.close();
                        data.on_select.call(url);
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
        if menu.is(ComposerMenuKind::Model) {
            if let Some(data) = model {
                ModelMenu {
                    placement,
                    models: data.models,
                    current_model_id: data.current_model_id,
                    selected: data.selected,
                    on_hover: data.on_hover,
                    on_select: move |entry: ModelOptionEntry| {
                        menu.close();
                        data.on_select.call(entry);
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
        if menu.is(ComposerMenuKind::Effort) {
            if let Some(data) = effort {
                EffortMenu {
                    placement,
                    levels: data.levels,
                    selected: data.selected,
                    on_select: move |level: String| {
                        menu.close();
                        data.on_select.call(level);
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
        if menu.is(ComposerMenuKind::Project) {
            if let Some(data) = project {
                ProjectPicker {
                    placement,
                    projects: data.projects,
                    expanded: data.expanded,
                    branches: data.branches,
                    branches_for: data.branches_for,
                    on_expand: data.on_expand,
                    on_pick: move |pick: ProjectPick| {
                        menu.close();
                        data.on_pick.call(pick);
                    },
                    on_choose_another: move |()| {
                        menu.close();
                        data.on_choose_another.call(());
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
        if menu.is(ComposerMenuKind::Branch) {
            if let Some(data) = branch {
                BranchPicker {
                    placement,
                    project: data.project,
                    branches: data.branches,
                    loaded: data.loaded,
                    on_pick: move |pick: ProjectPick| {
                        menu.close();
                        data.on_pick.call(pick);
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
    }
}

#[component]
fn ComposerChipSlot(kind: ComposerMenuKind, chip: ComposerChip, open: bool) -> Element {
    if chip.loading {
        let width = kind.skeleton_width();
        return rsx! {
            div { class: "{COMPOSER_CHIP_SKELETON} {width}" }
        };
    }
    let label_class = kind.label_class();
    let Some(on_open) = chip.on_open else {
        return rsx! {
            span { class: COMPOSER_CHIP, title: "{chip.title}",
                ComposerChipIcon { kind }
                span { class: label_class, "{chip.label}" }
            }
        };
    };
    let state = match open {
        true => COMPOSER_CHIP_OPEN,
        false => COMPOSER_CHIP_INTERACTIVE,
    };
    rsx! {
        button {
            class: "{COMPOSER_CHIP} {state}",
            title: "{chip.title}",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_open.call(()),
            ComposerChipIcon { kind }
            span { class: label_class, "{chip.label}" }
            svg {
                class: if open { "h-3 w-3 shrink-0 rotate-180 opacity-70 transition-transform duration-200 ease-out" } else { "h-3 w-3 shrink-0 opacity-50 transition-transform duration-200 ease-out" },
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                path { d: "m8 10 4 4 4-4" }
            }
        }
    }
}

#[component]
fn ComposerChipIcon(kind: ComposerMenuKind) -> Element {
    let class = "h-3.5 w-3.5 shrink-0";
    match kind {
        ComposerMenuKind::Agent | ComposerMenuKind::Model => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 3l1.7 4.6L18 9.3l-4.3 1.7L12 16l-1.7-5L6 9.3l4.3-1.7L12 3Z" }
                path { d: "M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15Z" }
            }
        },
        ComposerMenuKind::Effort => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 20a8 8 0 1 1 8-8" }
                path { d: "M12 12l3.5-2" }
            }
        },
        ComposerMenuKind::Project => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
            }
        },
        ComposerMenuKind::Branch => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "6", cy: "5", r: "2" }
                circle { cx: "6", cy: "19", r: "2" }
                circle { cx: "18", cy: "12", r: "2" }
                path { d: "M8 5h3a3 3 0 0 1 3 3v1a3 3 0 0 0 3 3" }
                path { d: "M6 7v10" }
            }
        },
    }
}

#[derive(Clone, PartialEq)]
pub struct ComposerChip {
    pub label: String,
    pub title: String,
    pub loading: bool,
    pub on_open: Option<EventHandler<()>>,
}

impl ComposerChip {
    pub fn loading() -> Self {
        Self {
            label: String::new(),
            title: String::new(),
            loading: true,
            on_open: None,
        }
    }

    pub fn ready(label: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            title: title.into(),
            loading: false,
            on_open: None,
        }
    }

    pub fn opens(mut self, on_open: EventHandler<()>) -> Self {
        self.on_open = Some(on_open);
        self
    }
}

#[derive(Clone, PartialEq)]
pub struct AgentMenuData {
    pub options: Vec<ComposerAgentOption>,
    pub selected_url: String,
    pub on_select: EventHandler<String>,
}

#[derive(Clone, PartialEq)]
pub struct ModelMenuData {
    pub models: Vec<ModelOptionEntry>,
    pub current_model_id: String,
    pub selected: usize,
    pub on_hover: EventHandler<usize>,
    pub on_select: EventHandler<ModelOptionEntry>,
}

#[derive(Clone, PartialEq)]
pub struct EffortMenuData {
    pub levels: Vec<String>,
    pub selected: String,
    pub on_select: EventHandler<String>,
}

#[derive(Clone, PartialEq)]
pub struct ProjectMenuData {
    pub projects: Vec<ProjectRow>,
    pub expanded: String,
    pub branches: Vec<ProjectBranch>,
    pub branches_for: String,
    pub on_expand: EventHandler<String>,
    pub on_pick: EventHandler<ProjectPick>,
    pub on_choose_another: EventHandler<()>,
}

#[derive(Clone, PartialEq)]
pub struct BranchMenuData {
    pub project: String,
    pub branches: Vec<ProjectBranch>,
    pub loaded: bool,
    pub on_pick: EventHandler<ProjectPick>,
}

#[derive(Clone, Copy, PartialEq)]
pub struct ComposerMenu {
    open: Signal<Option<ComposerMenuKind>>,
}

pub fn use_composer_menu() -> ComposerMenu {
    ComposerMenu {
        open: use_signal(|| None),
    }
}

impl ComposerMenu {
    pub fn opened(&self) -> Option<ComposerMenuKind> {
        (self.open)()
    }

    pub fn is(&self, kind: ComposerMenuKind) -> bool {
        self.opened() == Some(kind)
    }

    pub fn toggle(&self, kind: ComposerMenuKind) -> bool {
        let mut open = self.open;
        if *open.peek() == Some(kind) {
            open.set(None);
            return false;
        }
        open.set(Some(kind));
        true
    }

    pub fn close(&self) {
        let mut open = self.open;
        if open.peek().is_some() {
            open.set(None);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerMenuKind {
    Agent,
    Model,
    Effort,
    Project,
    Branch,
}

impl ComposerMenuKind {
    fn skeleton_width(self) -> &'static str {
        match self {
            Self::Agent => "w-24",
            Self::Model => "w-28",
            Self::Effort => "w-20",
            Self::Project => "w-24",
            Self::Branch => "w-20",
        }
    }

    fn label_class(self) -> &'static str {
        match self {
            Self::Branch => "truncate font-mono text-[10px]",
            Self::Effort => "truncate capitalize",
            _ => "truncate",
        }
    }
}
