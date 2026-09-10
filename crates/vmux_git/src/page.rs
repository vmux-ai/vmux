#![allow(non_snake_case)]

use std::collections::HashMap;
use std::path::Path;

use dioxus::prelude::*;
use vmux_core::event::{PAGE_CONTEXT_EVENT, PageContextEvent, PageContextRequest};
use vmux_ui::components::badge::Badge;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

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
    let mut selected_path = use_signal(String::new);
    let mut selected_abs_path = use_signal(String::new);
    let mut selected_commit = use_signal(String::new);
    let mut selected_branch = use_signal(String::new);
    let confirm_discard = use_signal(String::new);
    let commit_message = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut message = use_signal(String::new);
    let mut nonce = use_signal(|| 0u32);
    let markers = use_signal(HashMap::<u32, EditorDiffMarker>::new);

    let _context = use_listener::<PageContextEvent, _>(PAGE_CONTEXT_EVENT, move |context| {
        let path = crate::GitUrl::parse(&context.page_url)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or(context.working_directory);
        workspace.set(path);
        if workspace().is_empty() {
            loading.set(false);
            return;
        }
        loading.set(true);
        message.set(String::new());
        GitWorkspace::request(&workspace());
    });
    let _repository = use_listener::<GitRepositoryEvent, _>(GIT_REPOSITORY_EVENT, move |event| {
        let next_path = event
            .files
            .iter()
            .find(|entry| entry.path == selected_path())
            .or_else(|| event.files.first())
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        let next_commit = event
            .commits
            .iter()
            .find(|entry| entry.sha == selected_commit())
            .or_else(|| event.commits.first())
            .map(|entry| entry.sha.clone())
            .unwrap_or_default();
        let next_branch = event
            .branches
            .iter()
            .find(|entry| entry.name == selected_branch())
            .or_else(|| event.branches.iter().find(|entry| entry.current))
            .or_else(|| event.branches.first())
            .map(|entry| entry.name.clone())
            .unwrap_or_default();
        selected_abs_path.set(GitWorkspace::absolute_path(&event.repo_root, &next_path));
        selected_path.set(next_path);
        selected_commit.set(next_commit);
        selected_branch.set(next_branch);
        workspace.set(event.repo_root.clone());
        repository.set(Some(event));
        loading.set(false);
        message.set(String::new());
    });
    let _picked =
        use_listener::<GitRepositoryPickedEvent, _>(GIT_REPOSITORY_PICKED_EVENT, move |event| {
            if event.path.is_empty() {
                return;
            }
            workspace.set(event.path.clone());
            repository.set(None);
            loading.set(true);
            message.set(String::new());
            GitWorkspace::request(&event.path);
        });
    let _result = use_listener::<GitResultEvent, _>(GIT_RESULT_EVENT, move |result| {
        if result.ok {
            message.set(String::new());
        } else {
            message.set(result.message);
        }
        nonce.set(nonce().wrapping_add(1));
        GitWorkspace::request(&workspace());
    });
    let _error = use_listener::<GitErrorEvent, _>(GIT_ERROR_EVENT, move |event| {
        message.set(event.message);
        loading.set(false);
        if repository().is_none() {
            GitWorkspace::pick(&workspace());
        }
    });
    let _changed = use_listener::<GitChangedEvent, _>(GIT_CHANGED_EVENT, move |_| {
        nonce.set(nonce().wrapping_add(1));
        GitWorkspace::request(&workspace());
    });

    use_effect(move || {
        let _ = send(&PageContextRequest {});
    });

    rsx! {
        document::Title { {translate("git-title")} }
        div { class: "flex h-screen min-w-0 flex-col bg-background text-foreground",
            GitHeader {
                repository: repository(),
                workspace,
                loading: loading(),
                message: message(),
            }
            if let Some(repository) = repository() {
                GitDashboard {
                    repository,
                    selected_path,
                    selected_abs_path,
                    selected_commit,
                    selected_branch,
                    confirm_discard,
                    commit_message,
                    workspace,
                    nonce,
                    markers,
                }
            } else {
                EmptyRepository { loading: loading(), workspace: workspace(), message: message() }
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

    fn pick(path: &str) {
        let _ = send(&GitRepositoryPickerRequest {
            path: path.to_string(),
        });
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
fn GitHeader(
    repository: Option<GitRepositoryEvent>,
    workspace: Signal<String>,
    loading: bool,
    message: String,
) -> Element {
    let repo_root = match repository.as_ref() {
        Some(repo) => repo.repo_root.clone(),
        None => workspace(),
    };
    let repo_name = repository
        .as_ref()
        .map(|repo| repo.repo_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| translate("git-title"));

    rsx! {
        header { class: "flex shrink-0 flex-wrap items-center gap-2 border-b border-border bg-background px-3 py-2 sm:min-h-14 sm:flex-nowrap sm:gap-3 sm:px-4",
            LineIconView { icon: LineIcon::GitBranch, class: "h-5 w-5 shrink-0 text-muted-foreground" }
            div { class: "min-w-0 flex-[1_1_14rem]",
                div { class: "flex min-w-0 items-center gap-2",
                    h1 { class: "truncate text-sm font-semibold tracking-tight sm:text-base", "{repo_name}" }
                    if let Some(repo) = repository.as_ref() {
                        if !repo.branch.is_empty() {
                            span { class: "inline-flex min-w-0 max-w-48 items-center gap-1.5 rounded-md border border-border bg-muted/30 px-2 py-1 text-[11px] sm:max-w-64 sm:text-xs",
                                LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0" }
                                span { class: "truncate font-medium", "{repo.branch}" }
                            }
                        }
                        if repo.ahead > 0 || repo.behind > 0 {
                            span { class: "hidden shrink-0 items-center gap-2 text-xs text-muted-foreground sm:flex",
                                span { class: "flex items-center gap-0.5",
                                    LineIconView { icon: LineIcon::ArrowUp, class: "h-3 w-3" }
                                    "{repo.ahead}"
                                }
                                span { class: "flex items-center gap-0.5",
                                    LineIconView { icon: LineIcon::ArrowDown, class: "h-3 w-3" }
                                    "{repo.behind}"
                                }
                            }
                        }
                    }
                }
                div { class: "truncate text-[10px] text-muted-foreground sm:text-[11px]", "{repo_root}" }
            }
            if !message.is_empty() {
                span { class: "order-last w-full truncate text-[11px] text-ansi-1 sm:order-none sm:w-auto sm:max-w-72", title: "{message}", "{message}" }
            }
            Button {
                variant: ButtonVariant::Outline,
                class: "h-8 gap-1.5 rounded-md px-2 py-0 text-xs sm:px-2.5",
                disabled: loading || repo_root.is_empty(),
                onclick: move |_| GitWorkspace::request(&repo_root),
                LineIconView {
                    icon: LineIcon::RefreshCw,
                    class: if loading { "h-3.5 w-3.5 animate-spin" } else { "h-3.5 w-3.5" },
                }
                span { class: "hidden sm:inline", {translate("common-refresh")} }
            }
            if let Some(repo) = repository {
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-8 gap-1.5 rounded-md px-2 py-0 text-xs font-medium sm:px-3",
                    disabled: repo.branch.is_empty(),
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: repo.repo_root.clone() });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "h-3.5 w-3.5" }
                    span { class: "hidden sm:inline", {translate("git-push")} }
                }
            }
        }
    }
}

#[component]
fn GitDashboard(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    selected_commit: Signal<String>,
    selected_branch: Signal<String>,
    confirm_discard: Signal<String>,
    commit_message: Signal<String>,
    workspace: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    rsx! {
        main { class: "min-h-0 flex-1 overflow-y-auto p-2 sm:p-3 lg:overflow-hidden",
            div { class: "grid min-h-full grid-cols-1 gap-2 sm:gap-3 md:grid-cols-2 lg:h-full lg:grid-cols-[minmax(15rem,0.85fr)_minmax(14rem,0.7fr)_minmax(24rem,1.6fr)] lg:grid-rows-[minmax(12rem,0.75fr)_minmax(18rem,1.25fr)]",
                ChangesCard {
                    repository: repository.clone(),
                    selected_path,
                    selected_abs_path,
                    confirm_discard,
                    commit_message,
                }
                BranchesCard { repository: repository.clone(), selected_branch }
                HistoryCard { repository: repository.clone(), selected_commit }
                DiffCard {
                    repo_root: workspace,
                    selected_path,
                    selected_abs_path,
                    nonce,
                    markers,
                }
            }
        }
    }
}

#[component]
fn PanelHeader(title: String, count: usize, icon: LineIcon) -> Element {
    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-2 border-b border-border bg-muted/20 px-3",
            LineIconView { icon, class: "h-3.5 w-3.5 shrink-0 text-muted-foreground" }
            span { class: "min-w-0 flex-1 truncate text-xs font-semibold", "{title}" }
            Badge { class: "min-h-4 min-w-4 rounded-full bg-muted px-1.5 text-[10px] tabular-nums text-muted-foreground", "{count}" }
        }
    }
}

#[component]
fn ChangesCard(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<String>,
    commit_message: Signal<String>,
) -> Element {
    let staged = repository
        .files
        .iter()
        .filter(|entry| entry.staged)
        .cloned()
        .collect::<Vec<_>>();
    let unstaged = repository
        .files
        .iter()
        .filter(|entry| entry.unstaged)
        .cloned()
        .collect::<Vec<_>>();
    let staged_count = staged.len() as u32;

    rsx! {
        Card { variant: CardVariant::Panel, class: "min-h-[22rem] md:min-h-[28rem] lg:col-start-1 lg:row-span-2 lg:min-h-0",
            PanelHeader { title: translate("git-changes"), count: repository.files.len(), icon: LineIcon::File }
            div { class: "min-h-0 flex-1 overflow-y-auto",
                if repository.files.is_empty() {
                    div { class: "flex h-full min-h-40 flex-col items-center justify-center gap-2 px-6 text-center text-sm text-muted-foreground",
                        LineIconView { icon: LineIcon::ShieldCheck, class: "h-6 w-6 text-ansi-2" }
                        div { class: "font-medium text-foreground", {translate("git-repository-clean")} }
                        div { {translate("git-no-changes")} }
                    }
                } else {
                    if !staged.is_empty() {
                        FileSection {
                            title: translate("git-staged-changes"),
                            files: staged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: true,
                            selected_path,
                            selected_abs_path,
                            confirm_discard,
                        }
                    }
                    if !unstaged.is_empty() {
                        FileSection {
                            title: translate("git-unstaged-changes"),
                            files: unstaged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: false,
                            selected_path,
                            selected_abs_path,
                            confirm_discard,
                        }
                    }
                }
            }
            CommitPanel { repo_root: repository.repo_root, staged_count, commit_message }
        }
    }
}

#[component]
fn FileSection(
    title: String,
    files: Vec<GitFileEntry>,
    repo_root: String,
    staged_view: bool,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<String>,
) -> Element {
    rsx! {
        div { class: "border-b border-border last:border-b-0",
            div { class: "flex h-7 items-center justify-between bg-muted/10 px-3 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                span { "{title}" }
                span { class: "tabular-nums", "{files.len()}" }
            }
            div {
                for entry in files {
                    FileRow {
                        key: "{staged_view}-{entry.path}",
                        entry,
                        repo_root: repo_root.clone(),
                        staged_view,
                        selected_path,
                        selected_abs_path,
                        confirm_discard,
                    }
                }
            }
        }
    }
}

#[component]
fn FileRow(
    entry: GitFileEntry,
    repo_root: String,
    staged_view: bool,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<String>,
) -> Element {
    let absolute = GitWorkspace::absolute_path(&repo_root, &entry.path);
    let selected = selected_path() == entry.path;
    let file_path = entry.path.clone();
    let file_name = entry.name().to_string();
    let parent = entry.parent().to_string();
    let status_label = entry.status.label();
    let status_code = entry.status.code();
    let status_class = entry.status.class();
    let can_discard = entry.can_discard() && !staged_view;
    let confirming = confirm_discard() == entry.path;

    rsx! {
        div {
            class: if selected {
                "group flex min-h-9 cursor-default items-center gap-2 border-b border-l-2 border-border border-l-foreground bg-foreground/[0.06] px-2 last:border-b-0"
            } else {
                "group flex min-h-9 cursor-default items-center gap-2 border-b border-l-2 border-border border-l-transparent px-2 hover:bg-foreground/[0.04] last:border-b-0"
            },
            onclick: {
                let file_path = file_path.clone();
                let absolute = absolute.clone();
                move |_| {
                    selected_path.set(file_path.clone());
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
            div { class: "flex shrink-0 items-center gap-0.5 opacity-100 sm:opacity-0 sm:group-hover:opacity-100",
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-7 w-7 p-0 text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground",
                    title: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    aria_label: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    onclick: {
                        let absolute = absolute.clone();
                        let repo_root = repo_root.clone();
                        move |event: Event<MouseData>| {
                            event.stop_propagation();
                            if staged_view {
                                let _ = send(&GitUnstageRequest { repo_root: repo_root.clone(), path: absolute.clone() });
                            } else {
                                let _ = send(&GitStageRequest { repo_root: repo_root.clone(), path: absolute.clone() });
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
                            let file_path = file_path.clone();
                            let repo_root = repo_root.clone();
                            move |event: Event<MouseData>| {
                                event.stop_propagation();
                                if confirm_discard() == file_path {
                                    let _ = send(&GitDiscardRequest { repo_root: repo_root.clone(), path: absolute.clone() });
                                    confirm_discard.set(String::new());
                                } else {
                                    confirm_discard.set(file_path.clone());
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
fn CommitPanel(repo_root: String, staged_count: u32, commit_message: Signal<String>) -> Element {
    let can_commit = staged_count > 0 && !commit_message().trim().is_empty();

    rsx! {
        div { class: "shrink-0 border-t border-border bg-background p-2.5",
            Textarea {
                variant: TextareaVariant::Outline,
                class: "min-h-14 w-full resize-none rounded-md border border-border bg-muted/[0.08] px-2.5 py-2 text-xs outline-none placeholder:text-muted-foreground focus:border-foreground/30",
                placeholder: translate("git-commit-message"),
                value: "{commit_message}",
                oninput: move |event: Event<FormData>| commit_message.set(event.value()),
            }
            div { class: "mt-2 flex items-center justify-between gap-2",
                span { class: "truncate text-[10px] text-muted-foreground", {translate("git-staged-changes")} " · {staged_count}" }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-7 shrink-0 rounded-md px-2.5 text-xs font-medium disabled:opacity-40",
                    disabled: !can_commit,
                    onclick: move |_| {
                        let text = commit_message().trim().to_string();
                        if text.is_empty() {
                            return;
                        }
                        let _ = send(&GitCommitRequest { path: repo_root.clone(), message: text });
                        commit_message.set(String::new());
                    },
                    {translate_with("git-commit", &[("count", TranslationValue::Number(staged_count as i64))])}
                }
            }
        }
    }
}

#[component]
fn BranchesCard(repository: GitRepositoryEvent, selected_branch: Signal<String>) -> Element {
    let selected = repository
        .branches
        .iter()
        .find(|entry| entry.name == selected_branch())
        .cloned();

    rsx! {
        Card { variant: CardVariant::Panel, class: "min-h-56 lg:col-start-2 lg:row-start-1 lg:min-h-0",
            PanelHeader { title: translate("git-branches"), count: repository.branches.len(), icon: LineIcon::GitBranch }
            div { class: "min-h-0 flex-1 overflow-y-auto",
                if repository.branches.is_empty() {
                    div { class: "flex h-full min-h-32 items-center justify-center p-4 text-center text-xs text-muted-foreground", {translate("git-no-branches")} }
                }
                for branch in repository.branches {
                    Button {
                        variant: ButtonVariant::Ghost,
                        key: "{branch.name}",
                        class: if selected_branch() == branch.name {
                            "h-auto w-full justify-start gap-2 rounded-none border-b border-border bg-foreground/[0.06] px-3 py-2 text-left text-foreground"
                        } else {
                            "h-auto w-full justify-start gap-2 rounded-none border-b border-border px-3 py-2 text-left text-foreground hover:bg-foreground/[0.04]"
                        },
                        onclick: {
                            let name = branch.name.clone();
                            move |_| selected_branch.set(name.clone())
                        },
                        LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0" }
                        div { class: "min-w-0 flex-1",
                            div { class: "flex items-center gap-2",
                                span { class: "truncate text-xs font-medium", "{branch.name}" }
                                if branch.current {
                                    Badge { class: "rounded-full border border-ansi-2/30 bg-ansi-2/10 px-1.5 py-0.5 text-[9px] text-ansi-2", {translate("common-current")} }
                                }
                            }
                            if !branch.upstream.is_empty() {
                                div { class: "mt-0.5 truncate text-[10px] text-muted-foreground", "{branch.upstream}" }
                            }
                        }
                    }
                }
            }
            if let Some(branch) = selected {
                div { class: "shrink-0 border-t border-border bg-muted/10 px-3 py-2",
                    div { class: "truncate text-xs font-medium", "{branch.name}" }
                    div { class: "mt-0.5 truncate text-[10px] text-muted-foreground",
                        if branch.upstream.is_empty() { {translate("git-no-upstream")} } else { "{branch.upstream}" }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryCard(repository: GitRepositoryEvent, selected_commit: Signal<String>) -> Element {
    let selected = repository
        .commits
        .iter()
        .find(|entry| entry.sha == selected_commit())
        .cloned();

    rsx! {
        Card { variant: CardVariant::Panel, class: "min-h-64 lg:col-start-3 lg:row-start-1 lg:min-h-0",
            PanelHeader { title: translate("history-title"), count: repository.commits.len(), icon: LineIcon::Clock }
            div { class: "min-h-0 flex-1 overflow-y-auto",
                if repository.commits.is_empty() {
                    div { class: "flex h-full min-h-32 items-center justify-center p-4 text-center text-xs text-muted-foreground", {translate("git-no-commits")} }
                }
                for commit in repository.commits {
                    Button {
                        variant: ButtonVariant::Ghost,
                        key: "{commit.sha}",
                        class: if selected_commit() == commit.sha {
                            "h-auto w-full justify-start gap-2 rounded-none border-b border-border bg-foreground/[0.06] px-3 py-2 text-left text-foreground"
                        } else {
                            "h-auto w-full justify-start gap-2 rounded-none border-b border-border px-3 py-2 text-left text-foreground hover:bg-foreground/[0.04]"
                        },
                        onclick: {
                            let sha = commit.sha.clone();
                            move |_| selected_commit.set(sha.clone())
                        },
                        LineIconView { icon: LineIcon::GitCommit, class: "mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" }
                        div { class: "min-w-0 flex-1",
                            div { class: "truncate text-xs font-medium", "{commit.summary}" }
                            div { class: "mt-0.5 flex items-center gap-2 text-[10px] text-muted-foreground",
                                code { class: "font-mono", "{commit.short_sha}" }
                                span { class: "truncate", "{commit.author}" }
                                span { class: "ml-auto shrink-0", "{commit.date}" }
                            }
                        }
                    }
                }
            }
            if let Some(commit) = selected {
                div { class: "shrink-0 border-t border-border bg-muted/10 px-3 py-2",
                    div { class: "truncate text-xs font-medium", "{commit.summary}" }
                    div { class: "mt-0.5 flex items-center gap-2 text-[10px] text-muted-foreground",
                        code { class: "truncate font-mono", "{commit.sha}" }
                        span { class: "ml-auto shrink-0", "{commit.date}" }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffCard(
    repo_root: Signal<String>,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
) -> Element {
    rsx! {
        Card { variant: CardVariant::Panel, class: "min-h-[26rem] md:col-span-2 lg:col-start-2 lg:col-span-2 lg:row-start-2 lg:min-h-0",
            div { class: "flex h-9 shrink-0 items-center gap-2 border-b border-border bg-muted/20 px-3",
                TypeIcon { path: selected_path(), is_dir: false, class: "h-3.5 w-3.5 shrink-0" }
                span { class: "min-w-0 flex-1 truncate font-mono text-xs",
                    if selected_path().is_empty() { {translate("git-select-file")} } else { "{selected_path}" }
                }
            }
            if selected_abs_path().is_empty() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-select-file")}
                }
            } else {
                DiffView { repo_root, path: selected_abs_path, nonce, visible: true, markers }
            }
        }
    }
}

#[component]
fn EmptyRepository(loading: bool, workspace: String, message: String) -> Element {
    let picker_path = workspace.clone();
    let picker_icon_path = workspace.clone();
    rsx! {
        main { class: "flex min-h-0 flex-1 items-center justify-center p-8",
            div { class: "w-full max-w-lg text-center",
                LineIconView { icon: LineIcon::GitBranch, class: "mx-auto h-7 w-7 text-muted-foreground" }
                h2 { class: "mt-4 text-lg font-semibold",
                    if loading { {translate("common-loading")} } else { {translate("git-no-repository")} }
                }
                if !workspace.is_empty() {
                    div { class: "mt-2 break-all font-mono text-xs text-muted-foreground", "{workspace}" }
                }
                if !message.is_empty() {
                    div { class: "mt-4 rounded-md bg-ansi-1/10 p-3 text-left text-xs text-ansi-1", "{message}" }
                }
                if !loading {
                    Button {
                        variant: ButtonVariant::Primary,
                        class: "mt-5 h-9 gap-2 rounded-md px-4 text-sm",
                        onclick: move |_| GitWorkspace::pick(&picker_path),
                        TypeIcon { path: picker_icon_path, is_dir: true, class: "h-4 w-4" }
                        {translate("agent-choose-repository")}
                    }
                }
            }
        }
    }
}
