#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::space::ProjectRow;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::avatar::IdentityOrb;
use vmux_ui::components::breadcrumb::{BreadcrumbOption, BreadcrumbSeparator};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dropdown::{Dropdown, DropdownPopup, DropdownTrigger};
use vmux_ui::components::manager::{ManagerSearchAction, WorkspaceBreadcrumbHeader, WorkspacePage};
use vmux_ui::components::project_picker::{ProjectSelectPopup, ProjectSelectTrigger};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::scroll::ScrollPreload;

use crate::event::*;
use crate::state::{GitBranchCollection, GitCatalogProgress, GitPanel, HeaderDropdown};

use super::branches::{BranchCollectionTabs, BranchRow, TagRow};

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    open_dropdown: HeaderDropdown,
    projects: Vec<ProjectRow>,
    focused_panel: GitPanel,
    selected_branch: String,
    selected_branch_base: String,
    branch_collection: GitBranchCollection,
    selected_branch_collection: GitBranchCollection,
    selected_commit: String,
    can_push: bool,
    branches_progress: GitCatalogProgress,
    selected_branches_progress: GitCatalogProgress,
    commits_progress: GitCatalogProgress,
    viewed_count: usize,
    viewed_total: u32,
    file_query: Signal<String>,
) -> Element {
    let push_path = repository.repo_root.clone();
    let repository_open = open_dropdown == HeaderDropdown::Repository;
    let branch_open = open_dropdown == HeaderDropdown::Branch;
    let commit_open = open_dropdown == HeaderDropdown::Commit;
    let selected_commit_entry = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit)
        .cloned();
    let revision_label = selected_commit_entry
        .as_ref()
        .map(|commit| commit.short_sha.clone())
        .unwrap_or_default();
    let revision_title = selected_commit_entry
        .as_ref()
        .map(|commit| commit.summary.clone())
        .unwrap_or_default();
    let branch_label = if focused_panel == GitPanel::Branches {
        selected_branch.clone()
    } else {
        repository.branch.clone()
    };
    let branch_base = if focused_panel == GitPanel::Branches {
        selected_branch_base
    } else {
        String::new()
    };
    let checkout = (focused_panel == GitPanel::Branches)
        .then(|| selected_branch_collection.checkout(&repository, &selected_branch))
        .flatten();
    let checkout_required = focused_panel == GitPanel::Branches
        && checkout.as_ref().is_some_and(|checkout| !checkout.current);
    let checkout_branch = checkout
        .as_ref()
        .map(|checkout| checkout.branch.clone())
        .unwrap_or_default();
    let checkout_path = checkout
        .as_ref()
        .map(|checkout| checkout.checkout.clone())
        .unwrap_or_default();
    let checkout_start = checkout
        .as_ref()
        .map(|checkout| checkout.start_point.clone())
        .unwrap_or_default();
    let selected_reference_count = branch_collection.len(&repository);
    let selected_catalog = GitCatalog::from(branch_collection);
    let first_commit = repository
        .commits
        .first()
        .map(|commit| commit.sha.clone())
        .unwrap_or_default();
    let git_url = crate::GitUrl::from_path(std::path::Path::new(&repository.repo_root))
        .unwrap_or_else(|| crate::GIT_PAGE_URL.to_string());
    let editor_url = vmux_path::FileUrl::from_path(
        std::path::Path::new(&repository.repo_root),
        None,
        None,
        None,
    );
    let current_page = BreadcrumbOption {
        label: translate("git-title"),
        value: git_url.clone(),
        identity_seed: String::new(),
        icon_url: "assets/favicons/git.svg".to_string(),
        icon: Some(vmux_api::BuiltinIcon::GitBranch),
    };
    let page_options = WorkspacePage::options_with(vec![
        current_page.clone(),
        BreadcrumbOption {
            label: translate("editor-editor"),
            value: editor_url,
            identity_seed: String::new(),
            icon_url: "assets/favicons/editor.svg".to_string(),
            icon: Some(vmux_api::BuiltinIcon::Code),
        },
    ]);
    let project_picker_dir = repository.repo_root.clone();
    let mut root_projects = projects
        .into_iter()
        .filter(|project| project.depth == 0 && !project.missing)
        .collect::<Vec<_>>();
    if !root_projects
        .iter()
        .any(|project| project.path == repository.repo_root)
    {
        root_projects.insert(
            0,
            ProjectRow {
                path: repository.repo_root.clone(),
                label: repository.repo_name.clone(),
                display_path: repository.repo_root.clone(),
                is_active: true,
                is_worktree: repository.is_worktree,
                branch: repository.branch.clone(),
                ..Default::default()
            },
        );
    }
    rsx! {
        WorkspaceBreadcrumbHeader {
                current: current_page,
                options: page_options,
                actions: rsx! {
                    RepositoryActions {
                        focused_panel,
                        file_query,
                        viewed_count,
                        viewed_total,
                        checkout_required,
                        checkout_branch,
                        checkout_path,
                        checkout_start,
                        push_path,
                        can_push,
                    }
                },
                BreadcrumbSeparator {}
                Dropdown {
                    open: repository_open,
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| {
                        send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                    },
                    ProjectSelectTrigger {
                        label: repository.repo_name.clone(),
                        title: repository.repo_root.clone(),
                        open: repository_open,
                        seed: repository.repo_root.clone(),
                        onclick: move |_| {
                            let open = if repository_open {
                                HeaderDropdown::Closed
                            } else {
                                HeaderDropdown::Repository
                            };
                            send(&HeaderDropdownRequest { open });
                        },
                    }
                    if repository_open {
                        ProjectSelectPopup {
                            projects: root_projects.clone(),
                            selected_path: repository.repo_root.clone(),
                            class: "max-h-72 min-w-64 overflow-y-auto".to_string(),
                            on_pick: move |project: ProjectRow| {
                                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                send(&GitRepositoryRequest { path: project.path });
                            },
                            on_add: move |_| {
                                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                send(&GitRepositoryPickerRequest {
                                    path: project_picker_dir.clone(),
                                });
                            },
                        }
                    }
                }
                BreadcrumbSeparator {}
                ViewPicker {
                    open_dropdown,
                    focused_panel,
                    changes: repository.files.len(),
                    commits: repository.commits.len(),
                    stashes: repository.stashes.len(),
                    first_commit,
                    commit_selected: selected_commit_entry.is_some(),
                }
                BreadcrumbSeparator {}
                Dropdown {
                    open: branch_open,
                    class: "min-w-0".to_string(),
                    on_dismiss: move |_| {
                        send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                    },
                    DropdownTrigger {
                        label: branch_label.clone(),
                        title: branch_label.clone(),
                        open: branch_open,
                        onclick: move |_| {
                            let open = if branch_open {
                                HeaderDropdown::Closed
                            } else {
                                HeaderDropdown::Branch
                            };
                            send(&HeaderDropdownRequest { open });
                        },
                        leading: rsx! {
                            IdentityOrb {
                                seed: branch_label.clone(),
                                alt: branch_label.clone(),
                                class: "size-4".to_string(),
                            }
                            if !branch_base.is_empty() && branch_base != branch_label {
                                span { class: "truncate text-muted-foreground", "{branch_base}" }
                                span { class: "shrink-0 text-muted-foreground/65", "←" }
                            }
                        },
                    }
                    if branch_open {
                        DropdownPopup {
                            class: "flex max-h-96 min-w-80 flex-col overflow-hidden p-1.5",
                            if focused_panel == GitPanel::Branches {
                                div { class: "flex shrink-0 items-center gap-2 px-1 pb-1.5",
                                    BranchCollectionTabs { branch_collection }
                                    span { class: "text-[9px] tabular-nums text-muted-foreground", "{selected_reference_count}" }
                                }
                                div {
                                    class: "min-h-0 overflow-y-auto",
                                    onscroll: move |event: Event<ScrollData>| {
                                        if ScrollPreload::LIST.reached(&event) && selected_branches_progress.has_more && !selected_branches_progress.loading {
                                            send(&GitCatalogNextRequest { catalog: selected_catalog });
                                        }
                                    },
                                    match branch_collection {
                                        GitBranchCollection::Local => rsx! {
                                            for (index, branch) in repository.branches.iter().cloned().enumerate() {
                                                {
                                                    let checked_out = GitBranchCollection::Local
                                                        .is_checked_out(&repository, &branch.name);
                                                    rsx! {
                                                        BranchRow {
                                                            key: "header-local-{branch.name}",
                                                            branch,
                                                            index,
                                                            remote: false,
                                                            selected_branch: selected_branch.clone(),
                                                            checked_out,
                                                            onselected: move |_| {
                                                                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        GitBranchCollection::Remote => rsx! {
                                            for (index, branch) in repository.remote_branches.iter().cloned().enumerate() {
                                                {
                                                    let checked_out = GitBranchCollection::Remote
                                                        .is_checked_out(&repository, &branch.name);
                                                    rsx! {
                                                        BranchRow {
                                                            key: "header-remote-{branch.name}",
                                                            branch,
                                                            index,
                                                            remote: true,
                                                            selected_branch: selected_branch.clone(),
                                                            checked_out,
                                                            onselected: move |_| {
                                                                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        GitBranchCollection::Tags => rsx! {
                                            for (index, tag) in repository.tags.iter().cloned().enumerate() {
                                                TagRow {
                                                    key: "header-tag-{tag.name}",
                                                    tag,
                                                    index,
                                                    selected_branch: selected_branch.clone(),
                                                    onselected: move |_| {
                                                        send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                    },
                                                }
                                            }
                                        },
                                    }
                                    if selected_reference_count > 0 && selected_branches_progress.loaded && selected_branches_progress.loading {
                                        div { class: "flex flex-col gap-2 px-2.5 py-2",
                                            for width in ["w-9/12", "w-11/12", "w-8/12"] {
                                                vmux_ui::components::skeleton::Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div {
                                    class: "min-h-0 overflow-y-auto",
                                    onscroll: move |event: Event<ScrollData>| {
                                        if ScrollPreload::LIST.reached(&event) && branches_progress.has_more && !branches_progress.loading {
                                            send(&GitCatalogNextRequest { catalog: GitCatalog::LocalBranches });
                                        }
                                    },
                                    for branch in repository.branches.iter() {
                                        {
                                            let name = branch.name.clone();
                                            rsx! {
                                                button {
                                                    key: "header-branch-{name}",
                                                    r#type: "button",
                                                    class: if branch.current {
                                                        "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
                                                    } else {
                                                        "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
                                                    },
                                                    onclick: move |_| {
                                                        send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                        send(&GitPanelSelectRequest { panel: GitPanel::Branches });
                                                        send(&GitBranchCollectionSelectRequest {
                                                            collection: GitBranchCollection::Local,
                                                        });
                                                        send(&GitBranchSelectRequest {
                                                            reference: name.clone(),
                                                        });
                                                    },
                                                    IdentityOrb {
                                                        seed: branch.name.clone(),
                                                        alt: branch.name.clone(),
                                                        class: "size-5".to_string(),
                                                    }
                                                    span { class: "min-w-0 flex-1 truncate font-medium", "{branch.name}" }
                                                    if branch.current {
                                                        LineIconView { icon: LineIcon::Check, class: "size-3.5 shrink-0" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if selected_reference_count == 0 {
                                if selected_branches_progress.loading && !selected_branches_progress.initialized {
                                    div { class: "flex min-h-28 flex-col justify-center gap-2 px-2.5 py-2",
                                        for width in ["w-9/12", "w-11/12", "w-8/12"] {
                                            vmux_ui::components::skeleton::Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                                        }
                                    }
                                } else {
                                    div { class: "flex min-h-28 items-center justify-center px-3 text-xs text-muted-foreground", {translate("common-no-results")} }
                                }
                            }
                        }
                    }
                }
                if focused_panel == GitPanel::Commits && selected_commit_entry.is_some() {
                    div { class: "flex min-w-0 items-center",
                        BreadcrumbSeparator {}
                        Dropdown {
                            open: commit_open,
                            class: "min-w-0".to_string(),
                            on_dismiss: move |_| {
                                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                            },
                            DropdownTrigger {
                                label: revision_label.clone(),
                                title: "{revision_title}",
                                open: commit_open,
                                onclick: move |_| {
                                    let open = if commit_open {
                                        HeaderDropdown::Closed
                                    } else {
                                        HeaderDropdown::Commit
                                    };
                                    send(&HeaderDropdownRequest { open });
                                },
                                leading: rsx! {
                                    LineIconView { icon: LineIcon::GitCommit, class: "size-3.5 shrink-0" }
                                },
                            }
                            if commit_open {
                                DropdownPopup {
                                    class: "min-w-80 overflow-hidden p-1.5",
                                    div {
                                        class: "max-h-80 overflow-y-auto",
                                        onscroll: move |event: Event<ScrollData>| {
                                            if ScrollPreload::LIST.reached(&event) && commits_progress.has_more && !commits_progress.loading {
                                                send(&GitCatalogNextRequest {
                                                    catalog: GitCatalog::Commits,
                                                });
                                            }
                                        },
                                        for commit in repository.commits.iter() {
                                            {
                                                let sha = commit.sha.clone();
                                                rsx! {
                                                    button {
                                                        key: "header-commit-{sha}",
                                                        r#type: "button",
                                                        class: if selected_commit == commit.sha {
                                                            "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
                                                        } else {
                                                            "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
                                                        },
                                                        title: "{commit.summary}",
                                                        onclick: move |_| {
                                                            send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                                                            send(&GitPanelSelectRequest { panel: GitPanel::Commits });
                                                            send(&GitCommitSelectRequest { commit: sha.clone() });
                                                        },
                                                        code { class: "shrink-0 font-mono text-[10px] text-git-reference", "{commit.short_sha}" }
                                                        span { class: "min-w-0 flex-1 truncate font-medium", "{commit.summary}" }
                                                        span { class: "shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
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
}

#[component]
fn RepositoryActions(
    focused_panel: GitPanel,
    file_query: Signal<String>,
    viewed_count: usize,
    viewed_total: u32,
    checkout_required: bool,
    checkout_branch: String,
    checkout_path: String,
    checkout_start: String,
    push_path: String,
    can_push: bool,
) -> Element {
    let mut query = file_query;
    rsx! {
        if focused_panel == GitPanel::Branches {
            ManagerSearchAction {
                value: query(),
                placeholder: translate("common-filter"),
                label: translate("git-files"),
                onsearch: move |value| query.set(value),
            }
        }
        if focused_panel == GitPanel::Branches && viewed_total > 0 {
            span { class: "mr-1 shrink-0 text-[11px] tabular-nums text-muted-foreground",
                "{viewed_count} / {viewed_total} "
                {translate("git-viewed")}
            }
        }
        if checkout_required {
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Xs,
                class: "h-8 gap-1.5 px-2.5 text-[11px] font-medium".to_string(),
                onclick: move |_| {
                    if checkout_start.is_empty() {
                        send(&ProjectActivateRequest {
                            path: push_path.clone(),
                            branch: checkout_branch.clone(),
                            checkout: checkout_path.clone(),
                            pane_id: None,
                        });
                    } else {
                        send(&GitCreateBranchRequest {
                            repo_root: push_path.clone(),
                            branch: checkout_branch.clone(),
                            start_point: checkout_start.clone(),
                        });
                    }
                },
                LineIconView { icon: LineIcon::GitBranch, class: "size-4" }
                span { {translate("git-checkout")} }
            }
        } else {
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Xs,
                class: "h-8 gap-1.5 px-2.5 text-[11px] font-medium".to_string(),
                disabled: !can_push,
                onclick: move |_| {
                    send(&GitPushRequest { path: push_path.clone() });
                },
                LineIconView { icon: LineIcon::Upload, class: "size-4" }
                span { {translate("git-push-label")} }
            }
        }
    }
}

#[component]
fn ViewPicker(
    open_dropdown: HeaderDropdown,
    focused_panel: GitPanel,
    changes: usize,
    commits: usize,
    stashes: usize,
    first_commit: String,
    commit_selected: bool,
) -> Element {
    let open = open_dropdown == HeaderDropdown::View;
    let (icon, label, count) = match focused_panel {
        GitPanel::Files => (LineIcon::File, translate("git-changes"), changes),
        GitPanel::Branches => (LineIcon::GitBranch, translate("git-branch-changes"), 0),
        GitPanel::Commits => (LineIcon::GitCommit, translate("git-commits"), commits),
        GitPanel::Stash => (LineIcon::Package, translate("git-stashes"), stashes),
    };
    rsx! {
        Dropdown {
            open,
            class: "shrink-0".to_string(),
            on_dismiss: move |_| {
                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
            },
            div { class: "flex items-center rounded-md hover:bg-foreground/[0.055]",
                DropdownTrigger {
                    label,
                    open,
                    onclick: move |_| {
                        let open = if open {
                            HeaderDropdown::Closed
                        } else {
                            HeaderDropdown::View
                        };
                        send(&HeaderDropdownRequest { open });
                    },
                    leading: rsx! {
                        LineIconView { icon, class: "size-3.5 shrink-0" }
                    },
                    trailing: rsx! {
                        if count > 0 {
                            span { class: "text-[10px] tabular-nums", "{count}" }
                        }
                    },
                }
                if focused_panel == GitPanel::Commits && commit_selected {
                    button {
                        r#type: "button",
                        class: "mr-1 flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/[0.07] hover:text-foreground",
                        title: translate("git-branch-changes"),
                        aria_label: translate("git-branch-changes"),
                        onclick: move |_| {
                            send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                            send(&GitBranchDiffSelectRequest);
                        },
                        LineIconView { icon: LineIcon::X, class: "size-3" }
                    }
                }
            }
            if open {
                DropdownPopup { class: "min-w-48 p-1.5",
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Files,
                        icon: LineIcon::File,
                        label: translate("git-changes"),
                        count: changes,
                        onselect: move |_| {
                            send(&GitPanelSelectRequest { panel: GitPanel::Files });
                        },
                    }
                    div { class: "mx-1 my-1 h-px bg-foreground/[0.07]" }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Branches,
                        icon: LineIcon::GitBranch,
                        label: translate("git-branch-changes"),
                        count: 0,
                        onselect: move |_| {
                            send(&GitBranchDiffSelectRequest);
                        },
                    }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Commits,
                        icon: LineIcon::GitCommit,
                        label: translate("git-commits"),
                        count: commits,
                        onselect: move |_| {
                            if first_commit.is_empty() {
                                send(&GitPanelSelectRequest { panel: GitPanel::Commits });
                            } else {
                                send(&GitCommitSelectRequest {
                                    commit: first_commit.clone(),
                                });
                            }
                        },
                    }
                    ViewPickerItem {
                        active: focused_panel == GitPanel::Stash,
                        icon: LineIcon::Package,
                        label: translate("git-stashes"),
                        count: stashes,
                        onselect: move |_| {
                            send(&GitPanelSelectRequest { panel: GitPanel::Stash });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn ViewPickerItem(
    active: bool,
    icon: LineIcon,
    label: String,
    count: usize,
    onselect: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active {
                "flex w-full items-center gap-2 rounded-md bg-foreground/[0.075] px-2 py-2 text-left text-xs text-foreground"
            } else {
                "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-xs text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
            },
            onclick: move |_| {
                send(&HeaderDropdownRequest { open: HeaderDropdown::Closed });
                onselect.call(());
            },
            LineIconView { icon, class: "size-3.5 shrink-0" }
            span { class: "min-w-0 flex-1 truncate font-medium", "{label}" }
            if count > 0 {
                span { class: "shrink-0 text-[9px] tabular-nums text-muted-foreground", "{count}" }
            }
        }
    }
}
