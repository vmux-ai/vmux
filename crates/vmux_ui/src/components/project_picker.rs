use dioxus::prelude::*;
use vmux_core::event::{ProjectBranch, ProjectRow};

use crate::components::prompt_box::PromptPopup;
use crate::i18n::translate;

#[derive(Clone, PartialEq, Props)]
pub struct ProjectPickerProps {
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
        PromptPopup { class: "min-w-72", on_dismiss: move |()| on_dismiss.call(()),
            if roots.is_empty() {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-project-none")} }
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
                        div { class: "px-3.5 py-1.5 pl-8 text-xs text-muted-foreground", {translate("agent-project-loading-branches")} }
                    } else if branches.is_empty() {
                        div { class: "px-3.5 py-1.5 pl-8 text-xs text-muted-foreground", {translate("agent-project-no-branches")} }
                    } else {
                        for branch in branches.iter().cloned() {
                            ProjectBranchRow {
                                key: "pb{project.path}/{branch.branch}",
                                project: project.path.clone(),
                                branch,
                                on_pick: move |pick| on_pick.call(pick),
                            }
                        }
                    }
                }
            }
            button {
                class: "flex w-full items-center gap-2 border-t border-foreground/10 px-3.5 py-2 text-left text-sm text-muted-foreground transition hover:bg-foreground/[0.06] hover:text-foreground",
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| on_choose_another.call(()),
                {translate("agent-project-choose-another")}
            }
        }
    }
}

#[component]
fn ProjectPickerRow(project: ProjectRow, open: bool, on_toggle: EventHandler<String>) -> Element {
    let path = project.path.clone();
    rsx! {
        button {
            class: if project.is_active { "flex w-full items-center gap-2 bg-foreground/[0.06] px-3.5 py-2 text-left text-sm" } else { "flex w-full items-center gap-2 px-3.5 py-2 text-left text-sm transition hover:bg-foreground/[0.06]" },
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
            class: "flex w-full items-center gap-2 py-1.5 pl-8 pr-3.5 text-left text-xs transition hover:bg-foreground/[0.06]",
            title: "{title}",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| on_pick.call(pick.clone()),
            span { class: "truncate font-mono text-foreground", "{branch.branch}" }
            if held {
                span { class: "ml-auto shrink-0 truncate rounded bg-violet-500/[0.10] px-1.5 py-0.5 text-[10px] text-violet-600 dark:text-violet-300", "{branch.label}" }
            } else {
                span { class: "ml-auto shrink-0 text-[10px] text-muted-foreground/70", "+" }
            }
        }
    }
}
