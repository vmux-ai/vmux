#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ecs::event::{ProjectPickerRequest, ProjectRow, ProjectRowKind};
use vmux_ui::components::avatar::IdentityOrb;
use vmux_ui::components::dropdown::{DropdownPlacement, DropdownPopup};
use vmux_ui::components::project_picker::ProjectSelectPopup;
use vmux_ui::components::sheet::SheetSelectorRow;
use vmux_ui::hooks::{send, use_optimistic_map};
use vmux_ui::i18n::translate;

use crate::event::{
    ActiveSession, ActiveWorkspaceProject, SideSheetProjectPageOpenRequest, TabBoundary,
    WorkspaceBranch,
};

use super::LayoutDropdown;
use super::stack::StackIcon;
use super::state::LayoutUi;

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
    let projects = LayoutUi::current().use_selector(|state| state.projects.clone())();
    let project =
        project.unwrap_or_else(|| ActiveWorkspaceProject::available(projects, boundary.as_ref()));
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
        div { class: "relative w-full max-w-full min-w-0 overflow-visible py-2 pr-2 @max-[12rem]:py-1.5 @max-[12rem]:pr-1",
            div { class: "flex w-full min-w-0 items-center gap-2.5 overflow-hidden px-2 @max-[12rem]:gap-1.5 @max-[12rem]:px-1",
                div { class: "flex size-6 shrink-0 items-center justify-center",
                    StackIcon {
                        icon: page.icon.clone(),
                        url: page.url.clone(),
                        title: title.clone(),
                    }
                }
                div { class: "w-0 min-w-0 flex-1 overflow-hidden",
                    div { class: "line-clamp-2 block max-w-full text-ui font-semibold leading-snug text-foreground", title: "{title}", "{title}" }
                    if let Some(task) = task.as_ref() {
                        if !task.description.trim().is_empty() {
                            div { class: "truncate-fade text-[10px] text-muted-foreground", title: "{task.description}", "{task.description}" }
                        } else {
                            CurrentPagePath { url: page.url.clone() }
                        }
                    } else {
                        CurrentPagePath { url: page.url.clone() }
                    }
                }
            }
            div { class: "mt-2 min-w-0 flex flex-col gap-1",
                ActiveWorkspaceProjectTree {
                    project,
                    pane_id,
                }
            }
            if let Some(boundary) = boundary {
                if boundary.is_git_repo && (boundary.insertions > 0 || boundary.deletions > 0) {
                    div { class: "flex h-7 min-w-0 items-center gap-2 px-2 font-mono text-[10px] tabular-nums",
                        span { class: "text-ansi-2", "+{boundary.insertions}" }
                        span { class: "text-ansi-1", "−{boundary.deletions}" }
                    }
                }
            }
        }
    }
}

impl ActiveWorkspaceProject {
    fn available(state: crate::event::TabBoundaryState, boundary: Option<&TabBoundary>) -> Self {
        let mut choices: Vec<ProjectRow> = Vec::new();
        for row in state.projects {
            if row.depth != 0 || row.missing || row.kind != ProjectRowKind::Project {
                continue;
            }
            if choices
                .iter()
                .any(|held| held.identity_seed() == row.identity_seed())
            {
                continue;
            }
            choices.push(row);
        }
        let root = if let Some(boundary) = boundary {
            choices
                .iter()
                .find(|row| row.path == boundary.effective_dir)
                .cloned()
                .unwrap_or_else(|| ProjectRow {
                    path: boundary.effective_dir.clone(),
                    label: if boundary.repository.is_empty() {
                        translate("agent-project-select")
                    } else {
                        boundary.repository.clone()
                    },
                    display_path: boundary.project_dir.clone(),
                    ..Default::default()
                })
        } else {
            ProjectRow {
                label: translate("agent-project-select"),
                ..Default::default()
            }
        };
        Self {
            root,
            choices,
            children: Vec::new(),
            project_dir: boundary
                .map(|value| value.project_dir.clone())
                .unwrap_or_default(),
            branch: boundary
                .map(|value| value.branch.clone())
                .unwrap_or_default(),
            branches: state.branches,
            expanded: false,
        }
    }
}

#[component]
fn CurrentPagePath(url: String) -> Element {
    if matches!(
        vmux_session::Route::parse(&url),
        Some(vmux_session::Route::Session(_))
    ) {
        return rsx! {};
    }
    let label = vmux_path::FileUrl::parse(&url)
        .and_then(|file| file.path())
        .or_else(|| vmux_git::GitUrl::parse(&url))
        .map(vmux_path::DisplayPath::project)
        .unwrap_or_else(|| url.clone());
    rsx! {
        div { class: "mt-0.5 flex w-fit max-w-full min-w-0 rounded-md bg-foreground/[0.035] px-1.5 py-0.5 text-[10px] text-muted-foreground", title: url,
            span { class: "min-w-0 truncate font-mono", dir: "ltr",
                "{label}"
            }
        }
    }
}

#[component]
fn ActiveWorkspaceProjectTree(project: ActiveWorkspaceProject, pane_id: u64) -> Element {
    let ActiveWorkspaceProject {
        root,
        children: _,
        choices,
        project_dir,
        branch: current_branch,
        branches,
        expanded: _,
    } = project;
    let mut choosing = use_signal(|| false);
    let mut choosing_branch = use_signal(|| false);
    let mut renaming = use_signal(|| None::<ProjectRow>);
    let mut name_draft = use_signal(String::new);
    let project_key = pane_id;
    let optimistic_project = use_optimistic_map(std::collections::HashMap::from([(
        project_key,
        root.clone(),
    )]));
    let branch_key = format!("{pane_id}:{project_dir}");
    let optimistic_branch = use_optimistic_map(std::collections::HashMap::from([(
        branch_key.clone(),
        current_branch.clone(),
    )]));
    let selected_project = optimistic_project.value(&project_key, &root);
    let selected_path = selected_project.path.clone();
    let selected_identity = selected_project.identity_seed().to_string();
    let selected_label = selected_project.label.clone();
    let selected_display_path = selected_project.display_path.clone();
    let selected_branch = optimistic_branch.value(&branch_key, &current_branch);
    let current_branch_label = if selected_branch.is_empty() {
        "HEAD".to_string()
    } else {
        selected_branch.clone()
    };
    let mut branch_choices = branches;
    if !selected_branch.is_empty()
        && !branch_choices
            .iter()
            .any(|branch| branch.branch == selected_branch)
    {
        branch_choices.insert(
            0,
            WorkspaceBranch {
                branch: selected_branch.clone(),
                checkout: project_dir.clone(),
                label: String::new(),
            },
        );
    }
    let project_choices = choices;
    rsx! {
        div { class: "relative w-full max-w-full min-w-0 self-stretch overflow-visible",
            div { class: "flex min-w-0 flex-col gap-0.5 py-1 @max-[12rem]:py-0.5",
                if let Some(project) = renaming() {
                    input {
                        class: "h-8 min-w-0 rounded-md bg-card px-2 text-ui text-foreground outline-none focus:ring-1 focus:ring-primary/40",
                        value: name_draft(),
                        aria_label: translate("project-rename"),
                        onmounted: move |event| { spawn(async move { let _ = event.data().set_focus(true).await; }); },
                        oninput: move |event| name_draft.set(event.value()),
                        onkeydown: move |event| match event.key() {
                            Key::Enter => {
                                event.prevent_default();
                                if !name_draft().trim().is_empty() {
                                    let _ = send(&vmux_api::space::ProjectRenameRequest { id: project.identity.clone(), name: name_draft() });
                                    renaming.set(None);
                                }
                            }
                            Key::Escape => { event.prevent_default(); renaming.set(None); }
                            _ => {}
                        },
                    }
                } else {
                LayoutDropdown {
                    id: format!("active-project-{pane_id}"),
                    open: choosing(),
                    dismiss_on_mouseleave: true,
                    class: "w-full min-w-0".to_string(),
                    on_dismiss: move |_| choosing.set(false),
                    SheetSelectorRow {
                        label: selected_label.clone(),
                        title: selected_display_path.clone(),
                        open: choosing(),
                        leading: rsx! { div { class: "flex size-6 shrink-0 items-center justify-center", IdentityOrb {
                            seed: selected_identity,
                            alt: selected_label.clone(),
                            class: "size-4".to_string(),
                        } } },
                        on_hover_open: move |_| {
                            choosing_branch.set(false);
                            choosing.set(true);
                        },
                        onclick: move |_| {
                            choosing_branch.set(false);
                            choosing.set(true);
                        },
                    }
                    if choosing() {
                        ProjectSelectPopup {
                            projects: project_choices.clone(),
                            selected_path: selected_path.clone(),
                            match_width: true,
                            placement: DropdownPlacement::Right,
                            class: "max-h-64 min-w-0 overflow-y-auto".to_string(),
                            on_pick: move |project: ProjectRow| {
                                choosing.set(false);
                                optimistic_project.set(
                                    project_key,
                                    root.clone(),
                                    project.clone(),
                                );
                                if send(&vmux_ecs::event::space::ProjectActivateRequest {
                                    path: project.path,
                                    branch: String::new(),
                                    checkout: String::new(),
                                    pane_id: Some(pane_id),
                                }).is_err() {
                                    optimistic_project.clear(&project_key);
                                }
                            },
                            on_add: move |_| {
                                choosing.set(false);
                                let _ = send(&ProjectPickerRequest {
                                    current_dir: String::new(),
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
                            on_rename_selected: move |project: ProjectRow| {
                                choosing.set(false);
                                name_draft.set(project.label.clone());
                                renaming.set(Some(project));
                            },
                        }
                    }
                }
                }
                if !selected_branch.is_empty() || !branch_choices.is_empty() {
                        LayoutDropdown {
                            id: format!("active-branch-{pane_id}"),
                            open: choosing_branch(),
                            dismiss_on_mouseleave: true,
                            class: "w-full min-w-0".to_string(),
                            on_dismiss: move |_| choosing_branch.set(false),
                            SheetSelectorRow {
                                label: current_branch_label.clone(),
                                title: current_branch_label.clone(),
                                open: choosing_branch(),
                                label_class: "font-mono font-normal".to_string(),
                                leading: rsx! { div { class: "flex size-6 shrink-0 items-center justify-center", IdentityOrb {
                                    seed: selected_branch.clone(), alt: selected_branch.clone(), class: "size-4".to_string(),
                                } } },
                                on_hover_open: move |_| {
                                    choosing.set(false);
                                    choosing_branch.set(true);
                                },
                                onclick: move |_| {
                                    choosing.set(false);
                                    choosing_branch.set(true);
                                },
                            }
                            if choosing_branch() {
                                DropdownPopup { match_width: true, placement: DropdownPlacement::Right, class: "max-h-64 min-w-0 overflow-y-auto p-1",
                                    for branch in branch_choices {
                                        {
                                            let branch_name = branch.branch.clone();
                                            let checkout = branch.checkout.clone();
                                            let project_path = project_dir.clone();
                                            let observed_branch = current_branch.clone();
                                            let optimistic_key = branch_key.clone();
                                            let selected = branch_name == selected_branch;
                                            rsx! {
                                                ActiveWorkspaceBranchChoice {
                                                    key: "{branch.branch}",
                                                    branch,
                                                    selected,
                                                    on_pick: move |_| {
                                                        choosing_branch.set(false);
                                                        if selected {
                                                            return;
                                                        }
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
                                                        }).is_err() {
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
    }
}

#[component]
fn ActiveWorkspaceBranchChoice(
    branch: WorkspaceBranch,
    selected: bool,
    on_pick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
            onclick: move |_| on_pick.call(()),
            IdentityOrb { seed: branch.branch.clone(), alt: branch.branch.clone(), class: "size-3.5".to_string() }
            span { class: "min-w-0 flex-1 truncate font-mono text-[10px]", "{branch.branch}" }
            if !branch.label.is_empty() {
                span { class: "max-w-24 truncate text-[9px] text-muted-foreground", "{branch.label}" }
            }
            if selected {
                vmux_ui::icon::LineIconView { icon: vmux_ui::icon::LineIcon::Check, class: "ml-auto size-3.5 shrink-0" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use vmux_ui::transport::{
        BytesListener, HostBinding, PageHost, event_listener::EventListenerError,
    };

    struct Host;

    impl PageHost for Host {
        fn send(&self, _: &str, _: &[u8]) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[component]
    fn Controls(url: String) -> Element {
        let layout = LayoutUi::use_state();
        layout.provide();
        rsx! { ActiveSessionPanel {
            session: ActiveSession {
                page: crate::event::StackNode { id: 1, agent_id: None, title: "Page".into(), url, icon: Default::default(), is_active: true, is_loading: false, is_dirty: false, bg_color: None },
                task: None, agent: None, project: None, boundary: None, pane_id: 10,
            }
        } }
    }

    #[test]
    fn pages_without_an_assigned_workspace_keep_the_project_selector() {
        for url in [
            "vmux://projects/github.com/vmux-ai/vmux-cloud",
            "vmux://start/",
            "vmux://error/",
        ] {
            let mut dom = VirtualDom::new_with_props(Controls, ControlsProps { url: url.into() });
            dom.provide_root_context(HostBinding(Rc::new(Host)));
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains(&translate("agent-project-select")), "{url}");
        }
    }
}
