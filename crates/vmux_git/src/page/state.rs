use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::FileDirEntry;
use vmux_core::event::{PageContextEvent, TabWorkspaceEvent};
use vmux_ui::hooks::use_listener;
use vmux_ui::list_nav::{MenuDirection, move_selection};
use vmux_ui::scroll::ScrollIntoView;

use super::model::{BranchCollection, BranchPrompt, GitCommandLogEntry, GitPanel};
use super::workspace::GitWorkspace;
use crate::event::*;
use crate::view::EditorDiffMarker;

#[derive(Clone, Copy)]
pub(super) struct GitPageState {
    pub(super) workspace: Signal<String>,
    pub(super) repository: Signal<Option<GitRepositoryEvent>>,
    pub(super) directory: Signal<Option<GitDirectoryEvent>>,
    pub(super) directory_selected: Signal<usize>,
    pub(super) directory_children: Signal<Option<Vec<FileDirEntry>>>,
    pub(super) directory_preview_path: Signal<String>,
    pub(super) directory_came_from: Signal<String>,
    pub(super) directory_show_hidden: Signal<bool>,
    pub(super) selected_path: Signal<String>,
    pub(super) selected_path_bytes: Signal<Vec<u8>>,
    pub(super) selected_abs_path: Signal<String>,
    pub(super) selected_commit: Signal<String>,
    pub(super) selected_branch: Signal<String>,
    pub(super) branch_collection: Signal<BranchCollection>,
    pub(super) branch_prompt: Signal<Option<BranchPrompt>>,
    pub(super) branch_draft: Signal<String>,
    pub(super) pending_branch_checkout: Signal<String>,
    pub(super) selected_stash: Signal<String>,
    pub(super) confirm_discard: Signal<Vec<u8>>,
    pub(super) commit_message: Signal<String>,
    pub(super) pending_commit_message: Signal<String>,
    pub(super) fetching: Signal<bool>,
    pub(super) loading: Signal<bool>,
    pub(super) message: Signal<String>,
    pub(super) focused_panel: Signal<GitPanel>,
    pub(super) command_log: Signal<Vec<GitCommandLogEntry>>,
    pub(super) branch_log: Signal<Option<GitBranchLogEvent>>,
    pub(super) shortcut_help: Signal<bool>,
    pub(super) nonce: Signal<u32>,
    pub(super) markers: Signal<HashMap<u32, EditorDiffMarker>>,
    pub(super) diff_viewport: Signal<Option<GitDiffViewportEvent>>,
}

impl GitPageState {
    pub(super) fn use_state() -> Self {
        let state = Self {
            workspace: use_signal(String::new),
            repository: use_signal(|| None),
            directory: use_signal(|| None),
            directory_selected: use_signal(|| 0),
            directory_children: use_signal(|| None),
            directory_preview_path: use_signal(String::new),
            directory_came_from: use_signal(String::new),
            directory_show_hidden: use_signal(|| true),
            selected_path: use_signal(String::new),
            selected_path_bytes: use_signal(Vec::new),
            selected_abs_path: use_signal(String::new),
            selected_commit: use_signal(String::new),
            selected_branch: use_signal(String::new),
            branch_collection: use_signal(BranchCollection::default),
            branch_prompt: use_signal(|| None),
            branch_draft: use_signal(String::new),
            pending_branch_checkout: use_signal(String::new),
            selected_stash: use_signal(String::new),
            confirm_discard: use_signal(Vec::new),
            commit_message: use_signal(String::new),
            pending_commit_message: use_signal(String::new),
            fetching: use_signal(|| false),
            loading: use_signal(|| true),
            message: use_signal(String::new),
            focused_panel: use_signal(GitPanel::default),
            command_log: use_signal(Vec::new),
            branch_log: use_signal(|| None),
            shortcut_help: use_signal(|| false),
            nonce: use_signal(|| 0),
            markers: use_signal(HashMap::new),
            diff_viewport: use_signal(|| None),
        };
        state.subscribe();
        state
    }

    fn subscribe(self) {
        let Self {
            mut workspace,
            mut repository,
            mut directory,
            mut directory_selected,
            mut directory_children,
            mut directory_preview_path,
            mut directory_came_from,
            mut directory_show_hidden,
            mut selected_path,
            mut selected_path_bytes,
            mut selected_abs_path,
            mut selected_commit,
            mut selected_branch,
            mut branch_collection,
            mut branch_prompt,
            mut branch_draft,
            mut pending_branch_checkout,
            mut selected_stash,
            mut commit_message,
            mut pending_commit_message,
            mut fetching,
            mut loading,
            mut message,
            mut focused_panel,
            mut command_log,
            mut branch_log,
            mut diff_viewport,
            mut nonce,
            ..
        } = self;

        let _context = use_listener::<PageContextEvent, _>(move |context| {
            let path = crate::GitUrl::parse(&context.page_url)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or(context.working_directory);
            workspace.set(path.clone());
            repository.set(None);
            directory.set(None);
            directory_children.set(None);
            directory_selected.set(0);
            directory_preview_path.set(String::new());
            directory_came_from.set(String::new());
            directory_show_hidden.set(true);
            selected_path.set(String::new());
            selected_path_bytes.set(Vec::new());
            selected_abs_path.set(String::new());
            selected_commit.set(String::new());
            selected_branch.set(String::new());
            branch_collection.set(BranchCollection::Local);
            branch_prompt.set(None);
            branch_draft.set(String::new());
            pending_branch_checkout.set(String::new());
            selected_stash.set(String::new());
            fetching.set(false);
            loading.set(true);
            message.set(String::new());
            focused_panel.set(GitPanel::Status);
            command_log.set(Vec::new());
            branch_log.set(None);
            GitWorkspace::browse(&path, false);
        });
        let _repository = use_listener::<GitRepositoryEvent, _>(move |event| {
            if event.path != workspace() && event.repo_root != workspace() {
                return;
            }
            let next_file = event
                .files
                .iter()
                .find(|entry| entry.path_bytes == selected_path_bytes())
                .or_else(|| event.files.first())
                .cloned();
            let next_commit = event
                .commits
                .iter()
                .find(|entry| entry.sha == selected_commit())
                .or_else(|| event.commits.first())
                .map(|entry| entry.sha.clone())
                .unwrap_or_default();
            let next_branch = branch_collection().selected_reference(&event, &selected_branch());
            let next_stash = event
                .stashes
                .iter()
                .find(|entry| entry.reference == selected_stash())
                .or_else(|| event.stashes.first())
                .map(|entry| entry.reference.clone())
                .unwrap_or_default();
            selected_abs_path.set(
                next_file
                    .as_ref()
                    .map(|entry| GitWorkspace::absolute_path(&event.repo_root, &entry.path))
                    .unwrap_or_default(),
            );
            selected_path.set(
                next_file
                    .as_ref()
                    .map(|entry| entry.path.clone())
                    .unwrap_or_default(),
            );
            selected_path_bytes.set(next_file.map(|entry| entry.path_bytes).unwrap_or_default());
            selected_commit.set(next_commit);
            selected_branch.set(next_branch);
            selected_stash.set(next_stash);
            workspace.set(event.repo_root.clone());
            repository.set(Some(event));
            directory.set(None);
            loading.set(false);
            message.set(String::new());
        });
        let _directory = use_listener::<GitDirectoryEvent, _>(move |event| {
            if event.preview {
                if event.path == directory_preview_path() {
                    directory_children.set(Some(event.entries));
                }
                return;
            }
            workspace.set(event.path.clone());
            if !event.repo_root.is_empty() {
                workspace.set(event.repo_root.clone());
                loading.set(true);
                GitWorkspace::activate(&event.repo_root);
                GitWorkspace::request(&event.repo_root);
                return;
            }
            let came_from = directory_came_from();
            directory_came_from.set(String::new());
            let selected = event
                .entries
                .iter()
                .position(|entry| entry.path == came_from)
                .unwrap_or(0);
            let preview = event.entries.get(selected).filter(|entry| entry.is_dir);
            directory_selected.set(selected);
            directory_children.set(None);
            directory_preview_path.set(String::new());
            if let Some(entry) = preview {
                directory_preview_path.set(entry.path.clone());
                GitWorkspace::browse(&entry.path, true);
            }
            directory.set(Some(event));
            loading.set(false);
            message.set(String::new());
        });
        let _branch_log = use_listener::<GitBranchLogEvent, _>(move |event| {
            if event.repo_root == workspace() && event.branch == selected_branch() {
                branch_log.set(Some(event));
            }
        });
        let _diff_viewport = use_listener::<GitDiffViewportEvent, _>(move |event| {
            diff_viewport.set(Some(event));
        });
        let _repository_picked = use_listener::<GitRepositoryPickedEvent, _>(move |event| {
            if event.path.is_empty() {
                return;
            }
            loading.set(true);
            GitWorkspace::browse(&event.path, false);
        });
        let _result = use_listener::<GitResultEvent, _>(move |result| {
            GitCommandLogEntry::from_result(&result).append(&mut command_log.write());
            if result.action == "commit" {
                if result.ok && commit_message().trim() == pending_commit_message() {
                    commit_message.set(String::new());
                }
                pending_commit_message.set(String::new());
            }
            if result.action == "fetch" {
                fetching.set(false);
            }
            if result.action == "new branch" {
                let branch = pending_branch_checkout();
                pending_branch_checkout.set(String::new());
                if result.ok && !branch.is_empty() {
                    GitWorkspace::select_branch_name(&workspace(), &branch);
                }
            }
            if result.ok {
                message.set(String::new());
            } else {
                message.set(result.message);
            }
            nonce.set(nonce().wrapping_add(1));
            GitWorkspace::request(&workspace());
        });
        let _error = use_listener::<GitErrorEvent, _>(move |event| {
            GitCommandLogEntry::error(&event.message).append(&mut command_log.write());
            loading.set(false);
            fetching.set(false);
            message.set(event.message);
        });
        let _changed = use_listener::<GitChangedEvent, _>(move |_| {
            nonce.set(nonce().wrapping_add(1));
            GitWorkspace::request(&workspace());
        });
        let _workspace = use_listener::<TabWorkspaceEvent, _>(move |event| {
            if !event.error.is_empty() {
                GitCommandLogEntry::error(&event.error).append(&mut command_log.write());
                message.set(event.error);
                return;
            }
            if event.path.is_empty() || event.path == workspace() {
                return;
            }
            workspace.set(event.path.clone());
            repository.set(None);
            selected_path.set(String::new());
            selected_path_bytes.set(Vec::new());
            selected_abs_path.set(String::new());
            selected_commit.set(String::new());
            selected_branch.set(event.branch);
            selected_stash.set(String::new());
            fetching.set(false);
            branch_log.set(None);
            loading.set(true);
            GitWorkspace::request(&event.path);
        });
    }

    pub(super) fn move_selection(self, direction: MenuDirection) -> bool {
        let Some(repository) = (self.repository)() else {
            return false;
        };
        match (self.focused_panel)() {
            GitPanel::Status => false,
            GitPanel::Files => self.move_file_selection(&repository, direction),
            GitPanel::Branches => self.move_branch_selection(&repository, direction),
            GitPanel::Commits => self.move_commit_selection(&repository, direction),
            GitPanel::Stash => self.move_stash_selection(&repository, direction),
        }
    }

    fn move_file_selection(
        self,
        repository: &GitRepositoryEvent,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.files.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .files
            .iter()
            .position(|entry| entry.path_bytes == (self.selected_path_bytes)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let entry = &repository.files[index];
        let mut selected_path = self.selected_path;
        let mut selected_path_bytes = self.selected_path_bytes;
        let mut selected_abs_path = self.selected_abs_path;
        selected_path.set(entry.path.clone());
        selected_path_bytes.set(entry.path_bytes.clone());
        selected_abs_path.set(GitWorkspace::absolute_path(
            &repository.repo_root,
            &entry.path,
        ));
        let section = if entry.staged { "staged" } else { "unstaged" };
        ScrollIntoView::nearest(&format!("git-file-{section}-row-{index}"));
        true
    }

    fn move_branch_selection(
        self,
        repository: &GitRepositoryEvent,
        direction: MenuDirection,
    ) -> bool {
        let references = (self.branch_collection)().references(repository);
        let len = references.len();
        if len == 0 {
            return false;
        }
        let current = references
            .iter()
            .position(|reference| reference == &(self.selected_branch)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_branch = self.selected_branch;
        selected_branch.set(references[index].clone());
        ScrollIntoView::nearest(&format!("git-branch-row-{index}"));
        true
    }

    fn move_commit_selection(
        self,
        repository: &GitRepositoryEvent,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.commits.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .commits
            .iter()
            .position(|entry| entry.sha == (self.selected_commit)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_commit = self.selected_commit;
        selected_commit.set(repository.commits[index].sha.clone());
        ScrollIntoView::nearest(&format!("git-commit-row-{index}"));
        true
    }

    fn move_stash_selection(
        self,
        repository: &GitRepositoryEvent,
        direction: MenuDirection,
    ) -> bool {
        let len = repository.stashes.len();
        if len == 0 {
            return false;
        }
        let current = repository
            .stashes
            .iter()
            .position(|entry| entry.reference == (self.selected_stash)())
            .unwrap_or(match direction {
                MenuDirection::Next => len - 1,
                MenuDirection::Previous => 0,
            });
        let index = move_selection(current, len, direction);
        let mut selected_stash = self.selected_stash;
        selected_stash.set(repository.stashes[index].reference.clone());
        ScrollIntoView::nearest(&format!("git-stash-row-{index}"));
        true
    }

    pub(super) fn submit_branch_prompt(self, prompt: &BranchPrompt) -> bool {
        match prompt {
            BranchPrompt::Create { base } => {
                let branch = (self.branch_draft)().trim().to_string();
                if branch.is_empty() {
                    return false;
                }
                let mut pending_checkout = self.pending_branch_checkout;
                pending_checkout.set(branch.clone());
                GitWorkspace::operate(
                    &(self.workspace)(),
                    GitOperation::CreateBranch {
                        branch,
                        start_point: base.clone(),
                    },
                );
            }
            BranchPrompt::Delete { branch } => {
                GitWorkspace::operate(
                    &(self.workspace)(),
                    GitOperation::DeleteBranch {
                        branch: branch.clone(),
                    },
                );
            }
        }
        true
    }
}
