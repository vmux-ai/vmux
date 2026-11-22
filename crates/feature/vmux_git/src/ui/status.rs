#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::LineIcon;

use crate::event::*;
use crate::state::{GitBranchCheckout, GitBranchCollection, GitCatalogProgress, GitPanel};

#[component]
pub(super) fn RepositoryHeader(
    repository: GitRepositorySnapshot,
    focused_panel: GitPanel,
    selected_branch: String,
    selected_commit: String,
    can_push: bool,
    branch_checkout: Option<GitBranchCheckout>,
    commits_progress: GitCatalogProgress,
    viewed_count: usize,
    viewed_total: u32,
) -> Element {
    let address = HeaderAddress {
        repository: &repository,
        focused_panel,
        selected_branch: &selected_branch,
        selected_commit: &selected_commit,
        commits_progress,
    }
    .build();
    let toolbar = HeaderToolbar {
        repo_root: &repository.repo_root,
        focused_panel,
        can_push,
        branch_checkout,
        viewed_count,
        viewed_total,
    }
    .build();
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
                AddressIntent::GitCommit { commit } => {
                    let _ = send(&GitCommitSelectRequest { commit });
                }
                AddressIntent::GitCatalogNext { catalog } => {
                    let _ = send(&GitCatalogNextRequest { catalog });
                }
                AddressIntent::Section { id } => {
                    HeaderAddress::send_section(&id, &picker_root, &first_commit);
                }
                _ => {}
            },
        }
        PageToolbar { controls: toolbar }
    }
}

struct HeaderAddress<'a> {
    repository: &'a GitRepositorySnapshot,
    focused_panel: GitPanel,
    selected_branch: &'a str,
    selected_commit: &'a str,
    commits_progress: GitCatalogProgress,
}

impl HeaderAddress<'_> {
    fn build(&self) -> PageAddress {
        let git_url = crate::GitUrl::from_path(std::path::Path::new(&self.repository.repo_root))
            .unwrap_or_else(|| crate::GIT_PAGE_URL.to_string());
        let current_page = AddressOption {
            label_key: "git-title".into(),
            icon: Some(vmux_api::BuiltinIcon::GitBranch),
            intent: Some(AddressIntent::Navigate { url: git_url }),
            ..Default::default()
        };
        let mut address = PageAddress {
            current: current_page.clone(),
            contextual: vec![
                current_page,
                AddressOption {
                    label_key: "editor-editor".into(),
                    icon: Some(vmux_api::BuiltinIcon::Code),
                    intent: Some(AddressIntent::Navigate {
                        url: "file://".to_string(),
                    }),
                    ..Default::default()
                },
            ],
            fixed: vec![self.panel_crumb()],
            ..Default::default()
        };
        if let Some(crumb) = self.commit_crumb() {
            address.fixed.push(crumb);
        }
        address
    }

    fn panel_crumb(&self) -> AddressCrumb {
        let panel_label = match self.focused_panel {
            GitPanel::Files => "git-changes",
            GitPanel::Branches => "git-branch-changes",
            GitPanel::Commits => "git-commits",
            GitPanel::Stash => "git-stashes",
        };
        let selection = match self.focused_panel {
            GitPanel::Branches => Some(self.selected_branch),
            GitPanel::Commits => Some(self.selected_commit),
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
                GitPanel::Branches => Some(if self.selected_branch.is_empty() {
                    self.repository.branch.as_str()
                } else {
                    self.selected_branch
                }),
                GitPanel::Commits if !self.selected_commit.is_empty() => Some(self.selected_commit),
                _ => None,
            };
            if let Some(url) = crate::GitUrl::from_view(
                std::path::Path::new(&self.repository.repo_root),
                panel,
                choice,
            ) {
                views.push(AddressOption {
                    label_key: label.into(),
                    icon: Some(panel.icon()),
                    intent: Some(AddressIntent::Navigate { url }),
                    ..Default::default()
                });
            }
        }
        AddressCrumb {
            entry: AddressOption {
                label_key: panel_label.into(),
                icon: Some(self.focused_panel.icon()),
                intent: crate::GitUrl::from_view(
                    std::path::Path::new(&self.repository.repo_root),
                    self.focused_panel,
                    selection,
                )
                .map(|url| AddressIntent::Navigate { url }),
                ..Default::default()
            },
            options: views,
            ..Default::default()
        }
    }

    fn commit_crumb(&self) -> Option<AddressCrumb> {
        if self.focused_panel != GitPanel::Commits {
            return None;
        }
        let mut commits = Vec::new();
        let mut selected = None;
        for commit in &self.repository.commits {
            let entry = AddressOption {
                label: commit.short_sha.clone(),
                title: commit.summary.clone(),
                identity: commit.sha.clone(),
                intent: crate::GitUrl::from_view(
                    std::path::Path::new(&self.repository.repo_root),
                    GitPanel::Commits,
                    Some(&commit.sha),
                )
                .map(|url| AddressIntent::Navigate { url }),
                ..Default::default()
            };
            if commit.sha == self.selected_commit {
                selected = Some(entry.clone());
            }
            commits.push(entry);
        }
        if selected.is_none() && !self.selected_commit.is_empty() {
            let entry = AddressOption {
                label: self.selected_commit.chars().take(7).collect(),
                title: self.selected_commit.to_string(),
                identity: self.selected_commit.to_string(),
                intent: crate::GitUrl::from_view(
                    std::path::Path::new(&self.repository.repo_root),
                    GitPanel::Commits,
                    Some(self.selected_commit),
                )
                .map(|url| AddressIntent::Navigate { url }),
                ..Default::default()
            };
            commits.insert(0, entry.clone());
            selected = Some(entry);
        }
        let entry = selected?;
        Some(AddressCrumb {
            entry,
            options: commits,
            active: true,
            pending: self.commits_progress.loading,
            on_more: self
                .commits_progress
                .has_more
                .then_some(AddressIntent::GitCatalogNext {
                    catalog: GitCatalog::Commits,
                }),
            ..Default::default()
        })
    }

    fn send_section(id: &str, picker_root: &str, first_commit: &str) {
        match id {
            "changes" => {
                let _ = send(&GitPanelSelectRequest {
                    panel: GitPanel::Files,
                });
            }
            "branches" => {
                let _ = send(&GitBranchDiffSelectRequest);
            }
            "commits" if !first_commit.is_empty() => {
                let _ = send(&GitCommitSelectRequest {
                    commit: first_commit.to_string(),
                });
            }
            "commits" => {
                let _ = send(&GitPanelSelectRequest {
                    panel: GitPanel::Commits,
                });
            }
            "stash" => {
                let _ = send(&GitPanelSelectRequest {
                    panel: GitPanel::Stash,
                });
            }
            "browse-project" => {
                let _ = send(&GitRepositoryPickerRequest {
                    path: picker_root.to_string(),
                });
            }
            "local" | "remote" | "tags" => {
                let collection = match id {
                    "remote" => GitBranchCollection::Remote,
                    "tags" => GitBranchCollection::Tags,
                    _ => GitBranchCollection::Local,
                };
                let _ = send(&GitBranchCollectionSelectRequest { collection });
            }
            _ => {}
        }
    }
}

struct HeaderToolbar<'a> {
    repo_root: &'a str,
    focused_panel: GitPanel,
    can_push: bool,
    branch_checkout: Option<GitBranchCheckout>,
    viewed_count: usize,
    viewed_total: u32,
}

impl HeaderToolbar<'_> {
    fn build(self) -> Vec<ToolbarControl> {
        let mut toolbar = Vec::new();
        if self.focused_panel == GitPanel::Branches && self.viewed_total > 0 {
            toolbar.push(ToolbarControl::badge(format!(
                "{} / {} {}",
                self.viewed_count,
                self.viewed_total,
                translate("git-viewed")
            )));
        }
        let root = self.repo_root.to_string();
        if self.focused_panel == GitPanel::Branches
            && let Some(checkout) = self.branch_checkout
        {
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
                .disabled(!self.can_push),
            );
        }
        toolbar
    }
}
