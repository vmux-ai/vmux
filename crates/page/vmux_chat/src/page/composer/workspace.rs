use crate::event::{ChatBranch, ChatCreateWorktree, ChatGoToBranch, ChatSelectWorkspace};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_core::event::ProjectRow;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn WorkspacePills(chat: Chat) -> Element {
    let context = (chat.slash.composer_context)();
    let mut open = use_signal(|| false);
    let workspace_label = if context.workspace_selected && !context.workspace_name.is_empty() {
        context.workspace_name.clone()
    } else {
        translate("agent-project-select")
    };
    let workspace_title = if context.cwd.is_empty() {
        translate("agent-project-choose")
    } else {
        format!("{} · {}", translate("agent-project-choose"), context.cwd)
    };
    let branch_title = if context.branch.is_empty() {
        "Git repository".to_string()
    } else {
        format!("Branch {}", context.branch)
    };
    let worktree_title = if context.base_ref.is_empty() {
        "Linked worktree".to_string()
    } else {
        format!("Worktree from {}", context.base_ref)
    };
    rsx! {
        if context.can_manage_workspace {
            div { class: "relative shrink-0",
                button {
                    class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground transition hover:bg-foreground/[0.08] hover:text-foreground",
                    title: "{workspace_title}",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        let showing = *open.peek();
                        open.set(!showing);
                    },
                    svg {
                        class: "h-3.5 w-3.5 shrink-0",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                    }
                    span { class: "truncate", "{workspace_label}" }
                }
                if open() {
                    ProjectMenu {
                        chat,
                        projects: context.projects.clone(),
                        on_dismiss: move |()| open.set(false),
                    }
                }
            }
        } else if !context.cwd.is_empty() {
            span {
                class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                title: "{context.cwd}",
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                }
                span { class: "truncate", "{workspace_label}" }
            }
        }
        if context.is_git_repo {
            span {
                class: "flex h-7 max-w-40 shrink-0 items-center gap-1.5 rounded-lg px-2 font-mono text-[10px] text-muted-foreground",
                title: "{branch_title}",
                svg {
                    class: "h-3.5 w-3.5 shrink-0",
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
                span { class: "truncate", if context.branch.is_empty() { "Git" } else { "{context.branch}" } }
            }
            if context.is_worktree {
                span {
                    class: "flex h-7 shrink-0 items-center gap-1 rounded-lg bg-violet-500/[0.08] px-2 text-[10px] font-medium text-violet-600 ring-1 ring-inset ring-violet-500/15 dark:text-violet-300",
                    title: "{worktree_title}",
                    "Worktree"
                }
            } else if context.can_manage_workspace {
                button {
                    class: "flex h-7 shrink-0 items-center gap-1 rounded-lg px-2 text-[10px] font-medium text-muted-foreground transition hover:bg-violet-500/[0.08] hover:text-violet-600 dark:hover:text-violet-300",
                    title: "Create or select a worktree for this project",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        let _ = send(&ChatCreateWorktree);
                        focus_prompt_end(PROMPT_INPUT_ID);
                    },
                    "+ Worktree"
                }
            }
            if context.uncommitted > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-amber-500", title: "Uncommitted changes", "● {context.uncommitted}" }
            }
            if context.ahead > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-sky-500", title: "Commits ahead of upstream", "↑{context.ahead}" }
            }
        } else if context.workspace_selected {
            span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70", "No Git" }
        }
    }
}

#[component]
fn ProjectMenu(chat: Chat, projects: Vec<ProjectRow>, on_dismiss: EventHandler<()>) -> Element {
    let expanded = (chat.projects.expanded)();
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
                ProjectMenuRow {
                    key: "pm{project.path}",
                    chat,
                    project: project.clone(),
                    expanded: expanded == project.path,
                }
                if expanded == project.path {
                    match chat.projects.listed(&project.path) {
                        None => rsx! {
                            div { class: "px-3.5 py-1.5 pl-8 text-xs text-muted-foreground", {translate("agent-project-loading-branches")} }
                        },
                        Some(branches) if branches.is_empty() => rsx! {
                            div { class: "px-3.5 py-1.5 pl-8 text-xs text-muted-foreground", {translate("agent-project-no-branches")} }
                        },
                        Some(branches) => rsx! {
                            for branch in branches {
                                BranchMenuRow {
                                    key: "br{project.path}/{branch.branch}",
                                    project: project.path.clone(),
                                    branch: branch.clone(),
                                    on_pick: move |()| on_dismiss.call(()),
                                }
                            }
                        },
                    }
                }
            }
            button {
                class: "flex w-full items-center gap-2 border-t border-foreground/10 px-3.5 py-2 text-left text-sm text-muted-foreground transition hover:bg-foreground/[0.06] hover:text-foreground",
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| {
                    on_dismiss.call(());
                    let _ = send(&ChatSelectWorkspace);
                    focus_prompt_end(PROMPT_INPUT_ID);
                },
                {translate("agent-project-choose-another")}
            }
        }
    }
}

#[component]
fn ProjectMenuRow(chat: Chat, project: ProjectRow, expanded: bool) -> Element {
    let path = project.path.clone();
    rsx! {
        button {
            class: if project.is_active { "flex w-full items-center gap-2 px-3.5 py-2 text-left text-sm bg-foreground/[0.06]" } else { "flex w-full items-center gap-2 px-3.5 py-2 text-left text-sm transition hover:bg-foreground/[0.06]" },
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| chat.projects.expand(&path),
            svg {
                class: if expanded { "h-3 w-3 shrink-0 rotate-90 text-muted-foreground" } else { "h-3 w-3 shrink-0 text-muted-foreground" },
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
            span { class: "ml-auto truncate text-xs text-muted-foreground", "{project.display_path}" }
        }
    }
}

#[component]
fn BranchMenuRow(project: String, branch: ChatBranch, on_pick: EventHandler<()>) -> Element {
    let held = !branch.checkout.is_empty();
    let title = match held {
        true => translate("agent-project-open-worktree"),
        false => translate("agent-project-create-worktree"),
    };
    rsx! {
        button {
            class: "flex w-full items-center gap-2 py-1.5 pl-8 pr-3.5 text-left text-xs transition hover:bg-foreground/[0.06]",
            title: "{title}",
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| {
                on_pick.call(());
                let _ = send(&ChatGoToBranch {
                    project: project.clone(),
                    branch: branch.branch.clone(),
                    checkout: branch.checkout.clone(),
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            span { class: "truncate font-mono text-foreground", "{branch.branch}" }
            if held {
                span { class: "ml-auto shrink-0 truncate rounded bg-violet-500/[0.10] px-1.5 py-0.5 text-[10px] text-violet-600 dark:text-violet-300", "{branch.label}" }
            } else {
                span { class: "ml-auto shrink-0 text-[10px] text-muted-foreground/70", "+" }
            }
        }
    }
}
