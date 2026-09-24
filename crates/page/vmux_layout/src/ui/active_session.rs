#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::event::{ProjectRow, ProjectTreeToggle};
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::badge::Badge;
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::tree_row::{
    SIDEBAR_TREE_CHEVRON_CLOSED, SIDEBAR_TREE_CHEVRON_OPEN, SidebarTreeChildren, SidebarTreeRow,
    SidebarTreeRowGroup,
};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{BuiltinIconView, GitIconView, LineIcon, LineIconView, PageIconView};

use crate::event::{
    ActiveSession, ActiveWorkspaceProject, SideSheetProjectOpenRequest, TabBoundary,
};

#[component]
pub(crate) fn ActiveSessionPanel(session: ActiveSession) -> Element {
    let ActiveSession {
        page,
        agent,
        project,
        boundary,
        pane_id,
    } = session;
    let title = if page.title.trim().is_empty() {
        page.url.clone()
    } else {
        page.title.clone()
    };
    let status = agent.as_ref().map(|agent| {
        if agent.is_running {
            ("streaming", translate("agent-status-running"))
        } else if agent.is_done_unseen {
            ("idle", translate("agent-status-done"))
        } else {
            ("idle", translate("common-current"))
        }
    });
    rsx! {
        div { class: "border-t border-foreground/10 px-2.5 py-2.5",
            div { class: "flex min-w-0 items-center gap-2.5",
                div { class: "flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.055]",
                    PageIconView {
                        icon: page.icon.clone(),
                        url: page.url.clone(),
                        img_class: "size-4 shrink-0 rounded-sm object-contain".to_string(),
                        icon_class: "size-4 shrink-0 text-muted-foreground".to_string(),
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-ui font-semibold text-foreground", title: "{title}", "{title}" }
                    div { class: "truncate text-[10px] text-muted-foreground", title: "{page.url}", "{page.url}" }
                }
            }
            div { class: "mt-2 flex flex-col gap-1",
                if let Some(agent) = agent {
                    div { class: "flex min-w-0 items-center gap-2 rounded-md bg-foreground/[0.035] px-2 py-1.5",
                        Avatar {
                            src: agent.icon.clone(),
                            seed: agent.name.clone(),
                            background: agent.color.clone(),
                            alt: agent.name.clone(),
                            class: "size-4 text-[7px]",
                        }
                        span { class: "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground", "{agent.name}" }
                        if let Some((status, label)) = status {
                            span { class: "flex shrink-0 items-center gap-1.5 text-[10px] text-muted-foreground",
                                StatusDot { status: status.to_string(), size_class: "size-1.5".to_string() }
                                "{label}"
                            }
                        }
                    }
                }
                if let Some(project) = project {
                    ActiveWorkspaceProjectTree { project, pane_id }
                }
                if let Some(boundary) = boundary {
                    if boundary.is_git_repo {
                        ActiveSessionGit { boundary }
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveSessionGit(boundary: TabBoundary) -> Element {
    let repository = if boundary.repository.is_empty() {
        translate("composer-git-repository")
    } else {
        boundary.repository.clone()
    };
    let branch = if boundary.branch.is_empty() {
        repository.clone()
    } else {
        boundary.branch.clone()
    };
    let relation = if boundary.base_ref.is_empty() || boundary.base_ref == boundary.branch {
        branch
    } else {
        format!("{} → {}", boundary.base_ref, branch)
    };
    rsx! {
        div { class: "min-w-0 rounded-md bg-foreground/[0.035] px-2.5 py-2.5",
            div { class: "flex min-w-0 items-center gap-2",
                GitIconView { class: "size-3.5 shrink-0".to_string() }
                span { class: "min-w-0 flex-1 truncate text-[10px] font-semibold text-foreground", title: "{repository}", "{repository}" }
                if boundary.is_worktree {
                    span {
                        class: "flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary",
                        title: translate("layout-worktree"),
                        LineIconView { icon: LineIcon::GitFork, class: "size-3".to_string() }
                    }
                }
            }
            div { class: "mt-1.5 flex min-w-0 items-center gap-2 font-mono text-[10px] text-muted-foreground",
                span { class: "min-w-0 flex-1 truncate", title: "{relation}", "{relation}" }
            }
            div { class: "mt-2 flex flex-wrap items-center gap-1.5 text-[9px]",
                if boundary.uncommitted > 0 {
                    Badge {
                        class: "gap-1 rounded-full bg-amber-400/10 px-2 py-1 font-medium text-amber-700 ring-1 ring-inset ring-amber-400/20 dark:text-amber-300",
                        title: translate("composer-uncommitted-changes"),
                        span { class: "size-1.5 rounded-full bg-amber-400" }
                        span { class: "font-mono tabular-nums", "{boundary.uncommitted}" }
                        span { {translate("git-status-modified")} }
                    }
                }
                if boundary.changed_files > 0 {
                    Badge {
                        class: "gap-1 rounded-full bg-foreground/[0.05] px-2 py-1 text-muted-foreground ring-1 ring-inset ring-foreground/10",
                        title: translate("git-status-modified"),
                        LineIconView { icon: LineIcon::File, class: "size-3 shrink-0".to_string() }
                        span { class: "font-mono tabular-nums text-foreground", "{boundary.changed_files}" }
                    }
                }
                if boundary.ahead > 0 {
                    Badge {
                        class: "gap-1 rounded-full bg-primary/10 px-2 py-1 font-mono tabular-nums text-primary ring-1 ring-inset ring-primary/20",
                        title: translate("composer-commits-ahead"),
                        "↑{boundary.ahead}"
                    }
                }
                Badge {
                    class: "gap-1.5 rounded-full bg-foreground/[0.05] px-2 py-1 font-mono tabular-nums ring-1 ring-inset ring-foreground/10",
                    span { class: "text-success", "+{boundary.insertions}" }
                    span { class: "text-destructive", "−{boundary.deletions}" }
                }
            }
            if !boundary.effective_dir.is_empty() {
                div { class: "mt-2 flex min-w-0 items-center gap-1.5 text-muted-foreground/65",
                    LineIconView { icon: LineIcon::ExternalLink, class: "size-3 shrink-0".to_string() }
                    span {
                        dir: "rtl",
                        class: "min-w-0 truncate text-left font-mono text-[9px]",
                        title: "{boundary.effective_dir}",
                        "{boundary.effective_dir}"
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectTree(project: ActiveWorkspaceProject, pane_id: u64) -> Element {
    let root = project.root;
    let tree_path = root.path.clone();
    let choices = project.choices;
    let mut choosing = use_signal(|| false);
    rsx! {
        div { class: "relative min-w-0 rounded-md bg-foreground/[0.035]",
            div { class: "group relative flex min-w-0 items-center rounded-md transition-colors hover:bg-glass-hover",
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md py-1.5 pl-2 pr-10 text-left text-muted-foreground transition-colors group-hover:text-foreground",
                    title: "{root.display_path}",
                    onclick: move |_| {
                        choosing.set(false);
                        let _ = send(&ProjectTreeToggle {
                            path: tree_path.clone(),
                            pane_id: pane_id.to_string(),
                        });
                    },
                    Icon {
                        class: if root.expanded { SIDEBAR_TREE_CHEVRON_OPEN } else { SIDEBAR_TREE_CHEVRON_CLOSED },
                        path { d: "m9 18 6-6-6-6" }
                    }
                    if root.is_worktree {
                        LineIconView { icon: LineIcon::GitFork, class: "size-3.5 shrink-0".to_string() }
                    } else {
                        BuiltinIconView { icon: vmux_core::BuiltinIcon::Project, class: "size-3.5 shrink-0".to_string() }
                    }
                    div { class: "min-w-0 flex-1",
                        div { class: "truncate text-[10px] font-medium text-foreground", "{root.label}" }
                        if !root.branch.is_empty() {
                            div { class: "truncate font-mono text-[9px] text-muted-foreground", "{root.branch}" }
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: "absolute right-1 top-1 z-10 flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
                    title: translate("git-switch-workspace"),
                    aria_label: translate("git-switch-workspace"),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        choosing.set(!choosing());
                    },
                    LineIconView { icon: LineIcon::ChevronsUpDown, class: "size-3.5".to_string() }
                }
            }
            if choosing() {
                div { class: "max-h-64 overflow-y-auto border-t border-foreground/[0.06] p-1",
                    for choice in choices {
                        ActiveWorkspaceChoice {
                            key: "{choice.path}",
                            project: choice,
                            pane_id,
                            on_pick: move |_| choosing.set(false),
                        }
                    }
                }
            }
            SidebarTreeChildren { expanded: root.expanded,
                div { class: "border-t border-foreground/[0.06] py-1",
                    for child in project.children {
                        ActiveWorkspaceProjectRow {
                            key: "{child.path}",
                            project: child,
                            pane_id,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ActiveWorkspaceChoice(project: ProjectRow, pane_id: u64, on_pick: EventHandler<()>) -> Element {
    let path = project.path.clone();
    rsx! {
        button {
            r#type: "button",
            class: if project.is_active {
                "flex w-full min-w-0 items-center gap-2 rounded-md bg-primary/[0.10] px-2 py-1.5 text-left text-foreground"
            } else {
                "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
            },
            title: "{project.display_path}",
            onclick: move |_| {
                on_pick.call(());
                let _ = send(&vmux_core::event::space::ProjectRequest::Activate {
                    path: path.clone(),
                    branch: String::new(),
                    checkout: String::new(),
                    pane_id: Some(pane_id),
                });
            },
            BuiltinIconView {
                icon: vmux_core::BuiltinIcon::Project,
                class: "size-3.5 shrink-0".to_string(),
            }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-[10px] font-medium", "{project.label}" }
                if !project.branch.is_empty() {
                    div { class: "truncate font-mono text-[9px] text-muted-foreground", "{project.branch}" }
                }
            }
            if project.is_active {
                span { class: "shrink-0 rounded-full bg-primary/10 px-1.5 py-0.5 text-[8px] font-semibold text-primary", {translate("common-current")} }
            } else if project.is_worktree {
                LineIconView {
                    icon: LineIcon::GitFork,
                    class: "size-3.5 shrink-0 text-muted-foreground".to_string(),
                }
            }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectRow(project: ProjectRow, pane_id: u64) -> Element {
    let path = project.path.clone();
    let opens_tree = project.kind.opens_a_tree();
    rsx! {
        SidebarTreeRowGroup {
            SidebarTreeRow {
                path: project.path.clone(),
                label: project.label.clone(),
                is_dir: opens_tree,
                expanded: project.expanded,
                depth: project.depth,
                title: project.display_path.clone(),
                on_activate: move |()| {
                    if opens_tree {
                        let _ = send(&ProjectTreeToggle {
                            path: path.clone(),
                            pane_id: pane_id.to_string(),
                        });
                    } else {
                        let _ = send(&SideSheetProjectOpenRequest {
                            pane_id,
                            path: path.clone(),
                        });
                    }
                },
            }
        }
    }
}
