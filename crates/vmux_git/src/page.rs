#![allow(non_snake_case)]

use std::collections::HashMap;
use std::path::Path;

use dioxus::prelude::*;
use vmux_core::event::{PAGE_CONTEXT_EVENT, PageContextEvent, PageContextRequest};
use vmux_ui::components::badge::Badge;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::ui::DiffView;
use crate::view::EditorDiffMarker;

pub static NATIVE_PAGE: vmux_native::NativePage =
    vmux_native::NativePage::pane(crate::GIT_PAGE_URL, Page).titled("Git");

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut workspace = use_signal(String::new);
    let mut repository = use_signal(|| Option::<GitRepositoryEvent>::None);
    let section = use_signal(|| GitSection::Changes);
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
        workspace.set(context.working_directory);
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
                RepositoryTabs { repository: repository.clone(), section }
                div { class: "flex min-h-0 flex-1",
                    match section() {
                        GitSection::Changes => rsx! {
                            ChangesView {
                                repository,
                                selected_path,
                                selected_abs_path,
                                confirm_discard,
                                commit_message,
                                nonce,
                                markers,
                            }
                        },
                        GitSection::History => rsx! {
                            HistoryView { repository, selected_commit }
                        },
                        GitSection::Branches => rsx! {
                            BranchesView { repository, selected_branch }
                        },
                    }
                }
            } else {
                EmptyRepository { loading: loading(), workspace: workspace(), message: message() }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitSection {
    Changes,
    History,
    Branches,
}

impl GitSection {
    fn label(self) -> String {
        match self {
            Self::Changes => translate("git-changes"),
            Self::History => translate("history-title"),
            Self::Branches => translate("git-branches"),
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
        header { class: "flex h-14 shrink-0 items-center gap-3 border-b border-border bg-background px-4",
            LineIconView { icon: LineIcon::GitBranch, class: "h-5 w-5 shrink-0 text-muted-foreground" }
            div { class: "min-w-0 flex-1",
                div { class: "flex min-w-0 items-center gap-2",
                    h1 { class: "truncate text-base font-semibold tracking-tight", "{repo_name}" }
                    if let Some(repo) = repository.as_ref() {
                        if !repo.branch.is_empty() {
                            span { class: "inline-flex min-w-0 max-w-64 items-center gap-1.5 rounded-md border border-border bg-muted/30 px-2 py-1 text-xs",
                                LineIconView { icon: LineIcon::GitBranch, class: "h-3.5 w-3.5 shrink-0" }
                                span { class: "truncate font-medium", "{repo.branch}" }
                            }
                        }
                        if repo.ahead > 0 || repo.behind > 0 {
                            span { class: "flex shrink-0 items-center gap-2 text-xs text-muted-foreground",
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
                div { class: "truncate text-[11px] text-muted-foreground", "{repo_root}" }
            }
            if !message.is_empty() {
                span { class: "max-w-80 truncate text-xs text-ansi-1", title: "{message}", "{message}" }
            }
            Button {
                variant: ButtonVariant::Outline,
                class: "h-8 gap-1.5 rounded-md px-2.5 py-0 text-xs",
                disabled: loading || repo_root.is_empty(),
                onclick: move |_| GitWorkspace::request(&repo_root),
                LineIconView {
                    icon: LineIcon::RefreshCw,
                    class: if loading { "h-3.5 w-3.5 animate-spin" } else { "h-3.5 w-3.5" },
                }
                {translate("common-refresh")}
            }
            if let Some(repo) = repository {
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-8 gap-1.5 rounded-md px-3 py-0 text-xs font-medium",
                    disabled: repo.branch.is_empty(),
                    onclick: move |_| {
                        let _ = send(&GitPushRequest { path: repo.repo_root.clone() });
                    },
                    LineIconView { icon: LineIcon::Upload, class: "h-3.5 w-3.5" }
                    {translate("git-push")}
                }
            }
        }
    }
}

#[component]
fn RepositoryTabs(repository: GitRepositoryEvent, section: Signal<GitSection>) -> Element {
    let items = [
        (GitSection::Changes, repository.files.len()),
        (GitSection::History, repository.commits.len()),
        (GitSection::Branches, repository.branches.len()),
    ];

    rsx! {
        nav { class: "flex h-11 shrink-0 items-end gap-1 border-b border-border bg-background px-4",
            for (item, count) in items {
                Button {
                    variant: ButtonVariant::Ghost,
                    key: "{item:?}",
                    class: if section() == item {
                        "h-10 gap-2 rounded-none border-b-2 border-foreground bg-transparent px-3 py-0 text-sm font-medium text-foreground hover:bg-foreground/[0.04]"
                    } else {
                        "h-10 gap-2 rounded-none border-b-2 border-transparent px-3 py-0 text-sm text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground"
                    },
                    onclick: move |_| section.set(item),
                    match item {
                        GitSection::Changes => rsx! { LineIconView { icon: LineIcon::File, class: "h-4 w-4 shrink-0" } },
                        GitSection::History => rsx! { LineIconView { icon: LineIcon::Clock, class: "h-4 w-4 shrink-0" } },
                        GitSection::Branches => rsx! { LineIconView { icon: LineIcon::GitBranch, class: "h-4 w-4 shrink-0" } },
                    }
                    span { "{item.label()}" }
                    Badge { class: "min-h-4 min-w-4 rounded-full bg-muted px-1.5 text-[10px] tabular-nums", "{count}" }
                }
            }
        }
    }
}

#[component]
fn ChangesView(
    repository: GitRepositoryEvent,
    selected_path: Signal<String>,
    selected_abs_path: Signal<String>,
    confirm_discard: Signal<String>,
    commit_message: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
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
        div { class: "flex min-w-0 flex-1",
            section { class: "flex w-[360px] shrink-0 flex-col border-r border-border bg-muted/[0.08]",
                div { class: "min-h-0 flex-1 overflow-y-auto",
                    if repository.files.is_empty() {
                        div { class: "flex h-full flex-col items-center justify-center gap-2 px-6 text-center text-sm text-muted-foreground",
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
                CommitPanel {
                    repo_root: repository.repo_root.clone(),
                    staged_count,
                    commit_message,
                }
            }
            section { class: "flex min-w-0 flex-1 flex-col bg-background",
                if selected_abs_path().is_empty() {
                    div { class: "flex flex-1 items-center justify-center text-sm text-muted-foreground",
                        {translate("git-select-file")}
                    }
                } else {
                    div { class: "flex h-10 shrink-0 items-center gap-2 border-b border-border bg-muted/[0.08] px-4",
                        TypeIcon { path: selected_path(), is_dir: false, class: "h-4 w-4 shrink-0" }
                        span { class: "min-w-0 flex-1 truncate font-mono text-xs", "{selected_path}" }
                    }
                    DiffView {
                        path: selected_abs_path,
                        nonce,
                        visible: true,
                        markers,
                    }
                }
            }
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
        div { class: "border-b border-border",
            div { class: "flex h-8 items-center justify-between bg-muted/20 px-3 text-[11px] font-semibold text-muted-foreground",
                span { "{title}" }
                Badge { class: "min-h-4 min-w-4 rounded-full bg-muted px-1.5 text-[10px] tabular-nums", "{files.len()}" }
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
                "group flex min-h-10 cursor-default items-center gap-2 border-b border-l-2 border-border border-l-foreground bg-foreground/[0.06] px-2.5 last:border-b-0"
            } else {
                "group flex min-h-10 cursor-default items-center gap-2 border-b border-l-2 border-border border-l-transparent px-2.5 hover:bg-foreground/[0.04] last:border-b-0"
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
            div { class: "flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100",
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-7 w-7 p-0 text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground",
                    title: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    aria_label: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    onclick: {
                        let absolute = absolute.clone();
                        move |event: Event<MouseData>| {
                            event.stop_propagation();
                            if staged_view {
                                let _ = send(&GitUnstageRequest { path: absolute.clone() });
                            } else {
                                let _ = send(&GitStageRequest { path: absolute.clone() });
                            }
                        }
                    },
                    LineIconView {
                        icon: if staged_view { LineIcon::Minus } else { LineIcon::Plus },
                        class: "h-3.5 w-3.5",
                    }
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
                            move |event: Event<MouseData>| {
                                event.stop_propagation();
                                if confirm_discard() == file_path {
                                    let _ = send(&GitDiscardRequest { path: absolute.clone() });
                                    confirm_discard.set(String::new());
                                } else {
                                    confirm_discard.set(file_path.clone());
                                }
                            }
                        },
                        LineIconView {
                            icon: if confirming { LineIcon::AlertCircle } else { LineIcon::RotateCcw },
                            class: "h-3.5 w-3.5",
                        }
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
        div { class: "shrink-0 border-t border-border bg-background p-3",
            Textarea {
                variant: TextareaVariant::Outline,
                class: "min-h-14 w-full resize-none rounded-md border border-border bg-muted/[0.08] px-3 py-2 text-xs outline-none placeholder:text-muted-foreground focus:border-foreground/30",
                placeholder: translate("git-commit-message"),
                value: "{commit_message}",
                oninput: move |event: Event<FormData>| commit_message.set(event.value()),
            }
            div { class: "mt-2 flex items-center justify-between gap-3",
                span { class: "text-[11px] text-muted-foreground", {translate("git-staged-changes")} " · {staged_count}" }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "h-8 rounded-md px-3 text-xs font-medium disabled:opacity-40",
                    disabled: !can_commit,
                    onclick: move |_| {
                        let text = commit_message().trim().to_string();
                        if text.is_empty() {
                            return;
                        }
                        let _ = send(&GitCommitRequest {
                            path: repo_root.clone(),
                            message: text,
                        });
                        commit_message.set(String::new());
                    },
                    {translate_with(
                        "git-commit",
                        &[("count", TranslationValue::Number(staged_count as i64))],
                    )}
                }
            }
        }
    }
}

#[component]
fn HistoryView(repository: GitRepositoryEvent, selected_commit: Signal<String>) -> Element {
    let selected = repository
        .commits
        .iter()
        .find(|entry| entry.sha == selected_commit())
        .cloned();

    rsx! {
        div { class: "flex min-w-0 flex-1",
            section { class: "w-[44%] min-w-[360px] max-w-[560px] shrink-0 overflow-y-auto border-r border-border bg-muted/[0.08]",
                div { class: "sticky top-0 z-10 flex h-10 items-center border-b border-border bg-background px-4 text-xs font-semibold",
                    {translate("git-recent-commits")}
                }
                if repository.commits.is_empty() {
                    div { class: "p-6 text-center text-sm text-muted-foreground", {translate("git-no-commits")} }
                }
                for commit in repository.commits {
                    Button {
                        variant: ButtonVariant::Ghost,
                        key: "{commit.sha}",
                        class: if selected_commit() == commit.sha {
                            "h-auto w-full justify-start gap-3 rounded-none border-b border-border bg-foreground/[0.06] px-4 py-3 text-left text-foreground"
                        } else {
                            "h-auto w-full justify-start gap-3 rounded-none border-b border-border px-4 py-3 text-left text-foreground hover:bg-foreground/[0.04]"
                        },
                        onclick: {
                            let sha = commit.sha.clone();
                            move |_| selected_commit.set(sha.clone())
                        },
                        LineIconView { icon: LineIcon::GitCommit, class: "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" }
                        div { class: "min-w-0 flex-1",
                            div { class: "truncate text-sm font-medium", "{commit.summary}" }
                            div { class: "mt-1 flex items-center gap-2 text-[11px] text-muted-foreground",
                                code { class: "font-mono", "{commit.short_sha}" }
                                span { class: "truncate", "{commit.author}" }
                                span { class: "ml-auto shrink-0", "{commit.date}" }
                            }
                        }
                    }
                }
            }
            section { class: "min-w-0 flex-1 overflow-y-auto",
                if let Some(commit) = selected {
                    div { class: "border-b border-border px-6 py-5",
                        h2 { class: "text-lg font-semibold tracking-tight", "{commit.summary}" }
                        div { class: "mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground",
                            span { "{commit.author}" }
                            span { "·" }
                            span { "{commit.date}" }
                        }
                        div { class: "mt-4 flex items-center gap-3 text-xs",
                            code { class: "rounded-md bg-muted px-2 py-1 font-mono", "{commit.sha}" }
                        }
                    }
                } else {
                    div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
                        {translate("git-no-commits")}
                    }
                }
            }
        }
    }
}

#[component]
fn BranchesView(repository: GitRepositoryEvent, selected_branch: Signal<String>) -> Element {
    let selected = repository
        .branches
        .iter()
        .find(|entry| entry.name == selected_branch())
        .cloned();

    rsx! {
        div { class: "flex min-w-0 flex-1",
            section { class: "w-[44%] min-w-[360px] max-w-[560px] shrink-0 overflow-y-auto border-r border-border bg-muted/[0.08]",
                div { class: "sticky top-0 z-10 flex h-10 items-center border-b border-border bg-background px-4 text-xs font-semibold",
                    {translate("git-local-branches")}
                }
                if repository.branches.is_empty() {
                    div { class: "p-6 text-center text-sm text-muted-foreground", {translate("git-no-branches")} }
                }
                for branch in repository.branches {
                    Button {
                        variant: ButtonVariant::Ghost,
                        key: "{branch.name}",
                        class: if selected_branch() == branch.name {
                            "h-auto w-full justify-start gap-3 rounded-none border-b border-border bg-foreground/[0.06] px-4 py-3 text-left text-foreground"
                        } else {
                            "h-auto w-full justify-start gap-3 rounded-none border-b border-border px-4 py-3 text-left text-foreground hover:bg-foreground/[0.04]"
                        },
                        onclick: {
                            let name = branch.name.clone();
                            move |_| selected_branch.set(name.clone())
                        },
                        LineIconView { icon: LineIcon::GitBranch, class: "h-4 w-4 shrink-0" }
                        div { class: "min-w-0 flex-1",
                            div { class: "flex items-center gap-2",
                                span { class: "truncate text-sm font-medium", "{branch.name}" }
                                if branch.current {
                                    Badge { class: "rounded-full border border-ansi-2/30 bg-ansi-2/10 px-2 py-0.5 text-[10px] text-ansi-2", {translate("common-current")} }
                                }
                            }
                            if !branch.upstream.is_empty() {
                                div { class: "mt-1 truncate text-[11px] text-muted-foreground", "{branch.upstream}" }
                            }
                        }
                    }
                }
            }
            section { class: "min-w-0 flex-1 overflow-y-auto",
                if let Some(branch) = selected {
                    div { class: "border-b border-border px-6 py-5",
                        div { class: "flex items-center gap-3",
                            div { class: "flex h-9 w-9 items-center justify-center rounded-md border border-border bg-muted/30",
                                LineIconView { icon: LineIcon::GitBranch, class: "h-5 w-5" }
                            }
                            div { class: "min-w-0",
                                h2 { class: "truncate text-lg font-semibold", "{branch.name}" }
                                div { class: "mt-1 text-xs text-muted-foreground",
                                    if branch.upstream.is_empty() {
                                        {translate("git-no-upstream")}
                                    } else {
                                        "{branch.upstream}"
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
                        {translate("git-no-branches")}
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyRepository(loading: bool, workspace: String, message: String) -> Element {
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
            }
        }
    }
}
