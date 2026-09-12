use dioxus::prelude::*;
use vmux_wire::protocol::AcpModeOption;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::{ProjectBranch, ProjectRow};

use crate::components::agent_menu::{AgentMenu, ComposerAgentOption};
use crate::components::effort_menu::EffortMenu;
use crate::components::model_menu::ModelMenu;
use crate::components::permission_menu::PermissionMenu;
use crate::components::project_picker::{BranchPicker, ProjectPick, ProjectPicker};
use crate::components::prompt_box::PromptPopupPlacement;
use crate::components::skeleton::Skeleton;
use crate::i18n::{TranslationValue, translate, translate_with};
use crate::list_nav::MenuDirection;
use crate::util::cn;

const COMPOSER_CHIP: &str = "flex h-7 max-w-44 shrink-0 items-center gap-1 rounded-lg px-1.5 text-[11px] text-muted-foreground";
const COMPOSER_CHIP_LABEL_TIGHT: &str = "@max-[34rem]:hidden";
const COMPOSER_CHIP_INTERACTIVE: &str =
    "transition hover:bg-foreground/[0.08] hover:text-foreground";
const COMPOSER_CHIP_OPEN: &str = "transition bg-foreground/[0.12] text-foreground";
const COMPOSER_CHIP_SKELETON: &str = "h-7 shrink-0 rounded-lg bg-foreground/[0.06]";

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
    pub permission: Option<ComposerChip>,
    #[props(default)]
    pub project: Option<ComposerChip>,
    #[props(default)]
    pub branch: Option<ComposerChip>,
    #[props(default)]
    pub is_git_repo: bool,
    #[props(default)]
    pub workspace_known: bool,
    #[props(default)]
    pub uncommitted: u32,
    #[props(default)]
    pub ahead: u32,
    #[props(default)]
    pub status: String,
    #[props(default)]
    pub active_subagents: usize,
    #[props(default)]
    pub active_tasks: usize,
    #[props(default)]
    pub queued_count: usize,
}

#[component]
pub fn StatusDot(status: String, size_class: String) -> Element {
    let tone = match status.as_str() {
        "streaming" | "interrupted" => "bg-amber-400 shadow-[0_0_8px_rgba(251,191,36,0.65)]",
        "installing" => "bg-sky-400 shadow-[0_0_8px_rgba(56,189,248,0.65)]",
        "awaiting" => "bg-violet-400 shadow-[0_0_8px_rgba(167,139,250,0.65)]",
        "errored" => "bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.65)]",
        _ => "bg-success shadow-[0_0_8px_rgba(16,185,129,0.65)]",
    };
    rsx! {
        span { class: cn([size_class.as_str(), "rounded-full", tone]) }
    }
}

#[component]
pub fn ComposerStatus(
    #[props(default)] status: String,
    #[props(default)] active_subagents: usize,
    #[props(default)] active_tasks: usize,
    #[props(default)] queued_count: usize,
) -> Element {
    let run_label = match status.as_str() {
        "streaming" => translate("composer-status-running"),
        "awaiting" => translate("composer-status-approval"),
        "installing" => translate("composer-status-starting"),
        "errored" => translate("composer-status-error"),
        _ => String::new(),
    };
    rsx! {
        div { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground",
            if !run_label.is_empty() {
                span { class: "flex h-7 items-center gap-1.5 rounded-lg px-2",
                    StatusDot { status, size_class: "h-1.5 w-1.5" }
                    "{run_label}"
                }
            }
            if active_subagents > 0 {
                span {
                    class: "flex h-7 items-center gap-1 rounded-lg bg-violet-500/[0.07] px-2 text-violet-600 dark:text-violet-300",
                    title: translate("composer-status-subagents"),
                    svg {
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "9", cy: "8", r: "3" }
                        path { d: "M3.5 19a5.5 5.5 0 0 1 11 0" }
                        circle { cx: "17", cy: "9", r: "2.5" }
                        path { d: "M15.5 14.5A4.5 4.5 0 0 1 21 19" }
                    }
                    "{active_subagents}"
                }
            }
            if active_tasks > 0 {
                span {
                    class: "flex h-7 items-center gap-1 rounded-lg px-2",
                    title: translate("composer-status-tasks-title"),
                    {translate_with("composer-status-tasks", &[("count", TranslationValue::Number(active_tasks as i64))])}
                }
            }
            if queued_count > 0 {
                span {
                    class: "flex h-7 items-center gap-1 rounded-lg px-2",
                    title: translate("composer-status-queued-title"),
                    {translate_with("composer-status-queued", &[("count", TranslationValue::Number(queued_count as i64))])}
                }
            }
        }
    }
}

#[component]
pub fn WorkspaceBadges(
    is_git_repo: bool,
    workspace_known: bool,
    uncommitted: u32,
    ahead: u32,
) -> Element {
    rsx! {
        if is_git_repo {
            if uncommitted > 0 {
                span {
                    class: "shrink-0 font-mono text-[10px] text-amber-500",
                    title: translate("composer-uncommitted-changes"),
                    "\u{25cf} {uncommitted}"
                }
            }
            if ahead > 0 {
                span {
                    class: "shrink-0 font-mono text-[10px] text-sky-500",
                    title: translate("composer-commits-ahead"),
                    "\u{2191}{ahead}"
                }
            }
        } else if workspace_known {
            span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70",
                {translate("composer-no-git")}
            }
        }
    }
}

#[component]
pub fn ComposerBar(props: ComposerBarProps) -> Element {
    let ComposerBarProps {
        menu,
        agent,
        model,
        effort,
        permission,
        project,
        branch,
        is_git_repo,
        workspace_known,
        uncommitted,
        ahead,
        status,
        active_subagents,
        active_tasks,
        queued_count,
    } = props;
    rsx! {
        div { class: "@container flex min-w-0 items-center justify-between gap-1",
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
                WorkspaceBadges {
                    is_git_repo,
                    workspace_known,
                    uncommitted,
                    ahead,
                }
            }
            div { class: "flex shrink-0 items-center gap-1",
                if let Some(chip) = permission {
                    ComposerChipSlot {
                        kind: ComposerMenuKind::Permission,
                        chip,
                        open: menu.is(ComposerMenuKind::Permission),
                    }
                }
                ComposerStatus { status, active_subagents, active_tasks, queued_count }
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
    pub permission: Option<PermissionMenuData>,
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
        permission,
        project,
        branch,
    } = props;
    let cursor = menu.cursor();
    rsx! {
        if menu.is(ComposerMenuKind::Agent) {
            if let Some(data) = agent {
                AgentMenu {
                    placement,
                    options: data.options,
                    selected_url: data.selected_url,
                    cursor,
                    on_hover: move |index| menu.point_at(index),
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
                    selected: cursor,
                    on_hover: move |index| menu.point_at(index),
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
                    cursor,
                    on_hover: move |index| menu.point_at(index),
                    on_select: move |level: String| {
                        menu.close();
                        data.on_select.call(level);
                    },
                    on_dismiss: move |()| menu.close(),
                }
            }
        }
        if menu.is(ComposerMenuKind::Permission) {
            if let Some(data) = permission {
                PermissionMenu {
                    placement,
                    modes: data.modes,
                    current_mode_id: data.current_mode_id,
                    selected: cursor,
                    on_hover: move |index| menu.point_at(index),
                    on_select: move |mode: AcpModeOption| {
                        menu.close();
                        data.on_select.call(mode);
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
                    loaded: data.loaded,
                    cursor,
                    on_hover: move |index| menu.point_at(index),
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
                    cursor,
                    on_hover: move |index| menu.point_at(index),
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
            Skeleton { class: cn([COMPOSER_CHIP_SKELETON, width]) }
        };
    }
    let label_class = kind.label_class();
    let label_class = cn([label_class, COMPOSER_CHIP_LABEL_TIGHT]);
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
    let chip_class = cn([COMPOSER_CHIP, state]);
    rsx! {
        button {
            class: chip_class,
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
pub fn ComposerChipIcon(kind: ComposerMenuKind) -> Element {
    let class = "h-3.5 w-3.5 shrink-0";
    match kind {
        ComposerMenuKind::Agent => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect {
                    x: "4",
                    y: "8",
                    width: "16",
                    height: "11",
                    rx: "3",
                }
                path { d: "M12 4.5v3.5" }
                circle { cx: "12", cy: "3.5", r: "1.2" }
                path { d: "M9 13v1.5" }
                path { d: "M15 13v1.5" }
            }
        },
        ComposerMenuKind::Model => rsx! {
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
        ComposerMenuKind::Permission => rsx! {
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M12 3 5 6v5c0 4.6 2.8 8.2 7 10 4.2-1.8 7-5.4 7-10V6l-7-3Z" }
                path { d: "m9.5 12 1.7 1.7 3.5-3.7" }
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
    pub on_select: EventHandler<ModelOptionEntry>,
}

#[derive(Clone, PartialEq)]
pub struct EffortMenuData {
    pub levels: Vec<String>,
    pub selected: String,
    pub on_select: EventHandler<String>,
}

#[derive(Clone, PartialEq)]
pub struct PermissionMenuData {
    pub modes: Vec<AcpModeOption>,
    pub current_mode_id: String,
    pub on_select: EventHandler<AcpModeOption>,
}

#[derive(Clone, PartialEq)]
pub struct ProjectMenuData {
    pub projects: Vec<ProjectRow>,
    pub loaded: bool,
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
    cursor: Signal<usize>,
}

pub fn use_composer_menu() -> ComposerMenu {
    ComposerMenu {
        open: use_signal(|| None),
        cursor: use_signal(|| 0),
    }
}

impl ComposerMenu {
    pub fn opened(&self) -> Option<ComposerMenuKind> {
        (self.open)()
    }

    pub fn is(&self, kind: ComposerMenuKind) -> bool {
        self.opened() == Some(kind)
    }

    pub fn cursor(&self) -> usize {
        (self.cursor)()
    }

    pub fn point_at(&self, index: usize) {
        let mut cursor = self.cursor;
        if *cursor.peek() != index {
            cursor.set(index);
        }
    }

    pub fn step(&self, direction: MenuDirection, rows: usize) {
        if rows == 0 {
            return;
        }
        let mut cursor = self.cursor;
        let at = (*cursor.peek()).min(rows - 1);
        let next = match direction {
            MenuDirection::Next => (at + 1).min(rows - 1),
            MenuDirection::Previous => at.saturating_sub(1),
        };
        cursor.set(next);
    }

    pub fn toggle(&self, kind: ComposerMenuKind) -> bool {
        self.toggle_at(kind, 0)
    }

    pub fn toggle_at(&self, kind: ComposerMenuKind, index: usize) -> bool {
        let mut open = self.open;
        let mut cursor = self.cursor;
        cursor.set(index);
        if *open.peek() == Some(kind) {
            open.set(None);
            return false;
        }
        open.set(Some(kind));
        true
    }

    pub fn close(&self) {
        let mut open = self.open;
        let mut cursor = self.cursor;
        cursor.set(0);
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
    Permission,
    Project,
    Branch,
}

impl ComposerMenuKind {
    fn skeleton_width(self) -> &'static str {
        match self {
            Self::Agent => "w-24",
            Self::Model => "w-28",
            Self::Effort => "w-20",
            Self::Permission => "w-24",
            Self::Project => "w-24",
            Self::Branch => "w-20",
        }
    }

    fn label_class(self) -> &'static str {
        match self {
            Self::Branch => "truncate font-mono text-[10px]",
            Self::Effort | Self::Permission => "truncate capitalize",
            _ => "truncate",
        }
    }
}
