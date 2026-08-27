use crate::event::ChatCreateWorktree;
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::hooks::send;

#[component]
pub(super) fn WorkspaceBadges(chat: Chat) -> Element {
    let context = (chat.slash.composer_context)();
    let worktree_title = if context.base_ref.is_empty() {
        "Linked worktree".to_string()
    } else {
        format!("Worktree from {}", context.base_ref)
    };
    rsx! {
        if context.is_git_repo {
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
