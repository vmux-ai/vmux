#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::{ProjectPickerRequest, ProjectRow, ProjectTreeToggle};
use vmux_ui::components::avatar::IdentityOrb;
use vmux_ui::components::dropdown::DropdownPopup;
use vmux_ui::components::motion::Collapsible;
use vmux_ui::components::project_picker::ProjectSelectPopup;
use vmux_ui::components::sheet::{SheetSelectorRow, SheetToggleRow};
use vmux_ui::components::tree_row::{FileTreeRow, FileTreeRowGroup};
use vmux_ui::hooks::{send, use_optimistic_map};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView, PageIconView};

use crate::event::{
    ActiveSession, ActiveWorkspaceProject, SideSheetProjectOpenRequest,
    SideSheetProjectPageOpenRequest, SideSheetSectionRequest, TabBoundary, WorkspaceBranch,
};

use super::LayoutDropdown;

#[component]
pub(crate) fn ActiveSessionPanel(session: ActiveSession) -> Element {
    let ActiveSession {
        page,
        task,
        project,
        boundary,
        pane_id,
        ..
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
        div { class: "relative w-full max-w-full min-w-0 overflow-visible py-2 @max-[12rem]:py-1.5",
            div { class: "flex w-full min-w-0 items-center gap-2.5 overflow-hidden px-2 @max-[12rem]:gap-1.5 @max-[12rem]:px-1",
                div { class: "flex size-4 shrink-0 items-center justify-center",
                    PageIconView {
                        icon: page.icon.clone(),
                        url: page.url.clone(),
                        img_class: "size-4 shrink-0 rounded-sm object-contain".to_string(),
                        icon_class: "size-4 shrink-0 text-muted-foreground".to_string(),
                    }
                }
                div { class: "w-0 min-w-0 flex-1 overflow-hidden",
                    div { class: "block max-w-full truncate text-ui font-semibold text-foreground", title: "{title}", "{title}" }
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
            div { class: "mt-2 min-w-0 flex flex-col gap-1",
                if let Some(project) = project {
                    ActiveWorkspaceProjectTree {
                        project,
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

#[component]
fn ActiveSessionGit(boundary: TabBoundary) -> Element {
    let repository = if boundary.repository.is_empty() {
        translate("composer-git-repository")
    } else {
        boundary.repository.clone()
    };
    rsx! {
        div { class: "flex h-8 min-w-0 items-center gap-1 rounded-md bg-foreground/[0.035] px-2 text-[10px] @max-[12rem]:px-1",
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
fn ActiveWorkspaceProjectTree(project: ActiveWorkspaceProject, pane_id: u64) -> Element {
    let ActiveWorkspaceProject {
        root,
        children,
        choices,
        project_dir,
        branch: current_branch,
        branches,
        expanded,
    } = project;
    let picker_root = root.path.clone();
    let mut choosing = use_signal(|| false);
    let mut choosing_branch = use_signal(|| false);
    let mut optimistic_project = use_signal(|| None::<ProjectRow>);
    let branch_key = format!("{pane_id}:{project_dir}");
    let optimistic_branch = use_optimistic_map(std::collections::HashMap::from([(
        branch_key.clone(),
        current_branch.clone(),
    )]));
    let selected_project = optimistic_project().unwrap_or_else(|| root.clone());
    let selected_path = selected_project.path.clone();
    let selected_label = selected_project.label.clone();
    let selected_display_path = selected_project.display_path.clone();
    use_effect(move || {
        if optimistic_project
            .peek()
            .as_ref()
            .is_some_and(|project| project.path == root.path)
        {
            optimistic_project.set(None);
        }
    });
    let selected_branch = optimistic_branch.value(&branch_key, &current_branch);
    let current_branch_label = if selected_branch.is_empty() {
        "HEAD".to_string()
    } else {
        selected_branch.clone()
    };
    let branch_choices = branches
        .into_iter()
        .filter(|branch| branch.branch != selected_branch)
        .collect::<Vec<_>>();
    let project_choices = choices;
    rsx! {
        div { class: "relative w-full max-w-full min-w-0 self-stretch overflow-visible",
            div { class: "flex min-w-0 flex-col gap-0.5 py-1 @max-[12rem]:py-0.5",
                LayoutDropdown {
                    id: format!("active-project-{pane_id}"),
                    open: choosing(),
                    class: "w-full min-w-0".to_string(),
                    on_dismiss: move |_| choosing.set(false),
                    SheetSelectorRow {
                        label: selected_label.clone(),
                        title: selected_display_path.clone(),
                        open: choosing(),
                        leading: rsx! { IdentityOrb {
                            seed: selected_path.clone(),
                            alt: selected_label.clone(),
                            class: "size-4".to_string(),
                        } },
                        onclick: move |_| choosing.set(!choosing()),
                    }
                    if choosing() {
                        ProjectSelectPopup {
                            projects: project_choices.clone(),
                            selected_path: selected_path.clone(),
                            match_width: true,
                            class: "max-h-64 min-w-0 overflow-y-auto".to_string(),
                            on_pick: move |project: ProjectRow| {
                                choosing.set(false);
                                optimistic_project.set(Some(project.clone()));
                                let _ = send(&vmux_ecs::event::space::ProjectActivateRequest {
                                    path: project.path,
                                    branch: String::new(),
                                    checkout: String::new(),
                                    pane_id: Some(pane_id),
                                });
                            },
                            on_add: move |_| {
                                choosing.set(false);
                                let _ = send(&ProjectPickerRequest {
                                    current_dir: picker_root.clone(),
                                    pane_id: Some(pane_id),
                                });
                            },
                            on_open_selected: move |project: ProjectRow| {
                                choosing.set(false);
                                let _ = send(&SideSheetProjectPageOpenRequest {
                                    pane_id,
                                    path: project.path,
                                });
                            },
                        }
                    }
                }
                if !current_branch.is_empty() || !branch_choices.is_empty() {
                    if branch_choices.is_empty() {
                        div { class: "flex h-8 w-full min-w-0 items-center gap-2 rounded px-2 transition-colors hover:bg-foreground/[0.04] @max-[12rem]:px-1",
                            LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                            span { class: "min-w-0 flex-1 truncate font-mono text-[10px] text-foreground/85", "{current_branch}" }
                        }
                    } else {
                        LayoutDropdown {
                            id: format!("active-branch-{pane_id}"),
                            open: choosing_branch(),
                            class: "w-full min-w-0".to_string(),
                            on_dismiss: move |_| choosing_branch.set(false),
                            SheetSelectorRow {
                                label: current_branch_label.clone(),
                                open: choosing_branch(),
                                label_class: "font-mono font-normal".to_string(),
                                leading: rsx! { LineIconView {
                                    icon: LineIcon::GitBranch,
                                    class: "size-3.5 shrink-0 text-muted-foreground".to_string(),
                                } },
                                onclick: move |_| choosing_branch.set(!choosing_branch()),
                            }
                            if choosing_branch() {
                                DropdownPopup { match_width: true, class: "max-h-64 min-w-0 overflow-y-auto p-1",
                                    for branch in branch_choices {
                                        {
                                            let branch_name = branch.branch.clone();
                                            let checkout = branch.checkout.clone();
                                            let project_path = project_dir.clone();
                                            let observed_branch = current_branch.clone();
                                            let optimistic_key = branch_key.clone();
                                            rsx! {
                                                ActiveWorkspaceBranchChoice {
                                                    key: "{branch.branch}",
                                                    branch,
                                                    on_pick: move |_| {
                                                        choosing_branch.set(false);
                                                        optimistic_branch.set(
                                                            optimistic_key.clone(),
                                                            observed_branch.clone(),
                                                            branch_name.clone(),
                                                        );
                                                        if send(&vmux_ecs::event::space::ProjectActivateRequest {
                                                            path: project_path.clone(),
                                                            branch: branch_name.clone(),
                                                            checkout: checkout.clone(),
                                                            pane_id: Some(pane_id),
                                                        })
                                                        .is_err()
                                                        {
                                                            optimistic_branch.clear(&optimistic_key);
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SheetToggleRow {
                label: translate("git-files"),
                expanded,
                leading: rsx! { LineIconView {
                    icon: LineIcon::File,
                    class: "size-3.5 shrink-0 text-muted-foreground".to_string(),
                } },
                onclick: move |_| {
                    let _ = send(&SideSheetSectionRequest::new(
                        pane_id,
                        "projects",
                        !expanded,
                    ));
                },
            }
            Collapsible { expanded, content_class: "pb-1".to_string(),
                for child in children {
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

#[component]
fn ActiveWorkspaceBranchChoice(branch: WorkspaceBranch, on_pick: EventHandler<()>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
            onclick: move |_| on_pick.call(()),
            LineIconView { icon: LineIcon::GitBranch, class: "size-3.5 shrink-0".to_string() }
            span { class: "min-w-0 flex-1 truncate font-mono text-[10px]", "{branch.branch}" }
            if !branch.label.is_empty() {
                span { class: "max-w-24 truncate text-[9px] text-muted-foreground", "{branch.label}" }
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
        FileTreeRowGroup {
            FileTreeRow {
                path: project.path.clone(),
                label: project.label.clone(),
                is_dir: opens_tree,
                expanded: project.expanded,
                depth: project.depth,
                title: project.display_path.clone(),
                on_activate: move |_| {
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
