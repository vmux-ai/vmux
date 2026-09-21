use dioxus::prelude::*;
use vmux_api::space::{ProjectBranch, ProjectRow};

use crate::components::prompt_box::{
    PROMPT_MENU_INDENT, PROMPT_MENU_ROW, PromptMenuRow, PromptPopup, PromptPopupPlacement,
};
use crate::components::skeleton::Skeleton;
use crate::i18n::translate;
use crate::util::cn;

#[derive(Clone, PartialEq, Props)]
pub struct ProjectPickerProps {
    #[props(default)]
    pub placement: PromptPopupPlacement,
    pub projects: Vec<ProjectRow>,
    #[props(default)]
    pub loaded: bool,
    #[props(default)]
    pub cursor: usize,
    #[props(default)]
    pub on_hover: Option<EventHandler<usize>>,
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
        loaded,
        cursor,
        on_hover,
        on_pick,
        on_choose_another,
        on_dismiss,
    } = props;
    let mut roots = Vec::new();
    for project in &projects {
        if project.depth == 0 {
            roots.push(project.clone());
        }
    }
    let empty_class = cn([PROMPT_MENU_ROW, "text-muted-foreground"]);
    let choose_another_row = PromptMenuRow::class(cursor == roots.len());
    let choose_another_class = cn([
        choose_another_row.as_str(),
        "border-t border-foreground/10 text-muted-foreground hover:text-foreground",
    ]);
    rsx! {
        PromptPopup {
            placement,
            heading: translate("composer-project"),
            on_dismiss: move |()| on_dismiss.call(()),
            if roots.is_empty() {
                if loaded {
                    div { class: empty_class, {translate("agent-project-none")} }
                } else {
                    PromptMenuSkeleton { rows: 3 }
                }
            }
            for (index , project) in roots.iter().enumerate() {
                ProjectPickerRow {
                    key: "pp{project.path}",
                    project: project.clone(),
                    at_cursor: index == cursor,
                    on_hover: move |()| {
                        if let Some(hover) = on_hover {
                            hover.call(index);
                        }
                    },
                    on_pick: move |pick| on_pick.call(pick),
                }
            }
            button {
                class: choose_another_class,
                onmousedown: move |event| event.prevent_default(),
                onmouseenter: move |_| {
                    if let Some(hover) = on_hover {
                        hover.call(roots.len());
                    }
                },
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
    #[props(default)] cursor: usize,
    #[props(default)] on_hover: Option<EventHandler<usize>>,
    on_pick: EventHandler<ProjectPick>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let empty_class = cn([PROMPT_MENU_ROW, "text-muted-foreground"]);
    rsx! {
        PromptPopup {
            placement,
            heading: translate("composer-branch"),
            on_dismiss: move |()| on_dismiss.call(()),
            if !loaded {
                PromptMenuSkeleton { rows: 4 }
            } else if branches.is_empty() {
                div { class: empty_class, {translate("agent-project-no-branches")} }
            } else {
                for (index , branch) in branches.into_iter().enumerate() {
                    ProjectBranchRow {
                        key: "bp{branch.branch}",
                        project: project.clone(),
                        branch,
                        indent: false,
                        at_cursor: index == cursor,
                        on_hover: move |()| {
                            if let Some(hover) = on_hover {
                                hover.call(index);
                            }
                        },
                        on_pick: move |pick| on_pick.call(pick),
                    }
                }
            }
        }
    }
}

#[component]
fn PromptMenuSkeleton(rows: usize) -> Element {
    const WIDTHS: [&str; 4] = ["w-[88%]", "w-[76%]", "w-[64%]", "w-[52%]"];

    rsx! {
        div { class: "flex flex-col gap-1 px-3 py-2",
            for (row, width) in WIDTHS.iter().take(rows).enumerate() {
                Skeleton {
                    key: "skeleton-{row}",
                    class: cn(["h-5 bg-foreground/[0.06]", width]),
                }
            }
        }
    }
}

#[component]
fn ProjectPickerRow(
    project: ProjectRow,
    at_cursor: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<ProjectPick>,
) -> Element {
    let path = project.path.clone();
    rsx! {
        button {
            class: PromptMenuRow::class(at_cursor),
            onmousedown: move |event| event.prevent_default(),
            onmouseenter: move |_| on_hover.call(()),
            onclick: move |_| on_pick.call(ProjectPick {
                project: path.clone(),
                branch: String::new(),
                checkout: String::new(),
            }),
            span {
                class: if project.missing { "truncate font-medium text-muted-foreground/60 line-through" } else { "truncate font-medium text-foreground" },
                "{project.label}"
            }
            if !project.branch.is_empty() {
                span { class: "ml-auto shrink-0 font-mono text-[10px] text-muted-foreground", "{project.branch}" }
            }
            if project.is_active {
                svg {
                    class: "h-3.5 w-3.5 shrink-0 text-success",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2.2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "m5 12 4 4L19 6" }
                }
            }
        }
    }
}

#[component]
fn ProjectBranchRow(
    project: String,
    branch: ProjectBranch,
    indent: bool,
    at_cursor: bool,
    on_hover: EventHandler<()>,
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
    let row_class = PromptMenuRow::class(at_cursor);
    rsx! {
        button {
            class: cn([
                row_class.as_str(),
                if indent { PROMPT_MENU_INDENT } else { "" },
            ]),
            title: "{title}",
            onmousedown: move |event| event.prevent_default(),
            onmouseenter: move |_| on_hover.call(()),
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
