#![allow(non_snake_case)]

use std::collections::HashMap;
use std::path::Path;

use dioxus::prelude::*;
use vmux_core::event::FileDirEntry;
use vmux_core::event::space::ProjectCommandEvent;
use vmux_core::event::{
    PAGE_CONTEXT_EVENT, PageContextEvent, PageContextRequest, TAB_WORKSPACE_EVENT,
    TabWorkspaceEvent, TabWorkspaceRequest,
};
use vmux_ui::components::badge::Badge;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::directory::{DirectoryNavigator, DirectoryNavigatorAction, visible_directory_entries};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{GitIconView, LineIcon, LineIconView};
use vmux_ui::list_nav::{MenuDirection, move_selection};
use vmux_ui::scroll::ScrollIntoView;

use crate::event::*;
use crate::ui::DiffView;
use crate::view::EditorDiffMarker;

pub static NATIVE_PAGE: vmux_native::NativePage =
    vmux_native::NativePage::pane(crate::GIT_PAGE_URL, Page)
        .served_from(crate::GIT_DOCUMENT_URL)
        .titled("Git")
        .owning_subtree();

pub static LEGACY_NATIVE_PAGE: vmux_native::NativePage =
    vmux_native::NativePage::pane(crate::GIT_DOCUMENT_URL, Page).titled("Git");

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut workspace = use_signal(String::new);
    let mut repository = use_signal(|| Option::<GitRepositoryEvent>::None);
    let mut directory = use_signal(|| Option::<GitDirectoryEvent>::None);
    let mut directory_selected = use_signal(|| 0usize);
    let mut directory_children = use_signal(|| Option::<Vec<FileDirEntry>>::None);
    let mut directory_preview_path = use_signal(String::new);
    let mut directory_came_from = use_signal(String::new);
    let mut directory_show_hidden = use_signal(|| true);
    let mut selected_path = use_signal(String::new);
    let mut selected_path_bytes = use_signal(Vec::<u8>::new);
    let mut selected_abs_path = use_signal(String::new);
    let mut selected_commit = use_signal(String::new);
    let mut selected_branch = use_signal(String::new);
    let mut branch_collection = use_signal(BranchCollection::default);
    let mut branch_prompt = use_signal(|| Option::<BranchPrompt>::None);
    let mut branch_draft = use_signal(String::new);
    let mut pending_branch_checkout = use_signal(String::new);
    let mut selected_stash = use_signal(String::new);
    let mut confirm_discard = use_signal(Vec::<u8>::new);
    let mut commit_message = use_signal(String::new);
    let mut pending_commit_message = use_signal(String::new);
    let mut fetching = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut message = use_signal(String::new);
    let mut focused_panel = use_signal(GitPanel::default);
    let mut command_log = use_signal(Vec::<GitCommandLogEntry>::new);
    let mut branch_log = use_signal(|| Option::<GitBranchLogEvent>::None);
    let mut shortcut_help = use_signal(|| false);
    let mut nonce = use_signal(|| 0u32);
    let markers = use_signal(HashMap::<u32, EditorDiffMarker>::new);

    let _context = use_listener::<PageContextEvent, _>(PAGE_CONTEXT_EVENT, move |context| {
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
    let _repository = use_listener::<GitRepositoryEvent, _>(GIT_REPOSITORY_EVENT, move |event| {
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
    let _directory = use_listener::<GitDirectoryEvent, _>(GIT_DIRECTORY_EVENT, move |event| {
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
    let _branch_log = use_listener::<GitBranchLogEvent, _>(GIT_BRANCH_LOG_EVENT, move |event| {
        if event.repo_root == workspace() && event.branch == selected_branch() {
            branch_log.set(Some(event));
        }
    });
    let _repository_picked =
        use_listener::<GitRepositoryPickedEvent, _>(GIT_REPOSITORY_PICKED_EVENT, move |event| {
            if event.path.is_empty() {
                return;
            }
            loading.set(true);
            GitWorkspace::browse(&event.path, false);
        });
    let _result = use_listener::<GitResultEvent, _>(GIT_RESULT_EVENT, move |result| {
        {
            let mut entries = command_log.write();
            GitCommandLogEntry::from_result(&result).append(&mut entries);
        }
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
    let _error = use_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |event| {
        {
            let mut entries = command_log.write();
            GitCommandLogEntry::error(&event.message).append(&mut entries);
        }
        loading.set(false);
        fetching.set(false);
        message.set(event.message);
    });
    let _changed = use_listener::<GitChangedEvent, _>(GIT_CHANGED_EVENT, move |_| {
        nonce.set(nonce().wrapping_add(1));
        GitWorkspace::request(&workspace());
    });
    let _workspace = use_listener::<TabWorkspaceEvent, _>(TAB_WORKSPACE_EVENT, move |event| {
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

    use_effect(move || {
        let _ = send(&PageContextRequest {});
    });
    use_effect(move || {
        if focused_panel() != GitPanel::Branches {
            return;
        }
        let repo_root = workspace();
        let branch = selected_branch();
        if repo_root.is_empty() || branch.is_empty() {
            return;
        }
        if branch_log().is_some_and(|event| event.repo_root == repo_root && event.branch == branch)
        {
            return;
        }
        let _ = send(&GitBranchLogRequest { repo_root, branch });
    });

    rsx! {
        document::Title { {translate("git-title")} }
        div {
            class: "relative flex h-screen min-w-0 flex-col bg-background text-foreground outline-none",
            tabindex: "-1",
            autofocus: true,
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape && shortcut_help() {
                    event.prevent_default();
                    event.stop_propagation();
                    shortcut_help.set(false);
                    return;
                }
                if let Some(direction) = GitPanel::menu_direction(&event) {
                    let Some(repository) = repository() else {
                        return;
                    };
                    if focused_panel().move_selection(
                        &repository,
                        GitPanelSelection {
                            path: selected_path,
                            path_bytes: selected_path_bytes,
                            absolute_path: selected_abs_path,
                            branch: selected_branch,
                            branch_collection,
                            commit: selected_commit,
                            stash: selected_stash,
                        },
                        direction,
                    ) {
                        event.prevent_default();
                        event.stop_propagation();
                    }
                    return;
                }
                if event.is_auto_repeating() {
                    return;
                }
                let modifiers = event.modifiers();
                if modifiers.ctrl() || modifiers.alt() || modifiers.meta() {
                    return;
                }
                let key = event.key().to_string();
                if key == "?" {
                    event.prevent_default();
                    event.stop_propagation();
                    shortcut_help.set(!shortcut_help());
                    return;
                }
                if key == "Tab" {
                    event.prevent_default();
                    event.stop_propagation();
                    focused_panel.set(focused_panel().next(modifiers.shift()));
                    return;
                }
                if let Some(panel) = GitPanel::from_key(&key) {
                    event.prevent_default();
                    event.stop_propagation();
                    focused_panel.set(panel);
                    return;
                }
                let Some(repository) = repository() else {
                    return;
                };
                let handled = match (focused_panel(), key.as_str()) {
                    (GitPanel::Status, "e") => {
                        GitWorkspace::app_action(&repository.repo_root, GitAppAction::EditConfig);
                        true
                    }
                    (GitPanel::Status, "u") => {
                        GitWorkspace::app_action(
                            &repository.repo_root,
                            GitAppAction::CheckForUpdates,
                        );
                        true
                    }
                    (GitPanel::Status, "Enter") => {
                        let _ = send(&GitRepositoryPickerRequest {
                            path: repository.repo_root.clone(),
                        });
                        true
                    }
                    (GitPanel::Files, "a") => {
                        let _ = send(&GitStageAllRequest { path: repository.repo_root.clone() });
                        true
                    }
                    (GitPanel::Files, "s") => {
                        if repository.files.is_empty() {
                            false
                        } else {
                            GitWorkspace::operate(&repository.repo_root, GitOperation::StashPush);
                            true
                        }
                    }
                    (GitPanel::Files, "A") => {
                        let can_amend = !repository.commits.is_empty()
                            && repository.files.iter().any(|entry| entry.staged);
                        if can_amend {
                            GitWorkspace::operate(&repository.repo_root, GitOperation::Amend);
                        }
                        can_amend
                    }
                    (GitPanel::Files, " ") | (GitPanel::Files, "Space") => {
                        let Some(entry) = repository
                            .files
                            .iter()
                            .find(|entry| entry.path_bytes == selected_path_bytes())
                        else {
                            return;
                        };
                        let path = GitWorkspace::absolute_path(&repository.repo_root, &entry.path);
                        if entry.unstaged {
                            let _ = send(&GitStageRequest {
                                repo_root: repository.repo_root.clone(),
                                path,
                                path_bytes: entry.path_bytes.clone(),
                            });
                        } else if entry.staged {
                            let _ = send(&GitUnstageRequest {
                                repo_root: repository.repo_root.clone(),
                                path,
                                path_bytes: entry.path_bytes.clone(),
                            });
                        }
                        true
                    }
                    (GitPanel::Files, "x") => {
                        let Some(entry) = repository
                            .files
                            .iter()
                            .find(|entry| entry.path_bytes == selected_path_bytes())
                            .filter(|entry| entry.can_discard())
                        else {
                            return;
                        };
                        if confirm_discard() == entry.path_bytes {
                            let _ = send(&GitDiscardRequest {
                                repo_root: repository.repo_root.clone(),
                                path: GitWorkspace::absolute_path(&repository.repo_root, &entry.path),
                                path_bytes: entry.path_bytes.clone(),
                            });
                            confirm_discard.set(Vec::new());
                        } else {
                            confirm_discard.set(entry.path_bytes.clone());
                        }
                        true
                    }
                    (GitPanel::Branches, "Enter")
                    | (GitPanel::Branches, " ")
                    | (GitPanel::Branches, "Space")
                    | (GitPanel::Branches, "c")
                        if branch_collection() == BranchCollection::Local => {
                        let Some(branch) = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch())
                        else {
                            return;
                        };
                        GitWorkspace::select_branch(&repository.repo_root, branch);
                        true
                    }
                    (GitPanel::Branches, "r")
                    | (GitPanel::Branches, "M")
                    | (GitPanel::Branches, "f")
                        if branch_collection() == BranchCollection::Local => {
                        let Some(branch) = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch() && !branch.current)
                        else {
                            return;
                        };
                        let operation = match key.as_str() {
                            "r" => GitOperation::Rebase {
                                branch: branch.name.clone(),
                            },
                            "M" => GitOperation::Merge {
                                branch: branch.name.clone(),
                            },
                            _ => GitOperation::FastForward {
                                branch: branch.name.clone(),
                            },
                        };
                        GitWorkspace::operate(&repository.repo_root, operation);
                        true
                    }
                    (GitPanel::Branches, "n")
                        if branch_collection() == BranchCollection::Local => {
                        let base = selected_branch();
                        if base.is_empty() {
                            false
                        } else {
                            branch_draft.set(String::new());
                            branch_prompt.set(Some(BranchPrompt::Create { base }));
                            true
                        }
                    }
                    (GitPanel::Branches, "d")
                        if branch_collection() == BranchCollection::Local => {
                        let branch = repository
                            .branches
                            .iter()
                            .find(|branch| branch.name == selected_branch())
                            .filter(|branch| !branch.current && branch.checkout.is_empty());
                        if let Some(branch) = branch {
                            branch_prompt.set(Some(BranchPrompt::Delete {
                                branch: branch.name.clone(),
                            }));
                            true
                        } else {
                            false
                        }
                    }
                    (GitPanel::Commits, " ")
                    | (GitPanel::Commits, "Space")
                    | (GitPanel::Commits, "C")
                    | (GitPanel::Commits, "V")
                    | (GitPanel::Commits, "t") => {
                        let Some(commit) = repository
                            .commits
                            .iter()
                            .find(|commit| commit.sha == selected_commit())
                        else {
                            return;
                        };
                        let operation = match key.as_str() {
                            " " | "Space" => GitOperation::CheckoutCommit {
                                commit: commit.sha.clone(),
                            },
                            "t" => GitOperation::Revert {
                                commit: commit.sha.clone(),
                            },
                            _ => GitOperation::CherryPick {
                                commit: commit.sha.clone(),
                            },
                        };
                        GitWorkspace::operate(&repository.repo_root, operation);
                        true
                    }
                    (GitPanel::Stash, "g") | (GitPanel::Stash, "d") => {
                        let Some(stash) = repository
                            .stashes
                            .iter()
                            .find(|stash| stash.reference == selected_stash())
                        else {
                            return;
                        };
                        let operation = if key == "g" {
                            GitOperation::StashPop {
                                reference: stash.reference.clone(),
                            }
                        } else {
                            GitOperation::StashDrop {
                                reference: stash.reference.clone(),
                            }
                        };
                        GitWorkspace::operate(&repository.repo_root, operation);
                        true
                    }
                    _ => false,
                };
                if handled {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            if let Some(repository) = repository() {
                GitDashboard {
                    repository,
                    selected_path,
                    selected_path_bytes,
                    selected_abs_path,
                    selected_commit,
                    selected_branch,
                    branch_collection,
                    branch_prompt,
                    branch_draft,
                    selected_stash,
                    confirm_discard,
                    commit_message,
                    pending_commit_message,
                    workspace,
                    nonce,
                    markers,
                    focused_panel,
                    command_log,
                    branch_log,
                    shortcut_help,
                    fetching,
                }
            } else {
                EmptyRepository {
                    loading: loading(),
                    workspace: workspace(),
                    directory: directory(),
                    selected: directory_selected,
                    directory_children,
                    preview_path: directory_preview_path,
                    came_from: directory_came_from,
                    show_hidden: directory_show_hidden,
                    message: message(),
                }
            }
            if let Some(prompt) = branch_prompt() {
                BranchPromptDialog {
                    prompt,
                    repo_root: workspace(),
                    draft: branch_draft,
                    pending_checkout: pending_branch_checkout,
                    on_close: move |_| branch_prompt.set(None),
                }
            }
        }
    }
}

struct GitWorkspace;

impl GitWorkspace {
    fn request(path: &str) {
        if path.is_empty() {
            return;
        }
        let _ = send(&GitRepositoryRequest {
            path: path.to_string(),
        });
    }

    fn absolute_path(root: &str, relative: &str) -> String {
        if root.is_empty() || relative.is_empty() {
            return String::new();
        }
        Path::new(root).join(relative).to_string_lossy().to_string()
    }

    fn browse(path: &str, preview: bool) {
        let _ = send(&GitDirectoryRequest {
            path: path.to_string(),
            preview,
        });
    }

    fn activate(path: &str) {
        let _ = send(&ProjectCommandEvent {
            command: "activate".to_string(),
            path: Some(path.to_string()),
        });
        let _ = send(&TabWorkspaceRequest {
            path: path.to_string(),
            branch: String::new(),
            checkout: String::new(),
            pane_id: String::new(),
        });
    }

    fn select_branch(repo_root: &str, branch: &GitBranchEntry) {
        let _ = send(&ProjectCommandEvent {
            command: "activate".to_string(),
            path: Some(repo_root.to_string()),
        });
        let _ = send(&TabWorkspaceRequest {
            path: repo_root.to_string(),
            branch: branch.name.clone(),
            checkout: branch.checkout.clone(),
            pane_id: String::new(),
        });
    }

    fn select_branch_name(repo_root: &str, branch: &str) {
        let _ = send(&ProjectCommandEvent {
            command: "activate".to_string(),
            path: Some(repo_root.to_string()),
        });
        let _ = send(&TabWorkspaceRequest {
            path: repo_root.to_string(),
            branch: branch.to_string(),
            checkout: String::new(),
            pane_id: String::new(),
        });
    }

    fn operate(repo_root: &str, operation: GitOperation) {
        let _ = send(&GitOperationRequest {
            repo_root: repo_root.to_string(),
            operation,
        });
    }

    fn app_action(repo_root: &str, action: GitAppAction) {
        let _ = send(&GitAppActionRequest {
            repo_root: repo_root.to_string(),
            action,
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchPrompt {
    Create { base: String },
    Delete { branch: String },
}

impl BranchPrompt {
    fn submit(
        &self,
        repo_root: &str,
        draft: Signal<String>,
        mut pending_checkout: Signal<String>,
    ) -> bool {
        match self {
            Self::Create { base } => {
                let branch = draft().trim().to_string();
                if branch.is_empty() {
                    return false;
                }
                pending_checkout.set(branch.clone());
                GitWorkspace::operate(
                    repo_root,
                    GitOperation::CreateBranch {
                        branch,
                        start_point: base.clone(),
                    },
                );
            }
            Self::Delete { branch } => {
                GitWorkspace::operate(
                    repo_root,
                    GitOperation::DeleteBranch {
                        branch: branch.clone(),
                    },
                );
            }
        }
        true
    }
}

#[component]
fn BranchPromptDialog(
    prompt: BranchPrompt,
    repo_root: String,
    draft: Signal<String>,
    pending_checkout: Signal<String>,
    on_close: EventHandler<()>,
) -> Element {
    let title = match &prompt {
        BranchPrompt::Create { .. } => translate("git-new-branch"),
        BranchPrompt::Delete { .. } => translate("git-delete-branch"),
    };
    let is_create = matches!(prompt, BranchPrompt::Create { .. });
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                }
            },
            attributes: vec![],
            DialogContent { class: "max-w-[360px] p-4", attributes: vec![],
                DialogTitle { attributes: vec![], "{title}" }
                if is_create {
                    input {
                        class: "w-full rounded-md border border-border bg-foreground/[0.04] px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary/50",
                        autofocus: true,
                        value: "{draft}",
                        placeholder: translate("git-new-branch"),
                        oninput: move |event: Event<FormData>| draft.set(event.value()),
                        onkeydown: {
                            let prompt = prompt.clone();
                            let repo_root = repo_root.clone();
                            move |event: KeyboardEvent| {
                                event.stop_propagation();
                                if event.key() == Key::Enter
                                    && prompt.submit(&repo_root, draft, pending_checkout)
                                {
                                    event.prevent_default();
                                    on_close.call(());
                                }
                            }
                        },
                    }
                } else if let BranchPrompt::Delete { branch } = &prompt {
                    code { class: "rounded-md bg-foreground/[0.05] px-2 py-1 text-sm text-foreground", "{branch}" }
                }
                div { class: "flex justify-end gap-2",
                    Button {
                        size: ButtonSize::Xs,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| on_close.call(()),
                        {translate("common-cancel")}
                    }
                    Button {
                        size: ButtonSize::Xs,
                        variant: if is_create { ButtonVariant::Primary } else { ButtonVariant::Destructive },
                        disabled: is_create && draft().trim().is_empty(),
                        onclick: {
                            let prompt = prompt.clone();
                            move |_| {
                                if prompt.submit(&repo_root, draft, pending_checkout) {
                                    on_close.call(());
                                }
                            }
                        },
                        {if is_create { translate("common-save") } else { translate("common-delete") }}
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BranchCollection {
    #[default]
    Local,
    Remote,
    Tags,
}

impl BranchCollection {
    fn references(self, repository: &GitRepositoryEvent) -> Vec<String> {
        match self {
            Self::Local => repository
                .branches
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            Self::Remote => repository
                .remote_branches
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            Self::Tags => repository
                .tags
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
        }
    }

    fn selected_reference(self, repository: &GitRepositoryEvent, selected: &str) -> String {
        let references = self.references(repository);
        if references.iter().any(|reference| reference == selected) {
            return selected.to_string();
        }
        if self == Self::Local
            && let Some(current) = repository.branches.iter().find(|entry| entry.current)
        {
            return current.name.clone();
        }
        references.into_iter().next().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GitPanel {
    #[default]
    Status,
    Files,
    Branches,
    Commits,
    Stash,
}

#[derive(Clone, Copy)]
struct GitPanelSelection {
    path: Signal<String>,
    path_bytes: Signal<Vec<u8>>,
    absolute_path: Signal<String>,
    branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
    commit: Signal<String>,
    stash: Signal<String>,
}

impl GitPanel {
    fn menu_direction(event: &KeyboardData) -> Option<MenuDirection> {
        let modifiers = event.modifiers();
        if !modifiers.ctrl() && !modifiers.alt() && !modifiers.meta() && !modifiers.shift() {
            match event.key().to_string().as_str() {
                "j" => return Some(MenuDirection::Next),
                "k" => return Some(MenuDirection::Previous),
                _ => {}
            }
        }
        MenuDirection::from_key(event)
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "0" => Some(Self::Status),
            "1" => Some(Self::Status),
            "2" => Some(Self::Files),
            "3" => Some(Self::Branches),
            "4" => Some(Self::Commits),
            "5" => Some(Self::Stash),
            _ => None,
        }
    }

    fn next(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Status, false) => Self::Files,
            (Self::Files, false) => Self::Branches,
            (Self::Branches, false) => Self::Commits,
            (Self::Commits, false) => Self::Stash,
            (Self::Stash, false) => Self::Status,
            (Self::Status, true) => Self::Stash,
            (Self::Files, true) => Self::Status,
            (Self::Branches, true) => Self::Files,
            (Self::Commits, true) => Self::Branches,
            (Self::Stash, true) => Self::Commits,
        }
    }

    fn move_selection(
        self,
        repository: &GitRepositoryEvent,
        mut selection: GitPanelSelection,
        direction: MenuDirection,
    ) -> bool {
        match self {
            Self::Status => false,
            Self::Files => {
                let len = repository.files.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .files
                    .iter()
                    .position(|entry| entry.path_bytes == (selection.path_bytes)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                let entry = &repository.files[index];
                selection.path.set(entry.path.clone());
                selection.path_bytes.set(entry.path_bytes.clone());
                selection.absolute_path.set(GitWorkspace::absolute_path(
                    &repository.repo_root,
                    &entry.path,
                ));
                let section = if entry.staged { "staged" } else { "unstaged" };
                ScrollIntoView::nearest(&format!("git-file-{section}-row-{index}"));
                true
            }
            Self::Branches => {
                let references = (selection.branch_collection)().references(repository);
                let len = references.len();
                if len == 0 {
                    return false;
                }
                let current = references
                    .iter()
                    .position(|reference| reference == &(selection.branch)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection.branch.set(references[index].clone());
                ScrollIntoView::nearest(&format!("git-branch-row-{index}"));
                true
            }
            Self::Commits => {
                let len = repository.commits.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .commits
                    .iter()
                    .position(|entry| entry.sha == (selection.commit)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection.commit.set(repository.commits[index].sha.clone());
                ScrollIntoView::nearest(&format!("git-commit-row-{index}"));
                true
            }
            Self::Stash => {
                let len = repository.stashes.len();
                if len == 0 {
                    return false;
                }
                let current = repository
                    .stashes
                    .iter()
                    .position(|entry| entry.reference == (selection.stash)())
                    .unwrap_or(match direction {
                        MenuDirection::Next => len - 1,
                        MenuDirection::Previous => 0,
                    });
                let index = move_selection(current, len, direction);
                selection
                    .stash
                    .set(repository.stashes[index].reference.clone());
                ScrollIntoView::nearest(&format!("git-stash-row-{index}"));
                true
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct GitCommandLogEntry {
    action: String,
    message: String,
    ok: bool,
}

impl GitCommandLogEntry {
    fn from_result(result: &GitResultEvent) -> Self {
        Self {
            action: result.action.clone(),
            message: result.message.clone(),
            ok: result.ok,
        }
    }

    fn error(message: &str) -> Self {
        Self {
            action: String::new(),
            message: message.to_string(),
            ok: false,
        }
    }

    fn append(self, entries: &mut Vec<Self>) {
        if entries.len() >= 24 {
            entries.remove(0);
        }
        entries.push(self);
    }
}

impl FileStatus {
    fn label(self) -> String {
        match self {
            Self::Clean => translate("git-status-clean"),
            Self::Modified => translate("git-status-modified"),
            Self::Staged => translate("git-status-staged"),
            Self::StagedModified => translate("git-status-staged-modified"),
            Self::Untracked => translate("git-status-untracked"),
            Self::Deleted => translate("git-status-deleted"),
            Self::Conflicted => translate("git-status-conflict"),
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Clean => "·",
            Self::Modified => "M",
            Self::Staged => "A",
            Self::StagedModified => "M",
            Self::Untracked => "U",
            Self::Deleted => "D",
            Self::Conflicted => "!",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Staged | Self::StagedModified => "text-ansi-2",
            Self::Conflicted | Self::Deleted => "text-ansi-1",
            Self::Modified | Self::Untracked => "text-ansi-3",
            Self::Clean => "text-muted-foreground",
        }
    }
}

impl GitFileEntry {
    fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    fn parent(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("")
    }

    fn can_discard(&self) -> bool {
        self.unstaged && self.status != FileStatus::Untracked
    }
}

#[component]
fn GitDashboard(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    selected_commit: Signal<String>,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
    branch_prompt: Signal<Option<BranchPrompt>>,
    branch_draft: Signal<String>,
    selected_stash: Signal<String>,
    confirm_discard: Signal<Vec<u8>>,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
    workspace: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
    focused_panel: Signal<GitPanel>,
    command_log: Signal<Vec<GitCommandLogEntry>>,
    branch_log: Signal<Option<GitBranchLogEvent>>,
    shortcut_help: Signal<bool>,
    fetching: Signal<bool>,
) -> Element {
    rsx! {
        main { class: "min-h-0 flex-1 overflow-y-auto bg-[radial-gradient(120%_90%_at_50%_-20%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_55%)] p-1.5 sm:overflow-hidden sm:p-2",
            div { class: "grid min-h-full grid-cols-1 gap-1.5 sm:h-full sm:grid-cols-[minmax(18rem,0.86fr)_minmax(0,2.14fr)] sm:grid-rows-[4rem_minmax(7rem,0.55fr)_minmax(9rem,1fr)_minmax(5rem,0.75fr)_minmax(5rem,0.75fr)_1.75rem] sm:gap-2 xl:grid-cols-[minmax(22rem,0.9fr)_minmax(0,2.1fr)]",
                StatusCard { repository: repository.clone(), focused_panel, fetching }
                ChangesCard {
                    repository: repository.clone(),
                    selected_path,
                    selected_path_bytes,
                    selected_abs_path,
                    confirm_discard,
                    commit_message,
                    pending_commit_message,
                    focused_panel,
                }
                BranchesCard {
                    repository: repository.clone(),
                    selected_branch,
                    branch_collection,
                    branch_prompt,
                    branch_draft,
                    focused_panel,
                }
                HistoryCard { repository: repository.clone(), selected_commit, focused_panel }
                StashCard { repository: repository.clone(), selected_stash, focused_panel }
                if focused_panel() == GitPanel::Status {
                    StatusDetailCard { repository: repository.clone(), fetching }
                } else if focused_panel() == GitPanel::Branches {
                    BranchLogCard {
                        branch: selected_branch(),
                        branch_log,
                    }
                } else if focused_panel() == GitPanel::Commits {
                    CommitDiffCard {
                        repository: repository.clone(),
                        repo_root: workspace,
                        selected_commit,
                        nonce,
                        markers,
                    }
                } else {
                    DiffCard {
                        repo_root: workspace,
                        selected_path,
                        selected_path_bytes,
                        selected_abs_path,
                        nonce,
                        markers,
                    }
                }
                CommandLogCard { command_log }
            }
        }
        GitShortcutBar {
            repo_root: repository.repo_root,
            focused_panel,
            branch_collection,
            shortcut_help,
        }
        if shortcut_help() {
            GitShortcutHelp { shortcut_help }
        }
    }
}

#[component]
fn GitShortcutBar(
    repo_root: String,
    focused_panel: Signal<GitPanel>,
    branch_collection: Signal<BranchCollection>,
    shortcut_help: Signal<bool>,
) -> Element {
    let panel_shortcuts = match focused_panel() {
        GitPanel::Status => Vec::new(),
        GitPanel::Files => vec![
            ("space", translate("git-toggle-stage")),
            ("a", translate("git-stage-all")),
            ("s", translate("git-stash")),
            ("A", translate("git-amend")),
            ("x", translate("git-discard")),
        ],
        GitPanel::Branches if branch_collection() == BranchCollection::Local => vec![
            ("space", translate("git-checkout")),
            ("n", translate("git-new-branch")),
            ("d", translate("git-delete-branch")),
            ("r", translate("git-rebase")),
            ("M", translate("git-merge")),
            ("f", translate("git-fast-forward")),
        ],
        GitPanel::Branches => Vec::new(),
        GitPanel::Commits => vec![
            ("space", translate("git-checkout-commit")),
            ("C", translate("git-cherry-pick")),
            ("t", translate("git-revert-commit")),
        ],
        GitPanel::Stash => vec![
            ("g", translate("git-stash-pop")),
            ("d", translate("git-stash-drop")),
        ],
    };

    rsx! {
        footer { class: "flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-t border-foreground/[0.08] bg-card/92 px-2 text-[10px] text-muted-foreground backdrop-blur-xl",
            if focused_panel() == GitPanel::Status {
                ShortcutButton {
                    keycap: "e",
                    label: translate("git-edit-config"),
                    onclick: {
                        let repo_root = repo_root.clone();
                        move |_| GitWorkspace::app_action(&repo_root, GitAppAction::EditConfig)
                    },
                }
                ShortcutButton {
                    keycap: "u",
                    label: translate("settings-check-updates"),
                    onclick: {
                        let repo_root = repo_root.clone();
                        move |_| GitWorkspace::app_action(&repo_root, GitAppAction::CheckForUpdates)
                    },
                }
                ShortcutButton {
                    keycap: "enter",
                    label: translate("git-switch-recent-repository"),
                    onclick: {
                        let repo_root = repo_root.clone();
                        move |_| {
                            let _ = send(&GitRepositoryPickerRequest { path: repo_root.clone() });
                        }
                    },
                }
            } else {
                for (keycap, label) in panel_shortcuts {
                    ShortcutHint { keycap, label }
                }
                ShortcutHint { keycap: "↑↓", label: translate("git-shortcut-navigate") }
                ShortcutHint { keycap: "1–5", label: translate("git-shortcut-panels") }
            }
            button {
                r#type: "button",
                class: "ml-auto flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                onclick: move |_| shortcut_help.set(true),
                kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "?" }
                span { {translate("git-keybindings")} }
            }
        }
    }
}

#[component]
fn ShortcutButton(
    keycap: &'static str,
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
            onclick: move |event| onclick.call(event),
            kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "{keycap}" }
            span { "{label}" }
        }
    }
}

#[component]
fn ShortcutHint(keycap: &'static str, label: String) -> Element {
    rsx! {
        span { class: "flex h-6 shrink-0 items-center gap-1.5 rounded-md px-1.5",
            kbd { class: "rounded border border-foreground/10 bg-foreground/[0.055] px-1.5 py-0.5 font-mono text-[9px] font-semibold text-foreground", "{keycap}" }
            span { "{label}" }
        }
    }
}

#[component]
fn GitShortcutHelp(shortcut_help: Signal<bool>) -> Element {
    rsx! {
        div {
            class: "absolute inset-0 z-50 flex items-center justify-center bg-background/70 p-4 backdrop-blur-sm",
            onclick: move |_| shortcut_help.set(false),
            div {
                class: "max-h-[min(42rem,calc(100vh-2rem))] w-full max-w-2xl overflow-y-auto rounded-2xl border border-foreground/10 bg-card p-4 shadow-2xl sm:p-5",
                onclick: move |event| event.stop_propagation(),
                div { class: "flex items-center justify-between gap-3",
                    div {
                        h2 { class: "text-base font-semibold", {translate("git-shortcut-title")} }
                        p { class: "mt-1 text-xs text-muted-foreground", {translate("git-shortcut-description")} }
                    }
                    button {
                        r#type: "button",
                        class: "rounded-lg border border-foreground/10 bg-foreground/[0.04] px-2 py-1 font-mono text-[10px] text-muted-foreground hover:text-foreground",
                        onclick: move |_| shortcut_help.set(false),
                        "esc"
                    }
                }
                div { class: "mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2",
                    ShortcutGroup {
                        title: translate("git-shortcut-universal"),
                        shortcuts: vec![
                            ("1–5".to_string(), translate("git-shortcut-panels")),
                            ("tab".to_string(), translate("git-shortcut-next-panel")),
                            ("↑/k".to_string(), translate("git-shortcut-previous")),
                            ("↓/j".to_string(), translate("git-shortcut-next")),
                            ("ctrl+p".to_string(), translate("git-shortcut-previous")),
                            ("ctrl+n".to_string(), translate("git-shortcut-next")),
                            ("?".to_string(), translate("git-shortcut-help")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-status"),
                        shortcuts: vec![
                            ("f".to_string(), translate("git-fetch")),
                            ("p".to_string(), translate("git-pull")),
                            ("P".to_string(), translate("git-push-label")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-files"),
                        shortcuts: vec![
                            ("space".to_string(), translate("git-toggle-stage")),
                            ("a".to_string(), translate("git-stage-all")),
                            ("s".to_string(), translate("git-stash")),
                            ("A".to_string(), translate("git-amend")),
                            ("x x".to_string(), translate("git-discard")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-branches"),
                        shortcuts: vec![
                            ("space / enter".to_string(), translate("git-checkout")),
                            ("n".to_string(), translate("git-new-branch")),
                            ("d".to_string(), translate("git-delete-branch")),
                            ("r".to_string(), translate("git-rebase")),
                            ("M".to_string(), translate("git-merge")),
                            ("f".to_string(), translate("git-fast-forward")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-commits"),
                        shortcuts: vec![
                            ("space".to_string(), translate("git-checkout-commit")),
                            ("C / V".to_string(), translate("git-cherry-pick")),
                            ("t".to_string(), translate("git-revert-commit")),
                        ],
                    }
                    ShortcutGroup {
                        title: translate("git-stashes"),
                        shortcuts: vec![
                            ("g".to_string(), translate("git-stash-pop")),
                            ("d".to_string(), translate("git-stash-drop")),
                        ],
                    }
                }
            }
        }
    }
}

#[component]
fn ShortcutGroup(title: String, shortcuts: Vec<(String, String)>) -> Element {
    rsx! {
        section { class: "overflow-hidden rounded-xl border border-foreground/[0.08] bg-foreground/[0.02]",
            h3 { class: "border-b border-foreground/[0.07] px-3 py-2 text-xs font-semibold", "{title}" }
            div { class: "divide-y divide-foreground/[0.06]",
                for (keycap, label) in shortcuts {
                    div { class: "flex min-h-8 items-center justify-between gap-3 px-3 py-1.5 text-xs",
                        span { class: "text-muted-foreground", "{label}" }
                        kbd { class: "shrink-0 rounded-md border border-foreground/10 bg-background/70 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-foreground", "{keycap}" }
                    }
                }
            }
        }
    }
}

#[component]
fn PanelHeader(
    index: u8,
    title: String,
    count: Option<usize>,
    icon: PanelIcon,
    icon_class: &'static str,
    badge_class: &'static str,
    focused: bool,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: if focused {
                "flex h-7 shrink-0 items-center gap-1.5 border-b border-ansi-2/20 bg-gradient-to-r from-ansi-2/[0.08] to-transparent px-2"
            } else {
                "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-foreground/[0.035] to-transparent px-2"
            },
            div { class: "flex size-5 shrink-0 items-center justify-center rounded-md {icon_class}",
                match icon {
                    PanelIcon::Git => rsx! { GitIconView { class: "h-3 w-3" } },
                    PanelIcon::Line(icon) => rsx! { LineIconView { icon, class: "h-3 w-3" } },
                }
            }
            span { class: if focused { "font-mono text-[9px] font-semibold text-ansi-2" } else { "font-mono text-[9px] font-semibold text-muted-foreground" }, "[{index}]" }
            span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", "{title}" }
            if let Some(count) = count {
                Badge { class: "min-h-4 min-w-4 rounded-full border px-1 text-[8px] font-semibold tabular-nums {badge_class}", "{count}" }
            }
            if let Some(actions) = actions {
                {actions}
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PanelIcon {
    Git,
    Line(LineIcon),
}

#[component]
fn HeaderActionButton(
    icon: LineIcon,
    shortcut: &'static str,
    label: String,
    disabled: bool,
    danger: bool,
    onpress: EventHandler<()>,
) -> Element {
    let title = format!("{label} ({shortcut})");
    rsx! {
        button {
            r#type: "button",
            title: "{title}",
            aria_label: "{label}",
            class: if danger {
                "flex h-5 min-w-5 items-center justify-center rounded border border-ansi-1/20 bg-ansi-1/[0.05] px-1 font-mono text-[8px] font-semibold text-ansi-1 hover:bg-ansi-1/12 disabled:opacity-30"
            } else {
                "flex h-5 min-w-5 items-center justify-center rounded border border-foreground/10 bg-background/45 px-1 font-mono text-[8px] font-semibold text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground disabled:opacity-30"
            },
            disabled,
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                onpress.call(());
            },
            LineIconView { icon, class: "size-3" }
        }
    }
}

#[component]
fn StatusCard(
    repository: GitRepositoryEvent,
    focused_panel: Signal<GitPanel>,
    fetching: Signal<bool>,
) -> Element {
    let focused = focused_panel() == GitPanel::Status;

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused {
                "order-1 min-h-16 cursor-default border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_22%,transparent),0_14px_36px_rgb(0_0_0_/_12%)] sm:col-start-1 sm:row-start-1 sm:min-h-0 sm:order-none"
            } else {
                "order-1 min-h-16 cursor-default sm:col-start-1 sm:row-start-1 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Status),
            PanelHeader {
                index: 1,
                title: translate("git-status"),
                count: None,
                icon: PanelIcon::Git,
                icon_class: "bg-ansi-2/10 text-ansi-2 ring-1 ring-inset ring-ansi-2/15",
                badge_class: "",
                focused,
            }
            div { class: "flex min-h-0 flex-1 items-center gap-2 px-2 text-[11px]",
                span { class: "min-w-0 truncate font-semibold", "{repository.repo_name}" }
                span { class: "shrink-0 text-muted-foreground", "→" }
                span { class: "min-w-0 truncate font-medium", "{repository.branch}" }
                div { class: "ml-auto flex shrink-0 items-center gap-1.5 text-[10px] tabular-nums text-muted-foreground",
                    if fetching() {
                        FetchIndicator {}
                    }
                    if repository.ahead > 0 {
                        span { class: "inline-flex items-center gap-0.5 rounded-full border border-foreground/[0.08] bg-foreground/[0.035] px-1.5 py-0.5",
                            LineIconView { icon: LineIcon::ArrowUp, class: "h-3 w-3" }
                            "{repository.ahead}"
                        }
                    }
                    if repository.behind > 0 {
                        span { class: "inline-flex items-center gap-0.5 rounded-full border border-amber-400/20 bg-amber-400/[0.07] px-1.5 py-0.5 text-amber-400",
                            LineIconView { icon: LineIcon::ArrowDown, class: "h-3 w-3" }
                            "{repository.behind}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatusDetailCard(repository: GitRepositoryEvent, fetching: Signal<bool>) -> Element {
    let changed = repository.files.len();
    let staged = repository.files.iter().filter(|entry| entry.staged).count();
    let clean = changed == 0;

    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[26rem] border-t-ansi-2/30 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-ansi-2/[0.065] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-ansi-2/10 text-ansi-2 ring-1 ring-inset ring-ansi-2/15",
                    LineIconView { icon: LineIcon::GitBranch, class: "h-3 w-3" }
                }
                span { class: "font-mono text-[9px] font-semibold text-muted-foreground", "[0]" }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", {translate("git-status")} }
                if fetching() {
                    FetchIndicator {}
                }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-6 gap-1 rounded-md px-2 py-0 text-[10px] font-medium shadow-sm",
                    disabled: repository.branch.is_empty(),
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: repository.repo_root.clone() });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "h-3.5 w-3.5" }
                    span { class: "hidden sm:inline", {translate("git-push-label")} }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-4 sm:p-6",
                div { class: "mx-auto flex h-full max-w-3xl flex-col justify-center",
                    div { class: "flex items-start gap-4",
                        div { class: if clean {
                                "flex size-12 shrink-0 items-center justify-center rounded-2xl border border-ansi-2/20 bg-ansi-2/[0.08] text-ansi-2 shadow-[0_8px_24px_rgb(0_0_0_/_12%)]"
                            } else {
                                "flex size-12 shrink-0 items-center justify-center rounded-2xl border border-amber-400/20 bg-amber-400/[0.08] text-amber-400 shadow-[0_8px_24px_rgb(0_0_0_/_12%)]"
                            },
                            LineIconView { icon: if clean { LineIcon::ShieldCheck } else { LineIcon::GitBranch }, class: "h-5 w-5" }
                        }
                        div { class: "min-w-0 flex-1",
                            div { class: "flex flex-wrap items-center gap-2",
                                h2 { class: "truncate text-xl font-semibold tracking-[-0.025em]", "{repository.repo_name}" }
                                Badge { class: if clean {
                                        "rounded-full border border-ansi-2/20 bg-ansi-2/[0.08] px-2 py-0.5 text-[10px] font-medium text-ansi-2"
                                    } else {
                                        "rounded-full border border-amber-400/20 bg-amber-400/[0.08] px-2 py-0.5 text-[10px] font-medium text-amber-400"
                                    },
                                    if clean { {translate("git-status-clean")} } else { {translate("git-status-modified")} }
                                }
                            }
                            div { class: "mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground",
                                span { class: "inline-flex min-w-0 items-center gap-1.5 rounded-full border border-foreground/[0.08] bg-foreground/[0.035] px-2.5 py-1",
                                    LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0" }
                                    span { class: "truncate text-foreground", "{repository.branch}" }
                                }
                                span { class: "min-w-0 truncate rounded-full border border-foreground/[0.08] bg-foreground/[0.025] px-2.5 py-1",
                                    if repository.upstream.is_empty() { {translate("git-no-upstream")} } else { "{repository.upstream}" }
                                }
                            }
                        }
                    }
                    div { class: "mt-6 grid grid-cols-2 gap-2 sm:grid-cols-4",
                        StatusMetric { label: translate("git-changes"), value: changed, icon: LineIcon::File, class: "text-amber-400" }
                        StatusMetric { label: translate("git-staged-changes"), value: staged, icon: LineIcon::GitCommit, class: "text-ansi-2" }
                        StatusMetric { label: "↑".to_string(), value: repository.ahead as usize, icon: LineIcon::ArrowUp, class: "text-sky-400" }
                        StatusMetric { label: "↓".to_string(), value: repository.behind as usize, icon: LineIcon::ArrowDown, class: "text-violet-400" }
                    }
                    div { class: "mt-4 rounded-xl border border-foreground/[0.07] bg-foreground/[0.025] px-4 py-3 text-sm",
                        if clean {
                            div { class: "flex items-center gap-2 text-ansi-2",
                                LineIconView { icon: LineIcon::ShieldCheck, class: "h-4 w-4" }
                                span { class: "font-medium", {translate("git-repository-clean")} }
                            }
                        } else {
                            div { class: "flex items-center gap-2",
                                LineIconView { icon: LineIcon::File, class: "h-4 w-4 text-amber-400" }
                                span { class: "font-medium", "{changed} " {translate("git-changes")} }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FetchIndicator() -> Element {
    rsx! {
        span {
            class: "inline-flex h-5 shrink-0 items-center gap-1 rounded-full border border-sky-400/25 bg-sky-400/[0.08] px-1.5 font-medium text-sky-400",
            role: "status",
            aria_live: "polite",
            LineIconView { icon: LineIcon::RefreshCw, class: "h-3 w-3 animate-spin" }
            span { class: "hidden sm:inline", {translate("git-fetching")} }
        }
    }
}

#[component]
fn BranchLogCard(branch: String, branch_log: Signal<Option<GitBranchLogEvent>>) -> Element {
    let log = branch_log().filter(|event| event.branch == branch);
    let commit_count = log.as_ref().map(|event| event.commits.len()).unwrap_or(0);

    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-violet-400/30 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-violet-400/[0.065] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-violet-400/10 text-violet-400 ring-1 ring-inset ring-violet-400/15",
                    LineIconView { icon: LineIcon::GitCommit, class: "h-3 w-3" }
                }
                span { class: "font-mono text-[9px] font-semibold text-muted-foreground", "[0]" }
                span { class: "text-[11px] font-semibold tracking-[-0.01em]", {translate("git-log")} }
                span { class: "min-w-0 truncate rounded-full border border-violet-400/20 bg-violet-400/[0.07] px-2 py-0.5 font-mono text-[10px] text-violet-300", "{branch}" }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-3 py-2 sm:px-4",
                if let Some(log) = log {
                    if log.commits.is_empty() {
                        div { class: "flex h-full min-h-64 items-center justify-center text-sm text-muted-foreground", {translate("git-no-commits")} }
                    } else {
                        for (index, commit) in log.commits.into_iter().enumerate() {
                            article { key: "{commit.sha}", class: "relative grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2.5 py-3 first:pt-1",
                                div { class: "relative flex justify-center",
                                    if index > 0 {
                                        span { class: "absolute bottom-1/2 top-0 w-px bg-violet-400/35" }
                                    }
                                    if index + 1 < commit_count {
                                        span { class: "absolute bottom-0 top-1/2 w-px bg-violet-400/35" }
                                    }
                                    span { class: if index == 0 {
                                            "relative mt-1.5 size-2.5 rounded-full border-2 border-violet-300 bg-violet-400 shadow-[0_0_0_3px_color-mix(in_oklab,var(--card)_92%,transparent)]"
                                        } else {
                                            "relative mt-1.5 size-2 rounded-full border border-violet-400 bg-card shadow-[0_0_0_3px_color-mix(in_oklab,var(--card)_92%,transparent)]"
                                        } }
                                }
                                div { class: "min-w-0 border-b border-foreground/[0.06] pb-3",
                                    div { class: "flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1",
                                        code { class: "shrink-0 font-mono text-[11px] font-semibold text-amber-400", "{commit.short_sha}" }
                                        if !commit.references.is_empty() {
                                            span { class: "min-w-0 truncate rounded-full border border-ansi-2/20 bg-ansi-2/[0.07] px-1.5 py-0.5 font-mono text-[9px] text-ansi-2", "{commit.references}" }
                                        }
                                        span { class: "ml-auto shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
                                    }
                                    div { class: "mt-1.5 text-sm font-medium leading-5 text-foreground", "{commit.summary}" }
                                    div { class: "mt-1 text-[10px] text-muted-foreground", "{commit.author}" }
                                    if !commit.body.is_empty() {
                                        pre { class: "mt-3 whitespace-pre-wrap break-words border-l border-ansi-1/45 pl-3 font-sans text-xs leading-5 text-foreground/80", "{commit.body}" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "flex h-full min-h-64 flex-col gap-4 p-2",
                        for width in ["w-10/12", "w-7/12", "w-full", "w-9/12", "w-11/12", "w-6/12"] {
                            Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatusMetric(label: String, value: usize, icon: LineIcon, class: &'static str) -> Element {
    rsx! {
        div { class: "rounded-xl border border-foreground/[0.07] bg-foreground/[0.025] p-3",
            div { class: "flex items-center justify-between gap-2 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground",
                span { class: "truncate", "{label}" }
                LineIconView { icon, class: "h-3.5 w-3.5 {class}" }
            }
            div { class: "mt-2 text-xl font-semibold tabular-nums", "{value}" }
        }
    }
}

#[component]
fn CommandLogCard(command_log: Signal<Vec<GitCommandLogEntry>>) -> Element {
    let entries = command_log();

    rsx! {
        Card { variant: CardVariant::Panel, class: "order-3 min-h-36 border-t-sky-400/25 sm:col-start-2 sm:row-start-5 sm:row-span-2 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-sky-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                    LineIconView { icon: LineIcon::Terminal, class: "h-3 w-3" }
                }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", {translate("git-command-log")} }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5 font-mono text-[10px]",
                if entries.is_empty() {
                    div { class: "flex h-full min-h-20 items-center px-2 text-muted-foreground", {translate("git-command-log-empty")} }
                } else {
                    for (index, entry) in entries.iter().rev().enumerate() {
                        div { key: "{index}-{entry.action}", class: "flex min-h-6 items-start gap-2 rounded-md px-2 py-1 hover:bg-foreground/[0.035]",
                            span { class: if entry.ok { "mt-1 size-1.5 shrink-0 rounded-full bg-ansi-2" } else { "mt-1 size-1.5 shrink-0 rounded-full bg-ansi-1" } }
                            span { class: "shrink-0 font-semibold text-foreground",
                                if entry.action.is_empty() { {translate("git-command-error")} } else { "{entry.action}" }
                            }
                            span { class: if entry.ok { "min-w-0 truncate text-muted-foreground" } else { "min-w-0 break-words text-ansi-1" }, "{entry.message}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChangesCard(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<Vec<u8>>,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    for (index, entry) in repository.files.iter().enumerate() {
        if entry.staged {
            staged.push((index, entry.clone()));
        }
        if entry.unstaged {
            unstaged.push((index, entry.clone()));
        }
    }
    let staged_count = staged.len() as u32;
    let can_stash = !repository.files.is_empty();
    let can_amend = staged_count > 0 && !repository.commits.is_empty();
    let file_actions = rsx! {
        div { class: "flex shrink-0 items-center gap-0.5",
            HeaderActionButton {
                icon: LineIcon::Package,
                shortcut: "s",
                label: translate("git-stash"),
                disabled: !can_stash,
                danger: false,
                onpress: {
                    let repo_root = repository.repo_root.clone();
                    move |_| GitWorkspace::operate(&repo_root, GitOperation::StashPush)
                },
            }
            HeaderActionButton {
                icon: LineIcon::Pencil,
                shortcut: "A",
                label: translate("git-amend"),
                disabled: !can_amend,
                danger: false,
                onpress: {
                    let repo_root = repository.repo_root.clone();
                    move |_| GitWorkspace::operate(&repo_root, GitOperation::Amend)
                },
            }
        }
    };

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Files {
                if repository.files.is_empty() {
                    "order-4 min-h-[13rem] border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                } else {
                    "order-4 min-h-[19rem] border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                }
            } else {
                if repository.files.is_empty() {
                    "order-4 min-h-[13rem] border-t-amber-400/30 sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                } else {
                    "order-4 min-h-[19rem] border-t-amber-400/30 sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                }
            },
            onclick: move |_| focused_panel.set(GitPanel::Files),
            PanelHeader {
                index: 2,
                title: translate("git-files"),
                count: Some(repository.files.len()),
                icon: PanelIcon::Line(LineIcon::File),
                icon_class: "bg-amber-400/10 text-amber-500 ring-1 ring-inset ring-amber-400/15",
                badge_class: "border-amber-400/20 bg-amber-400/[0.08] text-amber-500",
                focused: focused_panel() == GitPanel::Files,
                actions: file_actions,
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                if repository.files.is_empty() {
                    div { class: "flex h-full min-h-16 items-center justify-center gap-2 px-3 text-center text-xs text-muted-foreground",
                        LineIconView { icon: LineIcon::ShieldCheck, class: "h-4 w-4 shrink-0 text-ansi-2" }
                        div { class: "truncate font-medium text-foreground", {translate("git-repository-clean")} }
                    }
                } else {
                    if !staged.is_empty() {
                        FileSection {
                            title: translate("git-staged-changes"),
                            files: staged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: true,
                            selected_path,
                            selected_path_bytes,
                            selected_abs_path,
                            confirm_discard,
                            focused_panel,
                        }
                    }
                    if !unstaged.is_empty() {
                        FileSection {
                            title: translate("git-unstaged-changes"),
                            files: unstaged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: false,
                            selected_path,
                            selected_path_bytes,
                            selected_abs_path,
                            confirm_discard,
                            focused_panel,
                        }
                    }
                }
            }
            CommitPanel {
                repo_root: repository.repo_root,
                staged_count,
                commit_message,
                pending_commit_message,
            }
        }
    }
}

#[component]
fn FileSection(
    title: String,
    files: Vec<(usize, GitFileEntry)>,
    repo_root: String,
    staged_view: bool,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<Vec<u8>>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    rsx! {
        div { class: "pb-1 last:pb-0",
            div { class: "flex h-7 items-center justify-between px-2.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-muted-foreground",
                span { "{title}" }
                span { class: "tabular-nums", "{files.len()}" }
            }
            div {
                for (row_index, entry) in files {
                    FileRow {
                        key: "{staged_view}-{entry.path_bytes:?}",
                        row_index,
                        entry,
                        repo_root: repo_root.clone(),
                        staged_view,
                        selected_path,
                        selected_path_bytes,
                        selected_abs_path,
                        confirm_discard,
                        focused_panel,
                    }
                }
            }
        }
    }
}

#[component]
fn FileRow(
    row_index: usize,
    entry: GitFileEntry,
    repo_root: String,
    staged_view: bool,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<Vec<u8>>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let absolute = GitWorkspace::absolute_path(&repo_root, &entry.path);
    let selected = selected_path_bytes() == entry.path_bytes;
    let file_path = entry.path.clone();
    let file_path_bytes = entry.path_bytes.clone();
    let file_name = entry.name().to_string();
    let parent = entry.parent().to_string();
    let status_label = entry.status.label();
    let status_code = entry.status.code();
    let status_class = entry.status.class();
    let can_discard = entry.can_discard() && !staged_view;
    let confirming = confirm_discard() == entry.path_bytes;
    let section = if staged_view { "staged" } else { "unstaged" };
    let row_id = format!("git-file-{section}-row-{row_index}");

    rsx! {
        div {
            id: "{row_id}",
            class: if selected {
                "group mx-1 flex min-h-8 cursor-default items-center gap-2 rounded-md bg-primary/[0.10] px-2 text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "group mx-1 flex min-h-8 cursor-default items-center gap-2 rounded-md px-2 hover:bg-foreground/[0.045]"
            },
            onclick: {
                let file_path = file_path.clone();
                let file_path_bytes = file_path_bytes.clone();
                let absolute = absolute.clone();
                move |_| {
                    focused_panel.set(GitPanel::Files);
                    selected_path.set(file_path.clone());
                    selected_path_bytes.set(file_path_bytes.clone());
                    selected_abs_path.set(absolute.clone());
                }
            },
            span { class: "w-4 shrink-0 text-center font-mono text-xs font-semibold {status_class}", title: "{status_label}", "{status_code}" }
            TypeIcon { path: entry.path.clone(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-xs", "{file_name}" }
                if !parent.is_empty() {
                    div { class: "truncate text-[10px] text-muted-foreground", "{parent}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-0.5 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100",
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-7 w-7 p-0 text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground",
                    title: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    aria_label: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    onclick: {
                        let absolute = absolute.clone();
                        let path_bytes = file_path_bytes.clone();
                        let repo_root = repo_root.clone();
                        move |event: Event<MouseData>| {
                            focused_panel.set(GitPanel::Files);
                            event.stop_propagation();
                            if staged_view {
                                let _ = send(&GitUnstageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                            } else {
                                let _ = send(&GitStageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                            }
                        }
                    },
                    LineIconView { icon: if staged_view { LineIcon::Minus } else { LineIcon::Plus }, class: "h-3.5 w-3.5" }
                }
                if can_discard {
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: if confirming {
                            "h-7 w-7 bg-ansi-1/15 p-0 text-ansi-1 hover:bg-ansi-1/20 hover:text-ansi-1"
                        } else {
                            "h-7 w-7 p-0 text-muted-foreground hover:bg-ansi-1/10 hover:text-ansi-1"
                        },
                        title: if confirming { translate("git-confirm-discard") } else { translate("git-discard") },
                        aria_label: if confirming { translate("git-confirm-discard") } else { translate("git-discard") },
                        onclick: {
                            let absolute = absolute.clone();
                            let path_bytes = file_path_bytes.clone();
                            let repo_root = repo_root.clone();
                            move |event: Event<MouseData>| {
                                focused_panel.set(GitPanel::Files);
                                event.stop_propagation();
                                if confirm_discard() == path_bytes {
                                    let _ = send(&GitDiscardRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                                    confirm_discard.set(Vec::new());
                                } else {
                                    confirm_discard.set(path_bytes.clone());
                                }
                            }
                        },
                        LineIconView { icon: if confirming { LineIcon::AlertCircle } else { LineIcon::RotateCcw }, class: "h-3.5 w-3.5" }
                    }
                }
            }
        }
    }
}

#[component]
fn CommitPanel(
    repo_root: String,
    staged_count: u32,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
) -> Element {
    let can_commit = staged_count > 0
        && !commit_message().trim().is_empty()
        && pending_commit_message().is_empty();

    rsx! {
        div { class: "shrink-0 border-t border-foreground/[0.07] bg-foreground/[0.015] p-1.5",
            Textarea {
                variant: TextareaVariant::Outline,
                class: "min-h-8 w-full resize-none rounded-md border border-foreground/[0.09] bg-background/65 px-2 py-1.5 text-[11px] shadow-inner outline-none placeholder:text-muted-foreground focus:border-primary/40 focus:ring-2 focus:ring-primary/10",
                placeholder: translate("git-commit-message"),
                value: "{commit_message}",
                oninput: move |event: Event<FormData>| commit_message.set(event.value()),
                onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
            }
            div { class: "mt-1.5 flex items-center gap-1",
                span { class: "truncate text-[9px] text-muted-foreground", {translate("git-staged-changes")} " · {staged_count}" }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "ml-auto h-6 shrink-0 rounded-md px-2 text-[10px] font-medium shadow-sm disabled:opacity-40",
                    disabled: !can_commit,
                    onclick: move |_| {
                        let text = commit_message().trim().to_string();
                        if text.is_empty() {
                            return;
                        }
                        if send(&GitCommitRequest {
                            path: repo_root.clone(),
                            message: text.clone(),
                        })
                        .is_ok()
                        {
                            pending_commit_message.set(text);
                        }
                    },
                    {translate_with("git-commit", &[("count", TranslationValue::Number(staged_count as i64))])}
                }
            }
        }
    }
}

#[component]
fn BranchesCard(
    repository: GitRepositoryEvent,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
    branch_prompt: Signal<Option<BranchPrompt>>,
    branch_draft: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let collection = branch_collection();
    let reference_count = collection.references(&repository).len();
    let selected = if collection == BranchCollection::Local {
        repository
            .branches
            .iter()
            .find(|entry| entry.name == selected_branch())
            .cloned()
    } else {
        None
    };
    let branch_actions = selected.map(|branch| {
        let current = branch.current;
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.clone();
                        move |_| GitWorkspace::select_branch(&repo_root, &branch)
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Plus,
                    shortcut: "n",
                    label: translate("git-new-branch"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let base = branch.name.clone();
                        move |_| {
                            branch_draft.set(String::new());
                            branch_prompt.set(Some(BranchPrompt::Create { base: base.clone() }));
                        }
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-delete-branch"),
                    disabled: current || !branch.checkout.is_empty(),
                    danger: true,
                    onpress: {
                        let branch = branch.name.clone();
                        move |_| branch_prompt.set(Some(BranchPrompt::Delete { branch: branch.clone() }))
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitPullRequest,
                    shortcut: "r",
                    label: translate("git-rebase"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Rebase { branch: branch.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitMerge,
                    shortcut: "M",
                    label: translate("git-merge"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Merge { branch: branch.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::FastForward,
                    shortcut: "f",
                    label: translate("git-fast-forward"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::FastForward { branch: branch.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Branches {
                "order-5 min-h-44 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-3 sm:min-h-0 sm:order-none"
            } else {
                "order-5 min-h-44 border-t-violet-400/30 sm:col-start-1 sm:row-start-3 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Branches),
            PanelHeader {
                index: 3,
                title: translate("git-branches"),
                count: Some(reference_count),
                icon: PanelIcon::Line(LineIcon::GitBranch),
                icon_class: "bg-violet-400/10 text-violet-400 ring-1 ring-inset ring-violet-400/15",
                badge_class: "border-violet-400/20 bg-violet-400/[0.08] text-violet-400",
                focused: focused_panel() == GitPanel::Branches,
                actions: rsx! {
                    div { class: "flex shrink-0 items-center gap-1",
                        BranchCollectionTabs {
                            repository: repository.clone(),
                            selected_branch,
                            branch_collection,
                        }
                        {branch_actions}
                    }
                },
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-0.5",
                match collection {
                    BranchCollection::Local => rsx! {
                        for (index, branch) in repository.branches.into_iter().enumerate() {
                            BranchRow {
                                key: "local-{branch.name}",
                                branch,
                                index,
                                remote: false,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    },
                    BranchCollection::Remote => rsx! {
                        for (index, branch) in repository.remote_branches.into_iter().enumerate() {
                            BranchRow {
                                key: "remote-{branch.name}",
                                branch,
                                index,
                                remote: true,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    },
                    BranchCollection::Tags => rsx! {
                        for (index, tag) in repository.tags.into_iter().enumerate() {
                            TagRow {
                                key: "tag-{tag.name}",
                                tag,
                                index,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchCollectionTabs(
    repository: GitRepositoryEvent,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
) -> Element {
    rsx! {
        div { class: "flex h-5 shrink-0 items-center rounded-md bg-background/50 p-0.5 ring-1 ring-inset ring-foreground/10",
            for (collection, label) in [
                (BranchCollection::Local, translate("git-local")),
                (BranchCollection::Remote, translate("git-remote")),
                (BranchCollection::Tags, translate("git-tags")),
            ] {
                button {
                    r#type: "button",
                    class: if branch_collection() == collection {
                        "h-4 rounded px-1.5 text-[8px] font-semibold text-foreground shadow-sm bg-foreground/[0.10]"
                    } else {
                        "h-4 rounded px-1.5 text-[8px] font-medium text-muted-foreground hover:text-foreground"
                    },
                    onclick: {
                        let repository = repository.clone();
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            branch_collection.set(collection);
                            selected_branch.set(collection.selected_reference(&repository, ""));
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

#[component]
fn BranchRow(
    branch: GitBranchEntry,
    index: usize,
    remote: bool,
    selected_branch: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let name = branch.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: if branch.checkout.is_empty() { branch.upstream.clone() } else { branch.checkout.clone() },
            class: if selected_branch() == branch.name {
                "flex h-5 w-full items-center justify-start gap-1 rounded px-1.5 text-left text-foreground bg-primary/[0.10] shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "flex h-5 w-full items-center justify-start gap-1 rounded px-1.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                focused_panel.set(GitPanel::Branches);
                selected_branch.set(name.clone());
            },
            BranchMarker {
                current: branch.current,
                upstream: branch.upstream.clone(),
                worktree: !branch.checkout.is_empty(),
                remote,
            }
            div { class: "flex min-w-0 flex-1 items-center gap-1.5",
                span { class: "min-w-0 flex-1 truncate text-[10px] font-medium", "{branch.name}" }
                if branch.ahead > 0 {
                    span { class: "shrink-0 rounded-full bg-sky-400/10 px-1.5 font-mono text-[8px] text-sky-400", "↑{branch.ahead}" }
                }
                if branch.behind > 0 {
                    span { class: "shrink-0 rounded-full bg-violet-400/10 px-1.5 font-mono text-[8px] text-violet-400", "↓{branch.behind}" }
                }
                if branch.current {
                    span { class: "shrink-0 rounded-full bg-ansi-2/10 px-1.5 text-[8px] font-medium text-ansi-2", {translate("common-current")} }
                } else if !branch.upstream.is_empty() {
                    span { class: "max-w-[42%] shrink truncate font-mono text-[8px] text-muted-foreground", "{branch.upstream}" }
                }
                if !branch.short_sha.is_empty() {
                    code { class: "shrink-0 font-mono text-[8px] text-muted-foreground/60", "{branch.short_sha}" }
                }
            }
        }
    }
}

#[component]
fn TagRow(
    tag: GitTagEntry,
    index: usize,
    selected_branch: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let name = tag.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: "{tag.message}",
            class: if selected_branch() == tag.name {
                "flex h-5 w-full items-center justify-start gap-1.5 rounded px-1.5 text-left text-foreground bg-primary/[0.10] shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "flex h-5 w-full items-center justify-start gap-1.5 rounded px-1.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                focused_panel.set(GitPanel::Branches);
                selected_branch.set(name.clone());
            },
            LineIconView { icon: LineIcon::Tag, class: "size-3 shrink-0 text-violet-400/75" }
            span { class: "min-w-0 flex-1 truncate text-[10px] font-medium", "{tag.name}" }
            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{tag.short_sha}" }
            span { class: "shrink-0 text-[8px] text-muted-foreground", "{tag.date}" }
        }
    }
}

#[component]
fn BranchMarker(current: bool, upstream: String, worktree: bool, remote: bool) -> Element {
    let icon = if worktree {
        LineIcon::GitFork
    } else {
        LineIcon::GitBranch
    };
    let tone = if current {
        "bg-ansi-2/12 text-ansi-2 ring-ansi-2/20"
    } else if worktree {
        "bg-amber-400/10 text-amber-400 ring-amber-400/20"
    } else if remote || !upstream.is_empty() {
        "bg-sky-400/10 text-sky-400 ring-sky-400/20"
    } else {
        "bg-violet-400/10 text-violet-400 ring-violet-400/20"
    };
    let title = if current {
        translate("common-current")
    } else if worktree {
        translate("layout-worktree")
    } else if remote {
        translate("git-remote")
    } else if !upstream.is_empty() {
        upstream
    } else {
        translate("git-local")
    };
    rsx! {
        span {
            class: "flex size-4 shrink-0 items-center justify-center rounded-md ring-1 ring-inset {tone}",
            title,
            LineIconView { icon, class: "size-2.5" }
        }
    }
}

#[component]
fn HistoryCard(
    repository: GitRepositoryEvent,
    selected_commit: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let selected = repository
        .commits
        .iter()
        .find(|entry| entry.sha == selected_commit())
        .cloned();
    let commit_actions = selected.clone().map(|commit| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout-commit"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::CheckoutCommit { commit: commit.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitCommit,
                    shortcut: "C",
                    label: translate("git-cherry-pick"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::CherryPick { commit: commit.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::RotateCcw,
                    shortcut: "t",
                    label: translate("git-revert-commit"),
                    disabled: false,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let commit = commit.sha.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Revert { commit: commit.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Commits {
                "order-6 min-h-48 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-4 sm:row-span-2 sm:min-h-0 sm:order-none"
            } else {
                "order-6 min-h-48 border-t-sky-400/30 sm:col-start-1 sm:row-start-4 sm:row-span-2 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Commits),
            PanelHeader {
                index: 4,
                title: translate("git-commits"),
                count: Some(repository.commits.len()),
                icon: PanelIcon::Line(LineIcon::Clock),
                icon_class: "bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                badge_class: "border-sky-400/20 bg-sky-400/[0.08] text-sky-400",
                focused: focused_panel() == GitPanel::Commits,
                actions: commit_actions,
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                if repository.commits.is_empty() {
                    div { class: "flex h-full min-h-32 items-center justify-center p-4 text-center text-xs text-muted-foreground", {translate("git-no-commits")} }
                }
                for (index, commit) in repository.commits.into_iter().enumerate() {
                    Button {
                        id: "git-commit-row-{index}",
                        variant: ButtonVariant::Ghost,
                        key: "{commit.sha}",
                        title: "{commit.summary} — {commit.author} — {commit.date}",
                        class: if selected_commit() == commit.sha {
                            "h-6 w-full justify-start gap-1.5 rounded-md bg-primary/[0.10] px-1.5 py-0 text-left text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
                        } else {
                            "h-6 w-full justify-start gap-1.5 rounded-md px-1.5 py-0 text-left text-foreground hover:bg-foreground/[0.045]"
                        },
                        onclick: {
                            let sha = commit.sha.clone();
                            move |_| {
                                focused_panel.set(GitPanel::Commits);
                                selected_commit.set(sha.clone());
                            }
                        },
                        LineIconView { icon: LineIcon::GitCommit, class: "h-3 w-3 shrink-0 text-sky-400/70" }
                        code { class: "shrink-0 font-mono text-[9px] text-sky-400", "{commit.short_sha}" }
                        span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{commit.summary}" }
                        span { class: "shrink-0 text-[8px] text-muted-foreground", "{commit.date}" }
                    }
                }
            }
        }
    }
}

#[component]
fn StashCard(
    repository: GitRepositoryEvent,
    selected_stash: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let selected = repository
        .stashes
        .iter()
        .find(|entry| entry.reference == selected_stash())
        .cloned();
    let stashes = repository.stashes.clone();
    let stash_actions = selected.clone().map(|stash| {
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Package,
                    shortcut: "g",
                    label: translate("git-stash-pop"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::StashPop { reference: reference.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-stash-drop"),
                    disabled: false,
                    danger: true,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let reference = stash.reference.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::StashDrop { reference: reference.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Stash {
                "order-7 min-h-28 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-6 sm:min-h-0 sm:order-none"
            } else {
                "order-7 min-h-28 border-t-rose-400/30 sm:col-start-1 sm:row-start-6 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Stash),
            PanelHeader {
                index: 5,
                title: translate("git-stashes"),
                count: Some(stashes.len()),
                icon: PanelIcon::Line(LineIcon::Package),
                icon_class: "bg-rose-400/10 text-rose-400 ring-1 ring-inset ring-rose-400/15",
                badge_class: "border-rose-400/20 bg-rose-400/[0.08] text-rose-400",
                focused: focused_panel() == GitPanel::Stash,
                actions: stash_actions,
            }
            if !stashes.is_empty() {
                div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                    for (index, stash) in stashes.into_iter().enumerate() {
                        Button {
                            id: "git-stash-row-{index}",
                            variant: ButtonVariant::Ghost,
                            key: "{stash.reference}",
                            title: "{stash.message}",
                            class: if selected_stash() == stash.reference {
                                "h-6 w-full justify-start gap-1.5 rounded-md bg-primary/[0.10] px-1.5 py-0 text-left text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
                            } else {
                                "h-6 w-full justify-start gap-1.5 rounded-md px-1.5 py-0 text-left text-foreground hover:bg-foreground/[0.045]"
                            },
                            onclick: {
                                let reference = stash.reference.clone();
                                move |_| {
                                    focused_panel.set(GitPanel::Stash);
                                    selected_stash.set(reference.clone());
                                }
                            },
                            span { class: "shrink-0 rounded bg-rose-400/[0.08] px-1 font-mono text-[8px] text-rose-400", "{stash.index}" }
                            span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{stash.message}" }
                            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{stash.reference}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CommitDiffCard(
    repository: GitRepositoryEvent,
    repo_root: Signal<String>,
    selected_commit: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    let empty_path = use_signal(String::new);
    let selected = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit())
        .cloned();
    let title = selected
        .as_ref()
        .map(|commit| format!("{}  {}", commit.short_sha, commit.summary))
        .unwrap_or_else(|| translate("git-no-commits"));
    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-sky-400/25 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-sky-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                    LineIconView { icon: LineIcon::GitCommit, class: "size-3" }
                }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{title}" }
                span { class: "rounded-full border border-sky-400/20 bg-sky-400/[0.08] px-2 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-sky-400", "Diff" }
            }
            if selected.is_none() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-no-commits")}
                }
            } else {
                DiffView {
                    repo_root,
                    path: empty_path,
                    reference: selected_commit(),
                    nonce,
                    visible: true,
                    markers,
                }
            }
        }
    }
}

#[component]
fn DiffCard(
    repo_root: Signal<String>,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-emerald-400/25 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-emerald-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-emerald-400/10 text-emerald-400 ring-1 ring-inset ring-emerald-400/15",
                    TypeIcon { path: selected_path(), is_dir: false, class: "h-3 w-3" }
                }
                span { class: "min-w-0 flex-1 truncate font-mono text-[11px] font-medium",
                    if selected_path().is_empty() { {translate("git-select-file")} } else { "{selected_path}" }
                }
                span { class: "rounded-full border border-emerald-400/20 bg-emerald-400/[0.08] px-2 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-emerald-400", "Diff" }
            }
            if selected_abs_path().is_empty() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-select-file")}
                }
            } else {
                DiffView { repo_root, path: selected_abs_path, path_bytes: selected_path_bytes(), nonce, visible: true, markers }
            }
        }
    }
}

#[component]
fn EmptyRepository(
    loading: bool,
    workspace: String,
    directory: Option<GitDirectoryEvent>,
    selected: Signal<usize>,
    directory_children: Signal<Option<Vec<FileDirEntry>>>,
    preview_path: Signal<String>,
    came_from: Signal<String>,
    show_hidden: Signal<bool>,
    message: String,
) -> Element {
    let Some(directory) = directory else {
        return rsx! {
            if loading {
                GitLoadingSkeleton {}
            } else {
                main { class: "flex min-h-0 flex-1 items-center justify-center p-8",
                    div { class: "text-center",
                        LineIconView { icon: LineIcon::GitBranch, class: "mx-auto h-7 w-7 text-muted-foreground" }
                        h2 { class: "mt-4 text-lg font-semibold", {translate("git-no-repository")} }
                        if !workspace.is_empty() {
                            div { class: "mt-2 break-all font-mono text-xs text-muted-foreground", "{workspace}" }
                        }
                    }
                }
            }
        };
    };
    let path = directory.path.clone();
    let entries = directory.entries.clone();
    let action_directory = directory.clone();

    rsx! {
        main { class: "flex min-h-0 flex-1 flex-col overflow-hidden bg-background bg-[radial-gradient(120%_80%_at_50%_-10%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_60%)] font-mono text-sm leading-normal",
            div { class: "relative z-20 flex h-7 shrink-0 items-center gap-1 overflow-hidden border-b border-foreground/[0.07] bg-background/40 px-4 font-sans text-ui text-muted-foreground",
                TypeIcon { path: path.clone(), is_dir: true, class: "h-3.5 w-3.5 shrink-0 opacity-80" }
                span { class: "truncate text-foreground/90", "{path}" }
                if loading {
                    LineIconView { icon: LineIcon::RefreshCw, class: "ml-auto h-3.5 w-3.5 shrink-0 animate-spin" }
                } else if !message.is_empty() {
                    span { class: "ml-auto truncate text-ansi-1", "{message}" }
                }
            }
            DirectoryNavigator {
                path,
                parent_entries: directory.parent_entries,
                entries,
                children: directory_children(),
                selected: selected(),
                thumbs: HashMap::new(),
                show_hidden: show_hidden(),
                preview: rsx! { div { class: "text-xs text-muted-foreground opacity-60", "" } },
                on_action: move |action| match action {
                    DirectoryNavigatorAction::Select { index, entry } => {
                        selected.set(index);
                        directory_children.set(None);
                        preview_path.set(String::new());
                        if entry.is_dir {
                            preview_path.set(entry.path.clone());
                            GitWorkspace::browse(&entry.path, true);
                        }
                    }
                    DirectoryNavigatorAction::Ascend { target } => {
                        if action_directory.parent_path.is_empty() {
                            return;
                        }
                        came_from.set(target);
                        directory_children.set(None);
                        GitWorkspace::browse(&action_directory.parent_path, false);
                    }
                    DirectoryNavigatorAction::Descend { target } => {
                        let Some(entry) = action_directory.entries.get(selected()) else {
                            return;
                        };
                        if !entry.is_dir {
                            return;
                        }
                        came_from.set(target);
                        directory_children.set(None);
                        GitWorkspace::browse(&entry.path, false);
                    }
                    DirectoryNavigatorAction::Open { entry } => {
                        if entry.is_dir {
                            came_from.set(String::new());
                            directory_children.set(None);
                            GitWorkspace::browse(&entry.path, false);
                        }
                    }
                    DirectoryNavigatorAction::ToggleHidden => {
                        let next = !show_hidden();
                        show_hidden.set(next);
                        let entries = visible_directory_entries(&action_directory.entries, next);
                        let index = selected().min(entries.len().saturating_sub(1));
                        selected.set(index);
                        directory_children.set(None);
                        preview_path.set(String::new());
                        if let Some(entry) = entries.get(index).filter(|entry| entry.is_dir) {
                            preview_path.set(entry.path.clone());
                            GitWorkspace::browse(&entry.path, true);
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn GitLoadingSkeleton() -> Element {
    rsx! {
        main { class: "min-h-0 flex-1 overflow-hidden bg-[radial-gradient(120%_90%_at_50%_-20%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_55%)] p-3",
            div { class: "grid h-full grid-cols-1 gap-3 sm:grid-cols-[minmax(17rem,0.78fr)_minmax(0,1.72fr)] sm:grid-rows-3",
                div { class: "flex min-h-52 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-span-1",
                    Skeleton { class: "h-5 w-28 bg-foreground/[0.08]" }
                    Skeleton { class: "h-9 w-full bg-foreground/[0.05]" }
                    Skeleton { class: "h-9 w-4/5 bg-foreground/[0.04]" }
                    Skeleton { class: "mt-auto h-16 w-full bg-foreground/[0.04]" }
                }
                div { class: "flex min-h-44 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-start-2",
                    Skeleton { class: "h-5 w-24 bg-foreground/[0.08]" }
                    Skeleton { class: "h-8 w-full bg-foreground/[0.05]" }
                    Skeleton { class: "h-8 w-3/4 bg-foreground/[0.04]" }
                }
                div { class: "flex min-h-48 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-start-3",
                    Skeleton { class: "h-5 w-20 bg-foreground/[0.08]" }
                    for width in ["w-full", "w-11/12", "w-4/5", "w-10/12"] {
                        Skeleton { class: "h-7 {width} bg-foreground/[0.045]" }
                    }
                }
                div { class: "hidden min-h-0 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:col-start-2 sm:row-start-1 sm:row-span-3 sm:flex",
                    Skeleton { class: "h-5 w-40 bg-foreground/[0.08]" }
                    for width in ["w-10/12", "w-full", "w-9/12", "w-11/12", "w-8/12", "w-full", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.04]" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GitPanel;

    #[test]
    fn number_keys_select_panels() {
        assert_eq!(GitPanel::from_key("0"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("1"), Some(GitPanel::Status));
        assert_eq!(GitPanel::from_key("2"), Some(GitPanel::Files));
        assert_eq!(GitPanel::from_key("3"), Some(GitPanel::Branches));
        assert_eq!(GitPanel::from_key("4"), Some(GitPanel::Commits));
        assert_eq!(GitPanel::from_key("5"), Some(GitPanel::Stash));
        assert_eq!(GitPanel::from_key("6"), None);
    }

    #[test]
    fn tab_cycles_panels_in_both_directions() {
        assert_eq!(GitPanel::Status.next(false), GitPanel::Files);
        assert_eq!(GitPanel::Commits.next(false), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(false), GitPanel::Status);
        assert_eq!(GitPanel::Status.next(true), GitPanel::Stash);
        assert_eq!(GitPanel::Stash.next(true), GitPanel::Commits);
        assert_eq!(GitPanel::Files.next(true), GitPanel::Status);
    }
}
