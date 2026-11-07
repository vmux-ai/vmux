#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::space::ProjectRow;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::LineIcon;

use crate::event::*;
use crate::state::{GitBranchCollection, GitCatalogProgress, GitPanel};

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
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
) -> Element {
    let git_url = crate::GitUrl::from_path(std::path::Path::new(&repository.repo_root))
        .unwrap_or_else(|| crate::GIT_PAGE_URL.to_string());
    let current_page = AddressOption {
        label_key: "git-title".into(),
        icon: Some(vmux_api::BuiltinIcon::GitBranch),
        intent: Some(AddressIntent::Navigate {
            url: git_url.clone(),
        }),
        ..Default::default()
    };
    let editor_url = vmux_path::FileUrl::from_path(
        std::path::Path::new(&repository.repo_root),
        None,
        None,
        None,
    );
    let (_panel, panel_label, _panel_icon) = match focused_panel {
        GitPanel::Files => ("changes", "git-changes", vmux_api::BuiltinIcon::Files),
        GitPanel::Branches => (
            "branches",
            "git-branch-changes",
            vmux_api::BuiltinIcon::GitBranch,
        ),
        GitPanel::Commits => ("commits", "git-commits", vmux_api::BuiltinIcon::Clock),
        GitPanel::Stash => ("stash", "git-stashes", vmux_api::BuiltinIcon::Archive),
    };
    let mut project_options = Vec::new();
    for project in &projects {
        if project.depth != 0 || project.missing {
            continue;
        }
        let Some(url) = crate::GitUrl::from_path(std::path::Path::new(&project.path)) else {
            continue;
        };
        project_options.push(AddressOption {
            label: project.label.clone(),
            title: project.path.clone(),
            identity: project.path.clone(),
            intent: Some(AddressIntent::Navigate { url }),
            ..Default::default()
        });
    }
    if !project_options
        .iter()
        .any(|option| option.identity == repository.repo_root)
    {
        project_options.insert(
            0,
            AddressOption {
                label: repository.repo_name.clone(),
                title: repository.repo_root.clone(),
                identity: repository.repo_root.clone(),
                intent: Some(AddressIntent::Navigate {
                    url: git_url.clone(),
                }),
                ..Default::default()
            },
        );
    }
    project_options.push(AddressOption {
        label_key: "agent-project-choose".into(),
        intent: Some(AddressIntent::Section {
            id: "browse-project".into(),
        }),
        ..Default::default()
    });
    let branch_label = if focused_panel == GitPanel::Branches {
        selected_branch.clone()
    } else {
        repository.branch.clone()
    };
    let comparison = if focused_panel == GitPanel::Branches
        && !selected_branch_base.is_empty()
        && selected_branch_base != branch_label
    {
        format!("{selected_branch_base} ← {branch_label}")
    } else {
        branch_label.clone()
    };
    let mut branch_options = Vec::new();
    for reference in branch_collection.references(&repository) {
        branch_options.push(AddressOption {
            label: reference.clone(),
            identity: reference.clone(),
            intent: crate::GitUrl::from_view(
                std::path::Path::new(&repository.repo_root),
                GitPanel::Branches,
                Some(&reference),
            )
            .map(|url| AddressIntent::Navigate { url }),
            ..Default::default()
        });
    }
    let mut branch_tabs = Vec::new();
    for (id, label) in [
        ("local", "git-local"),
        ("remote", "git-remote"),
        ("tags", "git-tags"),
    ] {
        branch_tabs.push(AddressOption {
            label_key: label.into(),
            intent: Some(AddressIntent::Section { id: id.into() }),
            ..Default::default()
        });
    }
    let progress = if branch_collection == GitBranchCollection::Local {
        branches_progress
    } else {
        selected_branches_progress
    };
    let catalog = GitCatalog::from(branch_collection);
    let mut address = PageAddress {
        current: current_page.clone(),
        contextual: vec![
            current_page,
            AddressOption {
                label_key: "editor-editor".into(),
                icon: Some(vmux_api::BuiltinIcon::Code),
                intent: Some(AddressIntent::Navigate { url: editor_url }),
                ..Default::default()
            },
        ],
        fixed: vec![
            AddressCrumb {
                entry: AddressOption {
                    label: repository.repo_name.clone(),
                    title: repository.repo_root.clone(),
                    identity: repository.repo_root.clone(),
                    intent: Some(AddressIntent::Navigate { url: git_url }),
                    ..Default::default()
                },
                options: project_options,
                ..Default::default()
            },
            AddressCrumb {
                entry: AddressOption {
                    label: comparison.clone(),
                    title: comparison,
                    identity: branch_label.clone(),
                    intent: crate::GitUrl::from_view(
                        std::path::Path::new(&repository.repo_root),
                        GitPanel::Branches,
                        Some(&branch_label),
                    )
                    .map(|url| AddressIntent::Navigate { url }),
                    ..Default::default()
                },
                options: branch_options,
                tabs: branch_tabs,
                selected_tab: Some(AddressIntent::Section {
                    id: match branch_collection {
                        GitBranchCollection::Local => "local",
                        GitBranchCollection::Remote => "remote",
                        GitBranchCollection::Tags => "tags",
                    }
                    .to_string(),
                }),
                pending: progress.loading,
                on_open: (!progress.initialized)
                    .then_some(AddressIntent::GitCatalogNext { catalog }),
                on_more: progress
                    .has_more
                    .then_some(AddressIntent::GitCatalogNext { catalog }),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    if focused_panel == GitPanel::Commits {
        let mut commits = Vec::new();
        let mut selected = None;
        for commit in &repository.commits {
            let entry = AddressOption {
                label: commit.short_sha.clone(),
                title: commit.summary.clone(),
                intent: crate::GitUrl::from_view(
                    std::path::Path::new(&repository.repo_root),
                    GitPanel::Commits,
                    Some(&commit.sha),
                )
                .map(|url| AddressIntent::Navigate { url }),
                ..Default::default()
            };
            if commit.sha == selected_commit {
                selected = Some(entry.clone());
            }
            commits.push(entry);
        }
        if let Some(entry) = selected {
            address.fixed.push(AddressCrumb {
                entry,
                options: commits,
                active: true,
                pending: commits_progress.loading,
                on_more: commits_progress
                    .has_more
                    .then_some(AddressIntent::GitCatalogNext {
                        catalog: GitCatalog::Commits,
                    }),
                ..Default::default()
            });
        }
    }
    let selection = match focused_panel {
        GitPanel::Branches => Some(selected_branch.as_str()),
        GitPanel::Commits => Some(selected_commit.as_str()),
        GitPanel::Stash | GitPanel::Files => None,
    };
    let mut views = Vec::new();
    for (panel, label) in [
        (GitPanel::Files, "git-changes"),
        (GitPanel::Branches, "git-branch-changes"),
        (GitPanel::Commits, "git-commits"),
        (GitPanel::Stash, "git-stashes"),
    ] {
        let choice = match panel {
            GitPanel::Branches => Some(if selected_branch.is_empty() {
                repository.branch.as_str()
            } else {
                selected_branch.as_str()
            }),
            GitPanel::Commits if !selected_commit.is_empty() => Some(selected_commit.as_str()),
            _ => None,
        };
        if let Some(url) =
            crate::GitUrl::from_view(std::path::Path::new(&repository.repo_root), panel, choice)
        {
            views.push(AddressOption {
                label_key: label.into(),
                intent: Some(AddressIntent::Navigate { url }),
                ..Default::default()
            });
        }
    }
    address.fixed.insert(
        0,
        AddressCrumb {
            entry: AddressOption {
                label_key: panel_label.into(),
                intent: crate::GitUrl::from_view(
                    std::path::Path::new(&repository.repo_root),
                    focused_panel,
                    selection,
                )
                .map(|url| AddressIntent::Navigate { url }),
                ..Default::default()
            },
            options: views,
            ..Default::default()
        },
    );
    let mut toolbar = Vec::new();
    if focused_panel == GitPanel::Branches && viewed_total > 0 {
        toolbar.push(ToolbarControl::badge(format!(
            "{viewed_count} / {viewed_total} {}",
            translate("git-viewed")
        )));
    }
    let checkout = if focused_panel == GitPanel::Branches {
        selected_branch_collection.checkout(&repository, &selected_branch)
    } else {
        None
    };
    let root = repository.repo_root.clone();
    if let Some(checkout) = checkout.filter(|checkout| !checkout.current) {
        toolbar.push(
            ToolbarControl::button(translate("git-checkout"), move |_| {
                if checkout.start_point.is_empty() {
                    let _ = send(&ProjectActivateRequest {
                        path: root.clone(),
                        branch: checkout.branch.clone(),
                        checkout: checkout.checkout.clone(),
                        pane_id: None,
                    });
                } else {
                    let _ = send(&GitCreateBranchRequest {
                        repo_root: root.clone(),
                        branch: checkout.branch.clone(),
                        start_point: checkout.start_point.clone(),
                    });
                }
            })
            .icon(LineIcon::GitBranch)
            .primary(),
        );
    } else {
        toolbar.push(
            ToolbarControl::button(translate("git-push-label"), move |_| {
                let _ = send(&GitPushRequest { path: root.clone() });
            })
            .icon(LineIcon::Upload)
            .primary()
            .disabled(!can_push),
        );
    }
    let picker_root = repository.repo_root.clone();
    let first_commit = repository
        .commits
        .first()
        .map(|commit| commit.sha.clone())
        .unwrap_or_default();
    rsx! {
        PageBreadcrumbs {
            state: address,
            onintent: move |intent| match intent {
                AddressIntent::GitBranch { reference } => {
                    let _ = send(&GitBranchSelectRequest { reference });
                }
                AddressIntent::GitCommit { commit } => { let _ = send(&GitCommitSelectRequest { commit }); }
                AddressIntent::GitCatalogNext { catalog } => { let _ = send(&GitCatalogNextRequest { catalog }); }
                AddressIntent::Section { id } => {
                    match id.as_str() {
                        "changes" => { let _ = send(&GitPanelSelectRequest { panel: GitPanel::Files }); }
                        "branches" => { let _ = send(&GitBranchDiffSelectRequest); }
                        "commits" if !first_commit.is_empty() => { let _ = send(&GitCommitSelectRequest { commit: first_commit.clone() }); }
                        "commits" => { let _ = send(&GitPanelSelectRequest { panel: GitPanel::Commits }); }
                        "stash" => { let _ = send(&GitPanelSelectRequest { panel: GitPanel::Stash }); }
                        "browse-project" => { let _ = send(&GitRepositoryPickerRequest { path: picker_root.clone() }); }
                        "local" | "remote" | "tags" => {
                            let collection = match id.as_str() {
                                "remote" => GitBranchCollection::Remote,
                                "tags" => GitBranchCollection::Tags,
                                _ => GitBranchCollection::Local,
                            };
                            let _ = send(&GitBranchCollectionSelectRequest { collection });
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
        }
        PageToolbar { controls: toolbar }
    }
}
