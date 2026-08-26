use crate::event::{ChatCreateWorktree, ChatGoToBranch, ChatSelectWorkspace};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::project_picker::{ProjectPick, ProjectPicker};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn WorkspaceMenu(chat: Chat) -> Element {
    let context = (chat.slash.composer_context)();
    rsx! {
        ProjectPicker {
            projects: context.projects.clone(),
            expanded: (chat.projects.expanded)(),
            branches: (chat.projects.branches)(),
            branches_for: (chat.projects.branches_for)(),
            on_expand: move |path: String| chat.projects.expand(&path),
            on_pick: move |pick: ProjectPick| {
                chat.projects.close();
                let _ = send(&ChatGoToBranch {
                    project: pick.project,
                    branch: pick.branch,
                    checkout: pick.checkout,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            on_choose_another: move |()| {
                chat.projects.close();
                let _ = send(&ChatSelectWorkspace);
                focus_prompt_end(PROMPT_INPUT_ID);
            },
            on_dismiss: move |()| chat.projects.close(),
        }
    }
}

#[component]
pub(super) fn WorkspacePills(chat: Chat) -> Element {
    let context = (chat.slash.composer_context)();
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
            button {
                class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground transition hover:bg-foreground/[0.08] hover:text-foreground",
                title: "{workspace_title}",
                onmousedown: move |event| event.prevent_default(),
                onclick: move |_| {
                    chat.open_only(crate::page::state::ComposerMenu::Projects);
                    chat.projects.toggle();
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
