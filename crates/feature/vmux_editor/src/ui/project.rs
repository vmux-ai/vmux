use std::path::Path;

use dioxus::prelude::*;
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;
use vmux_ecs::event::{FileDirEntry, FileDirectoryOpenRequest, OpenEditorItem};
use vmux_git::event::GitRepositoryRequest;
use vmux_git::state::GitUiState;
use vmux_session::SessionSummary;
use vmux_session::event::ResumeSession;
use vmux_session::state::ChatUiState;
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::i18n::{translate, translate_humanized};
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};

use super::state::FileUi;

#[component]
pub(super) fn ProjectDashboard() -> Element {
    let files = FileUi::use_root();
    let sessions = use_ui_state::<ChatUiState>();
    let git = use_ui_state::<GitUiState>();
    let directory = files.use_value(|state| state.document.directory.as_ref());
    let open_editors = files.use_value(|state| state.explorer.open_editors.as_ref());
    let root = use_memo(move || {
        directory
            .read()
            .as_ref()
            .map(|directory| directory.abs_path.clone())
            .unwrap_or_default()
    });
    let entries = use_memo(move || {
        directory
            .read()
            .as_ref()
            .map(|directory| directory.entries.clone())
            .unwrap_or_default()
    });
    let project_name = use_memo(move || ProjectPath::name(&root()));
    let mut requested_git = use_signal(String::new);
    use_effect(move || {
        let root = root();
        if root.is_empty() || requested_git() == root {
            return;
        }
        requested_git.set(root.clone());
        let _ = send(&GitRepositoryRequest { path: root });
    });

    let session_state = sessions.state.read();
    let related_sessions = session_state
        .sessions
        .sessions
        .iter()
        .filter(|session| ProjectPath::contains(&root(), &session.cwd))
        .take(8)
        .cloned()
        .collect::<Vec<_>>();
    drop(session_state);
    let recent_files = open_editors
        .read()
        .as_ref()
        .map(|editors| {
            editors
                .items
                .iter()
                .filter(|item| !item.is_dir && ProjectPath::contains(&root(), &item.path))
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let git_state = git.state.read();
    let repository = git_state
        .snapshot
        .repository
        .as_ref()
        .filter(|repository| ProjectPath::same(&repository.repo_root, &root()))
        .cloned();
    let git_loading = git_state.snapshot.loading && git_state.snapshot.workspace == root();
    drop(git_state);
    let start_url = "vmux://start/".to_string();

    rsx! {
        main { class: "flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground",
            header { class: "flex min-h-16 shrink-0 items-center gap-3 bg-foreground/[0.025] px-5 py-3",
                div { class: "flex size-9 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.055]",
                    TypeIcon { path: root(), is_dir: true, class: "size-5".to_string() }
                }
                div { class: "min-w-0 flex-1",
                    h1 { class: "truncate text-sm font-semibold tracking-[-0.015em]", "{project_name}" }
                    div { class: "flex min-w-0 items-center gap-2",
                        span { class: "truncate font-mono text-[10px] text-muted-foreground", "{root}" }
                        span { class: "hidden shrink-0 text-[10px] text-muted-foreground/55 sm:inline", {translate("start-tagline")} }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Sm,
                    class: "h-8 gap-1.5 text-xs".to_string(),
                    onclick: move |_| {
                        let _ = send(&OpenRequest {
                            value: start_url.clone(),
                            open: Some(OpenTarget::InPlace),
                        });
                    },
                    LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                    span { {translate("sessions-start")} }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto",
                div { class: "mx-auto flex w-full max-w-5xl flex-col gap-5 px-5 py-5",
                    ProjectSessions { sessions: related_sessions }
                    ProjectGit { root: root(), repository, loading: git_loading }
                    if !recent_files.is_empty() {
                        ProjectRecentFiles { files: recent_files }
                    }
                    ProjectFiles { entries: entries() }
                }
            }
        }
    }
}

#[component]
fn ProjectSessions(sessions: Vec<SessionSummary>) -> Element {
    rsx! {
        ProjectSection { title: translate("sessions-title"), count: sessions.len(),
            if sessions.is_empty() {
                div { class: "flex min-h-20 items-center justify-center px-3 text-xs text-muted-foreground",
                    {translate("sessions-inbox-empty")}
                }
            } else {
                div { class: "divide-y divide-foreground/[0.055]",
                    for session in sessions {
                        ProjectSessionRow { key: "{session.id}", session }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectSessionRow(session: SessionSummary) -> Element {
    let title = ProjectSession::title(&session);
    let preview = ProjectSession::preview(&session);
    let stage = ProjectSession::stage(&session);
    let age = ProjectTime::from_millis(session.last_activated_at).label();
    let managed = session.managed;
    let id = session.id.clone();
    let kind = session.kind.clone();
    let sid = session.sid.clone();
    let cwd = session.cwd.clone();
    rsx! {
        button {
            r#type: "button",
            class: "group flex w-full min-w-0 items-center gap-3 rounded-md px-3 py-2.5 text-left hover:bg-foreground/[0.035]",
            onclick: move |_| {
                if managed {
                    let url = vmux_session::Route::Session(vmux_session::SessionId(id.clone())).url();
                    let _ = send(&OpenRequest {
                        value: url,
                        open: Some(OpenTarget::InPlace),
                    });
                } else {
                    let _ = send(&ResumeSession {
                        kind: kind.clone(),
                        sid: sid.clone(),
                        cwd: cwd.clone(),
                    });
                }
            },
            AgentThumbnail {
                src: (!session.agent_icon.is_empty()).then(|| session.agent_icon.clone()),
                seed: session.agent.clone(),
                background: session.agent_color.clone(),
                alt: session.agent_name.clone(),
                class: "size-5 text-[6px]".to_string(),
            }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-xs font-medium text-foreground/90", "{title}" }
                if !preview.is_empty() {
                    div { class: "truncate text-[10px] text-muted-foreground/70", "{preview}" }
                }
            }
            span { class: "hidden shrink-0 rounded-full bg-foreground/[0.05] px-2 py-1 text-[9px] text-muted-foreground sm:inline-flex",
                "{stage}"
            }
            time { class: "w-10 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground/65", "{age}" }
            LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100".to_string() }
        }
    }
}

#[component]
fn ProjectGit(
    root: String,
    repository: Option<vmux_git::event::GitRepositorySnapshot>,
    loading: bool,
) -> Element {
    let git_url = vmux_git::GitUrl::from_path(Path::new(&root));
    rsx! {
        ProjectSection { title: translate("git-title"), count: repository.as_ref().map(|repository| repository.files.len()).unwrap_or_default(),
            if let Some(repository) = repository {
                button {
                    r#type: "button",
                    class: "group flex w-full min-w-0 items-center gap-3 rounded-md px-3 py-3 text-left hover:bg-foreground/[0.035]",
                    onclick: move |_| {
                        if let Some(url) = git_url.clone() {
                            let _ = send(&OpenRequest {
                                value: url,
                                open: Some(OpenTarget::InPlace),
                            });
                        }
                    },
                    GitSymbolView { symbol: GitSymbol::Repository, class: "size-4 shrink-0 text-muted-foreground".to_string() }
                    div { class: "min-w-0 flex-1",
                        div { class: "flex min-w-0 items-center gap-2",
                            span { class: "truncate text-xs font-medium", "{repository.repo_name}" }
                            span { class: "truncate font-mono text-[10px] text-muted-foreground", "{repository.branch}" }
                        }
                        div { class: "mt-1 flex items-center gap-3 text-[10px] text-muted-foreground",
                            span { {format!("{} {}", repository.files.len(), translate("git-changes"))} }
                            if repository.ahead > 0 {
                                span { class: "inline-flex items-center gap-1",
                                    LineIconView { icon: LineIcon::ArrowUp, class: "size-3".to_string() }
                                    "{repository.ahead}"
                                }
                            }
                            if repository.behind > 0 {
                                span { class: "inline-flex items-center gap-1",
                                    LineIconView { icon: LineIcon::ArrowDown, class: "size-3".to_string() }
                                    "{repository.behind}"
                                }
                            }
                        }
                    }
                    LineIconView { icon: LineIcon::ChevronDown, class: "size-3.5 -rotate-90 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100".to_string() }
                }
            } else if loading {
                div { class: "flex min-h-20 items-center gap-3 px-3",
                    GitSymbolView { symbol: GitSymbol::Repository, class: "size-4 shrink-0 text-muted-foreground".to_string() }
                    div { class: "min-w-0 flex-1 space-y-2",
                        Skeleton { class: "h-3 w-32 bg-foreground/[0.07]" }
                        Skeleton { class: "h-2.5 w-48 max-w-full bg-foreground/[0.045]" }
                    }
                }
            } else {
                div { class: "flex min-h-20 items-center gap-3 px-3 text-xs text-muted-foreground",
                    GitSymbolView { symbol: GitSymbol::Repository, class: "size-4 shrink-0".to_string() }
                    {translate("git-no-repository")}
                }
            }
        }
    }
}

#[component]
fn ProjectRecentFiles(files: Vec<OpenEditorItem>) -> Element {
    rsx! {
        ProjectSection { title: translate("editor-open-editors"), count: files.len(),
            div { class: "grid grid-cols-1 gap-px sm:grid-cols-2",
                for file in files {
                    ProjectFileButton {
                        key: "recent-{file.path}",
                        path: file.path,
                        name: file.name,
                        context: file.context,
                        is_dir: false,
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectFiles(entries: Vec<FileDirEntry>) -> Element {
    let mut expanded = use_signal(|| true);
    let visible = entries.iter().take(18).cloned().collect::<Vec<_>>();
    rsx! {
        section { class: "overflow-hidden rounded-lg bg-foreground/[0.025]",
            div { class: "flex h-10 items-center gap-2 px-3",
                div { class: "flex min-w-0 flex-1 items-center gap-2",
                    LineIconView { icon: LineIcon::File, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                    h2 { class: "truncate text-xs font-semibold", {translate("git-files")} }
                    span { class: "text-[10px] tabular-nums text-muted-foreground", "{entries.len()}" }
                }
                SheetHeaderChevron {
                    expanded: expanded(),
                    label: if expanded() { translate("common-collapse") } else { translate("common-expand") },
                    onclick: move |_| expanded.set(!expanded()),
                }
            }
            div { class: if expanded() {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "min-h-0 overflow-hidden",
                    div { class: "grid grid-cols-1 gap-px px-1 pb-1 sm:grid-cols-2 lg:grid-cols-3",
                        for entry in visible {
                            ProjectFileButton {
                                key: "file-{entry.path}",
                                path: entry.path,
                                name: entry.name,
                                context: String::new(),
                                is_dir: entry.is_dir,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectFileButton(path: String, name: String, context: String, is_dir: bool) -> Element {
    let open_path = path.clone();
    let icon_path = path.clone();
    rsx! {
        button {
            r#type: "button",
            class: "group flex min-w-0 items-center gap-2 rounded-md px-2.5 py-2 text-left hover:bg-foreground/[0.04]",
            title: "{path}",
            onclick: move |_| {
                let _ = send(&FileDirectoryOpenRequest { path: open_path.clone() });
            },
            TypeIcon { path: icon_path, is_dir, class: "size-4 shrink-0".to_string() }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-[11px] text-foreground/85", "{name}" }
                if !context.is_empty() {
                    div { class: "truncate text-[9px] text-muted-foreground", "{context}" }
                }
            }
        }
    }
}

#[component]
fn ProjectSection(title: String, count: usize, children: Element) -> Element {
    rsx! {
        section { class: "overflow-hidden rounded-lg bg-foreground/[0.025]",
            header { class: "flex h-10 items-center gap-2 px-3",
                h2 { class: "truncate text-xs font-semibold", "{title}" }
                if count > 0 {
                    span { class: "text-[10px] tabular-nums text-muted-foreground", "{count}" }
                }
            }
            {children}
        }
    }
}

struct ProjectPath;

impl ProjectPath {
    fn clean(path: &str) -> &str {
        path.trim_end_matches('/')
    }

    fn same(left: &str, right: &str) -> bool {
        Self::clean(left) == Self::clean(right)
    }

    fn contains(root: &str, path: &str) -> bool {
        let root = Self::clean(root);
        let path = Self::clean(path);
        if root.is_empty() || path.is_empty() {
            return false;
        }
        path == root
            || path
                .strip_prefix(root)
                .is_some_and(|relative| relative.starts_with('/'))
    }

    fn name(path: &str) -> String {
        Self::clean(path)
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(path)
            .to_string()
    }
}

struct ProjectSession;

impl ProjectSession {
    fn title(session: &SessionSummary) -> String {
        for title in [
            &session.conversation_title,
            &session.name,
            &session.last_prompt,
        ] {
            if !title.trim().is_empty() {
                return title.clone();
            }
        }
        session.sid.clone()
    }

    fn preview(session: &SessionSummary) -> String {
        let title = Self::title(session);
        for preview in [
            &session.last_prompt,
            &session.last_message,
            &session.description,
        ] {
            if !preview.trim().is_empty() && preview.as_str() != title {
                return preview.clone();
            }
        }
        String::new()
    }

    fn stage(session: &SessionSummary) -> String {
        let stale = session.stage != "done"
            && session.runtime == "inactive"
            && ProjectTime::from_millis(session.last_activated_at).0 >= 3 * ProjectTime::DAY;
        if stale {
            return translate_humanized("stale");
        }
        translate_humanized(&session.stage)
    }
}

struct ProjectTime(u64);

impl ProjectTime {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * Self::MINUTE;
    const DAY: u64 = 24 * Self::HOUR;
    const WEEK: u64 = 7 * Self::DAY;

    fn from_millis(timestamp: i64) -> Self {
        if timestamp <= 0 {
            return Self(0);
        }
        let now = vmux_ecs::UnixMillis::now().0;
        Self(now.saturating_sub(timestamp).max(0) as u64 / 1_000)
    }

    fn label(self) -> String {
        if self.0 < Self::MINUTE {
            return translate("resume-age-now");
        }
        if self.0 < Self::HOUR {
            return format!("{}m", self.0 / Self::MINUTE);
        }
        if self.0 < Self::DAY {
            return format!("{}h", self.0 / Self::HOUR);
        }
        if self.0 < Self::WEEK {
            return format!("{}d", self.0 / Self::DAY);
        }
        format!("{}w", self.0 / Self::WEEK)
    }
}
