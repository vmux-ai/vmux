#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::{ProjectRow, ProjectTreeToggle};
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::badge::Badge;
use vmux_ui::components::composer_bar::StatusDot;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::tree_row::{SidebarTreeRow, SidebarTreeRowGroup};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{translate, translate_humanized};
use vmux_ui::icon::{
    BuiltinIconView, GitSymbol, GitSymbolView, LineIcon, LineIconView, PageIconView,
};

use crate::event::{
    ActiveSession, ActiveWorkspaceProject, SideSheetProjectOpenRequest, TabBoundary,
};

#[component]
pub(crate) fn ActiveSessionPanel(session: ActiveSession) -> Element {
    let ActiveSession {
        page,
        task,
        agent,
        project,
        boundary,
        pane_id,
    } = session;
    let title = if task
        .as_ref()
        .is_some_and(|task| !task.name.trim().is_empty())
    {
        task.as_ref().unwrap().name.clone()
    } else if page.title.trim().is_empty() {
        page.url.clone()
    } else {
        page.title.clone()
    };
    rsx! {
        div { class: "px-2 py-2",
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
                    if let Some(task) = task.as_ref() {
                        if !task.description.trim().is_empty() {
                            div { class: "truncate text-[10px] text-muted-foreground", title: "{task.description}", "{task.description}" }
                        } else {
                            div { class: "truncate text-[10px] text-muted-foreground", title: "{page.url}", "{page.url}" }
                        }
                    } else {
                        div { class: "truncate text-[10px] text-muted-foreground", title: "{page.url}", "{page.url}" }
                    }
                }
            }
            div { class: "mt-2 flex flex-col gap-1",
                if let Some(task) = task {
                    div { class: "flex items-center gap-1.5",
                        Badge {
                            class: "rounded-full bg-foreground/[0.05] px-2 py-1 text-[9px] text-muted-foreground ring-1 ring-inset ring-foreground/10",
                            {translate_humanized(&task.stage_name)}
                        }
                        if task.runtime != "inactive" {
                            span { class: "flex items-center gap-1.5 text-[10px] text-muted-foreground",
                                StatusDot { status: task.runtime.clone(), size_class: "size-1.5".to_string() }
                                {session_runtime_label(&task.runtime)}
                            }
                        }
                    }
                }
                if let Some(agent) = agent {
                    div { class: "flex min-w-0 items-center gap-2 rounded-md bg-foreground/[0.035] px-2 py-1.5",
                        AgentThumbnail {
                            src: agent.icon.clone(),
                            seed: agent.name.clone(),
                            background: agent.color.clone(),
                            alt: agent.name.clone(),
                            class: "size-4 text-[7px]",
                        }
                        span { class: "min-w-0 flex-1 truncate text-[10px] font-medium text-foreground", "{agent.name}" }
                    }
                }
                if let Some(boundary) = boundary {
                    if boundary.is_git_repo {
                        ActiveSessionGit { boundary }
                    }
                }
                if let Some(project) = project {
                    ActiveWorkspaceProjectTree { project, pane_id }
                }
            }
        }
    }
}

fn session_runtime_label(runtime: &str) -> String {
    let id = match runtime {
        "idle" => "session-runtime-idle",
        "streaming" => "session-runtime-streaming",
        "awaiting" => "session-runtime-awaiting",
        "errored" => "session-runtime-errored",
        _ => "session-runtime-inactive",
    };
    translate(id)
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
        format!("{} ← {}", boundary.base_ref, branch)
    };
    rsx! {
        div { class: "min-w-0 rounded-md px-2.5 py-2.5 ring-1 ring-inset ring-foreground/[0.11]",
            div { class: "flex min-w-0 items-center gap-2",
                GitSymbolView {
                    symbol: GitSymbol::Repository,
                    class: "size-3.5 shrink-0".to_string(),
                }
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
        }
    }
}

#[component]
fn ActiveWorkspaceProjectTree(project: ActiveWorkspaceProject, pane_id: u64) -> Element {
    let root = project.root;
    let choices = project.choices;
    let mut choosing = use_signal(|| false);
    let mut expanded = use_signal(|| true);
    let toggle_label = if expanded() {
        translate("common-collapse")
    } else {
        translate("common-expand")
    };
    rsx! {
        div { class: "flex min-w-0 flex-col gap-1",
            Dropdown {
                open: choosing(),
                class: "min-w-0".to_string(),
                on_dismiss: move |_| choosing.set(false),
                button {
                    r#type: "button",
                    class: "flex h-8 w-full min-w-0 cursor-pointer items-center gap-2 rounded-md bg-foreground/[0.035] px-2.5 text-left text-muted-foreground transition-colors hover:bg-glass-hover hover:text-foreground",
                    title: "{root.display_path}",
                    aria_expanded: choosing(),
                    onclick: move |_| choosing.set(!choosing()),
                    TypeIcon {
                        path: root.path.clone(),
                        is_dir: true,
                        class: "size-3.5 shrink-0".to_string(),
                    }
                    span { class: "truncate-start min-w-0 flex-1 font-mono text-[10px] text-foreground", "{root.display_path}" }
                    LineIconView {
                        icon: LineIcon::ChevronsUpDown,
                        class: "size-3.5 shrink-0".to_string(),
                    }
                }
                if choosing() {
                    div { class: "absolute left-0 right-0 top-9 z-30 max-h-64 overflow-y-auto rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
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
            }
            div { class: "min-w-0 rounded-md bg-foreground/[0.035] py-1.5",
                div { class: "flex items-center gap-1 px-2 pb-1",
                    span {
                        class: "inline-flex min-w-0 max-w-full items-center rounded-full bg-foreground/[0.065] px-2 py-0.5 text-[9px] font-medium text-foreground/85 ring-1 ring-inset ring-foreground/[0.07]",
                        span { class: "truncate", "{root.label}" }
                    }
                    div { class: "ml-auto",
                        SheetHeaderChevron {
                            expanded: expanded(),
                            label: toggle_label,
                            onclick: move |_| expanded.set(!expanded()),
                        }
                    }
                }
                div { class: if expanded() {
                        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                    } else {
                        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                    },
                    div { class: "min-h-0 overflow-hidden",
                        if !project.children.is_empty() {
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
                let _ = send(&vmux_ecs::event::space::ProjectActivateRequest {
                    path: path.clone(),
                    branch: String::new(),
                    checkout: String::new(),
                    pane_id: Some(pane_id),
                });
            },
            BuiltinIconView {
                icon: vmux_api::BuiltinIcon::Project,
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
    let opens_tree = matches!(
        project.kind,
        vmux_api::space::ProjectRowKind::Project | vmux_api::space::ProjectRowKind::Directory
    );
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
