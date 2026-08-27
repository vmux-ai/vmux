use dioxus::prelude::*;
use vmux_wire::space::{ProjectBranch, ProjectRow};

use crate::components::prompt_box::{
    PROMPT_MENU_INDENT, PROMPT_MENU_ROW, PROMPT_MENU_ROW_IDLE, PROMPT_MENU_ROW_SELECTED,
    PromptPopup, PromptPopupPlacement,
};
use crate::i18n::translate;

#[derive(Clone, PartialEq, Props)]
pub struct ProjectPickerProps {
    #[props(default)]
    pub placement: PromptPopupPlacement,
    pub projects: Vec<ProjectRow>,
    pub expanded: String,
    pub branches: Vec<ProjectBranch>,
    pub branches_for: String,
    pub on_expand: EventHandler<String>,
    pub on_pick: EventHandler<ProjectPick>,
    pub on_choose_another: EventHandler<()>,
    pub on_dismiss: EventHandler<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPick {
    pub project: String,
    pub branch: String,
    pub checkout: String,
}

#[component]
pub fn ProjectPicker(props: ProjectPickerProps) -> Element {
    let ProjectPickerProps {
        placement,
        projects,
        expanded,
        branches,
        branches_for,
        on_expand,
        on_pick,
        on_choose_another,
        on_dismiss,
    } = props;
    let roots = projects
        .iter()
        .filter(|project| project.depth == 0)
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        PromptPopup {
            placement,
            heading: translate("composer-project"),
            on_dismiss: move |()| on_dismiss.call(()),
            if roots.is_empty() {
                div { class: "{PROMPT_MENU_ROW} text-muted-foreground", {translate("agent-project-none")} }
            }
            for project in roots {
                ProjectPickerRow {
                    key: "pp{project.path}",
                    project: project.clone(),
                    open: expanded == project.path,
                    on_toggle: move |path| on_expand.call(path),
                }
                if expanded == project.path {
                    if branches_for != project.path {
                        div { class: "{PROMPT_MENU_ROW} {PROMPT_MENU_INDENT} text-muted-foreground", {translate("agent-project-loading-branches")} }
                    } else if branches.is_empty() {
                        div { class: "{PROMPT_MENU_ROW} {PROMPT_MENU_INDENT} text-muted-foreground", {translate("agent-project-no-branches")} }
                    } else {
                        for branch in branches.iter().cloned() {
                            ProjectBranchRow {
                                key: "pb{project.path}/{branch.branch}",
                                project: project.path.clone(),
                                branch,
                                indent: true,
                                on_pick: move |pick| on_pick.call(pick),
                            }
                        }
                    }
                }
            }
            button {
                class: "{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE} border-t border-foreground/10 text-muted-foreground hover:text-foreground",
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| on_choose_another.call(()),
                {translate("agent-project-choose-another")}
            }
        }
    }
}

#[component]
pub fn BranchPicker(
    #[props(default)] placement: PromptPopupPlacement,
    project: String,
    branches: Vec<ProjectBranch>,
    loaded: bool,
    on_pick: EventHandler<ProjectPick>,
    on_dismiss: EventHandler<()>,
) -> Element {
    rsx! {
        PromptPopup {
            placement,
            heading: translate("composer-branch"),
            on_dismiss: move |()| on_dismiss.call(()),
            if !loaded {
                div { class: "{PROMPT_MENU_ROW} text-muted-foreground", {translate("agent-project-loading-branches")} }
            } else if branches.is_empty() {
                div { class: "{PROMPT_MENU_ROW} text-muted-foreground", {translate("agent-project-no-branches")} }
            } else {
                for branch in branches {
                    ProjectBranchRow {
                        key: "bp{branch.branch}",
                        project: project.clone(),
                        branch,
                        indent: false,
                        on_pick: move |pick| on_pick.call(pick),
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectPickerRow(project: ProjectRow, open: bool, on_toggle: EventHandler<String>) -> Element {
    let path = project.path.clone();
    rsx! {
        button {
            class: if project.is_active { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_SELECTED}") } else { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE}") },
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_toggle.call(path.clone()),
            svg {
                class: if open { "h-3 w-3 shrink-0 rotate-90 text-muted-foreground" } else { "h-3 w-3 shrink-0 text-muted-foreground" },
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2.4",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "m9 6 6 6-6 6" }
            }
            span {
                class: if project.missing { "truncate font-medium text-muted-foreground/60 line-through" } else { "truncate font-medium text-foreground" },
                "{project.label}"
            }
            if !project.branch.is_empty() {
                span { class: "ml-auto shrink-0 font-mono text-[10px] text-muted-foreground", "{project.branch}" }
            }
        }
    }
}

#[component]
fn ProjectBranchRow(
    project: String,
    branch: ProjectBranch,
    indent: bool,
    on_pick: EventHandler<ProjectPick>,
) -> Element {
    let held = branch.held();
    let title = match held {
        true => translate("agent-project-open-worktree"),
        false => translate("agent-project-create-worktree"),
    };
    let pick = ProjectPick {
        project,
        branch: branch.branch.clone(),
        checkout: branch.checkout.clone(),
    };
    rsx! {
        button {
            class: if indent { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE} {PROMPT_MENU_INDENT}") } else { format!("{PROMPT_MENU_ROW} {PROMPT_MENU_ROW_IDLE}") },
            title: "{title}",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_pick.call(pick.clone()),
            span { class: "truncate font-mono text-foreground", "{branch.branch}" }
            div { class: "ml-auto flex shrink-0 items-center gap-2",
                BranchChangeStat {
                    insertions: branch.insertions,
                    deletions: branch.deletions,
                }
                if held {
                    span { class: "shrink-0 truncate rounded bg-violet-500/[0.10] px-1.5 py-0.5 text-[10px] text-violet-600 dark:text-violet-300", "{branch.label}" }
                } else {
                    span { class: "shrink-0 text-[10px] text-muted-foreground/70", "+" }
                }
            }
        }
    }
}

#[component]
fn BranchChangeStat(insertions: u32, deletions: u32) -> Element {
    if insertions == 0 && deletions == 0 {
        return rsx! {};
    }
    rsx! {
        span {
            class: "flex shrink-0 items-center gap-1 font-mono text-[10px] tabular-nums",
            title: translate("agent-project-branch-change"),
            if insertions > 0 {
                span { class: "text-emerald-600 dark:text-emerald-400", "+{insertions}" }
            }
            if deletions > 0 {
                span { class: "text-rose-600 dark:text-rose-400", "−{deletions}" }
            }
        }
    }
}
