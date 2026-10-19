#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::{ProjectBranch, ProjectPickerRequest, ProjectRow, ProjectTreeToggle};
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
                if let Some(project) = project {
                    ActiveWorkspaceProjectTree {
                        project,
                        boundary: boundary.clone().filter(|boundary| boundary.is_git_repo),
                        pane_id,
                    }
                } else if let Some(boundary) = boundary {
                    if boundary.is_git_repo {
                        ActiveSessionGit { boundary }
                    }
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
    rsx! {
        div { class: "flex h-8 min-w-0 items-center gap-1 rounded-md bg-foreground/[0.035] px-2.5 text-[10px]",
            GitSymbolView {
                symbol: GitSymbol::Repository,
                class: "size-3.5 shrink-0 text-muted-foreground".to_string(),
            }
            span { class: "min-w-0 truncate font-medium text-foreground", title: "{repository}", "{repository}" }
            span { class: "shrink-0 text-xs text-muted-foreground/45", "›" }
            LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
            span { class: "min-w-0 truncate font-mono text-foreground/85", "{boundary.branch}" }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectTree(
    project: ActiveWorkspaceProject,
    boundary: Option<TabBoundary>,
    pane_id: u64,
) -> Element {
    let root = project.root;
    let picker_root = root.path.clone();
    let choices = project.choices;
    let mut choosing = use_signal(|| false);
    let mut choosing_branch = use_signal(|| false);
    let mut expanded = use_signal(|| true);
    let branches = boundary
        .as_ref()
        .map(|boundary| boundary.branches.clone())
        .unwrap_or_default();
    let current_branch = boundary
        .as_ref()
        .map(|boundary| boundary.branch.clone())
        .unwrap_or_default();
    let branch_choices = branches
        .into_iter()
        .filter(|branch| branch.branch != current_branch)
        .collect::<Vec<_>>();
    let project_choices = choices
        .into_iter()
        .filter(|choice| !choice.is_active)
        .collect::<Vec<_>>();
    let toggle_label = if expanded() {
        translate("common-collapse")
    } else {
        translate("common-expand")
    };
    rsx! {
        div { class: "min-w-0 overflow-hidden rounded-md bg-foreground/[0.035]",
            div { class: "flex h-8 min-w-0 items-center gap-1 px-2",
                Dropdown {
                    open: choosing(),
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| choosing.set(false),
                    button {
                        r#type: "button",
                        class: "flex h-7 min-w-0 items-center gap-1.5 rounded px-1.5 text-left text-foreground hover:bg-foreground/[0.055]",
                        title: "{root.display_path}",
                        aria_expanded: choosing(),
                        onclick: move |_| choosing.set(!choosing()),
                        TypeIcon {
                            path: root.path.clone(),
                            is_dir: true,
                            class: "size-3.5 shrink-0".to_string(),
                        }
                        span { class: "min-w-0 truncate text-[10px] font-medium", "{root.label}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0 text-muted-foreground".to_string() }
                    }
                    if choosing() {
                        div { class: "absolute left-0 top-8 z-30 max-h-64 min-w-56 overflow-y-auto rounded-md bg-background p-1 shadow-xl",
                            for choice in project_choices.iter() {
                                ActiveWorkspaceChoice {
                                    key: "{choice.path}",
                                    project: choice.clone(),
                                    pane_id,
                                    on_pick: move |_| choosing.set(false),
                                }
                            }
                            if !project_choices.is_empty() {
                                div { class: "mx-1 my-1 h-px bg-foreground/[0.08]" }
                            }
                            button {
                                r#type: "button",
                                class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[10px] font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                                onclick: move |_| {
                                    choosing.set(false);
                                    let _ = send(&ProjectPickerRequest {
                                        current_dir: picker_root.clone(),
                                        pane_id: Some(pane_id),
                                    });
                                },
                                LineIconView { icon: LineIcon::Plus, class: "size-3.5 shrink-0".to_string() }
                                span { {translate("common-add")} }
                            }
                        }
                    }
                }
                if !current_branch.is_empty() {
                    span { class: "shrink-0 text-xs text-muted-foreground/45", "›" }
                    if branch_choices.is_empty() {
                        LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                        span { class: "min-w-0 truncate font-mono text-[10px] text-foreground/85", "{current_branch}" }
                    } else {
                        Dropdown {
                            open: choosing_branch(),
                            class: "min-w-0".to_string(),
                            on_dismiss: move |_| choosing_branch.set(false),
                            button {
                                r#type: "button",
                                class: "flex h-7 min-w-0 items-center gap-1.5 rounded px-1.5 text-left hover:bg-foreground/[0.055]",
                                aria_expanded: choosing_branch(),
                                onclick: move |_| choosing_branch.set(!choosing_branch()),
                                LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                                span { class: "min-w-0 truncate font-mono text-[10px] text-foreground/85", "{current_branch}" }
                                LineIconView { icon: LineIcon::ChevronDown, class: "size-3 shrink-0 text-muted-foreground".to_string() }
                            }
                            if choosing_branch() {
                                div { class: "absolute left-0 top-8 z-30 max-h-64 min-w-56 overflow-y-auto rounded-md bg-background p-1 shadow-xl",
                                    for branch in branch_choices {
                                        ActiveWorkspaceBranchChoice {
                                            key: "{branch.branch}",
                                            project_path: root.path.clone(),
                                            branch,
                                            pane_id,
                                            on_pick: move |_| choosing_branch.set(false),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "ml-auto shrink-0",
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
                div { class: "min-h-0 overflow-hidden pb-1",
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
fn ActiveWorkspaceBranchChoice(
    project_path: String,
    branch: ProjectBranch,
    pane_id: u64,
    on_pick: EventHandler<()>,
) -> Element {
    let branch_name = branch.branch.clone();
    let checkout = branch.checkout.clone();
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
            onclick: move |_| {
                on_pick.call(());
                let _ = send(&vmux_ecs::event::space::ProjectActivateRequest {
                    path: project_path.clone(),
                    branch: branch_name.clone(),
                    checkout: checkout.clone(),
                    pane_id: Some(pane_id),
                });
            },
            LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0".to_string() }
            span { class: "min-w-0 flex-1 truncate font-mono text-[10px]", "{branch.branch}" }
            if !branch.label.is_empty() {
                span { class: "max-w-24 truncate text-[9px] text-muted-foreground", "{branch.label}" }
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
