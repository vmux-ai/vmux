use std::path::Path;

use dioxus::prelude::*;
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;
use vmux_ecs::event::{FileDirEntry, FileDirectoryOpenRequest, OpenEditorItem};
use vmux_git::event::GitRepositoryRequest;
use vmux_git::state::GitUiState;
use vmux_session::event::ResumeSession;
use vmux_session::state::SessionScopeUiState;
use vmux_session::{SessionAge, SessionSummary, SessionSummaryProjection, StageSummary};
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerCard, ManagerCollection, ManagerControls, ManagerEmpty, ManagerList,
    ManagerPage, ManagerRow, ManagerSearch, ManagerSkeleton, ManagerTone, ManagerToolbar,
    ManagerView, ManagerViewToggle,
};
use vmux_ui::components::page::{PageContent, PageHeader, PageShell};
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::i18n::{translate, translate_humanized};
use vmux_ui::icon::{BuiltinIconView, GitSymbol, GitSymbolView, LineIcon, LineIconView};

use super::state::FileUi;
use crate::project::{
    ProjectCatalogView, ProjectCloneRequest, ProjectCreateRequest, ProjectEntry,
    ProjectImportRequest, ProjectUiState, ProjectView,
};

#[component]
pub(super) fn ProjectPage() -> Element {
    let state = use_ui_state::<ProjectUiState>().state;
    let snapshot = state();
    match snapshot.view {
        ProjectView::Detail => rsx! { ProjectDashboard {} },
        ProjectView::Loading => rsx! { ProjectList { view: None } },
        ProjectView::Catalog(view) => rsx! { ProjectList { view: Some(view) } },
        ProjectView::Error(error) => rsx! { ProjectList { error } },
    }
}

#[component]
pub(super) fn ProjectList(
    #[props(default)] view: Option<ProjectCatalogView>,
    #[props(default)] error: String,
) -> Element {
    let entries = view
        .as_ref()
        .map(|view| view.entries.clone())
        .unwrap_or_default();
    let count = entries.len();
    let pending = view.as_ref().is_some_and(|view| view.operation_pending);
    let mut query = use_signal(String::new);
    let mut manager_view = use_signal(ManagerView::default);
    let needle = query().trim().to_ascii_lowercase();
    let visible = entries
        .iter()
        .filter(|entry| entry.matches(&needle))
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        ManagerPage {
            ManagerToolbar {
                div { class: "flex shrink-0 items-center gap-2 px-1",
                    BuiltinIconView {
                        icon: vmux_api::BuiltinIcon::Project,
                        class: "size-4 text-muted-foreground".to_string(),
                    }
                    span { class: "hidden text-xs font-semibold text-foreground sm:inline", {translate("layout-projects")} }
                }
                ManagerSearch {
                    value: query(),
                    placeholder: translate("layout-projects"),
                    onsearch: move |event: FormEvent| query.set(event.value()),
                    onkeydown: None,
                }
                ProjectAdd { pending }
            }
            div { class: "flex min-h-0 flex-1 overflow-hidden bg-background/45",
                ManagerList {
                    if count > 0 {
                        ManagerControls {
                            ManagerViewToggle {
                                view: manager_view(),
                                list_label: translate("tools-view-list"),
                                grid_label: translate("tools-view-grid"),
                                onchange: move |view| manager_view.set(view),
                            }
                        }
                    }
                    if view.is_none() && error.is_empty() {
                        ManagerSkeleton {}
                    } else if !error.is_empty() {
                        ManagerEmpty { title: error, detail: String::new() }
                    } else if entries.is_empty() {
                        ManagerEmpty {
                            title: translate("agent-project-none"),
                            detail: translate("project-empty-description"),
                            actions: rsx! { ProjectAdd { pending } },
                        }
                    } else if visible.is_empty() {
                        ManagerEmpty {
                            title: translate("agent-project-none"),
                            detail: query(),
                        }
                    } else {
                        ManagerCollection { view: manager_view(),
                            for entry in visible {
                                ProjectItem {
                                    key: "project-{entry.path}",
                                    entry: entry.clone(),
                                    view: manager_view(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl ProjectEntry {
    fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.name.to_ascii_lowercase().contains(needle)
            || self.source.to_ascii_lowercase().contains(needle)
            || self.path.to_ascii_lowercase().contains(needle)
            || self.branch.to_ascii_lowercase().contains(needle)
    }
}

#[component]
fn ProjectItem(entry: ProjectEntry, view: ManagerView) -> Element {
    let url = entry.url.clone();
    let select = move |_| ProjectEntry::open(url.clone());
    let icon = rsx! {
        TypeIcon { path: entry.path.clone(), is_dir: true, class: "size-5".to_string() }
    };
    let meta = rsx! { ProjectMeta { entry: entry.clone() } };
    if view == ManagerView::Grid {
        return rsx! {
            ManagerCard {
                selected: false,
                onselect: select,
                icon,
                title: entry.name,
                subtitle: entry.source,
                meta,
                actions: rsx! {},
            }
        };
    }
    rsx! {
        ManagerRow {
            selected: false,
            onselect: select,
            icon,
            title: entry.name,
            subtitle: entry.source,
            meta,
            actions: rsx! {},
        }
    }
}

#[component]
fn ProjectMeta(entry: ProjectEntry) -> Element {
    let changes = format!("{} {}", entry.uncommitted, translate("git-changes"));
    let sessions = format!("{} {}", entry.sessions, translate("sessions-title"));
    rsx! {
        if entry.uncommitted > 0 {
            ManagerBadge { tone: ManagerTone::Amber,
                span { "{changes}" }
            }
        }
        if entry.ahead > 0 {
            ManagerBadge { tone: ManagerTone::Green,
                span { title: translate("composer-commits-ahead"), "↑{entry.ahead}" }
            }
        }
        if entry.sessions > 0 {
            ManagerBadge { tone: ManagerTone::Primary,
                span { "{sessions}" }
            }
        }
    }
}

impl ProjectEntry {
    fn open(url: String) {
        let _ = send(&OpenRequest {
            value: url,
            open: Some(OpenTarget::InPlace),
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ProjectPrompt {
    New,
    Clone,
}

#[component]
fn ProjectAdd(pending: bool) -> Element {
    let mut open = use_signal(|| false);
    let mut prompt = use_signal(|| None::<ProjectPrompt>);
    let mut draft = use_signal(String::new);
    let ready = !draft().trim().is_empty();
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Sm,
                class: "h-8 gap-1.5 text-xs".to_string(),
                disabled: pending,
                onclick: move |_| open.set(!open()),
                LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                span { {translate("project-add")} }
            }
            if open() {
                div { class: "absolute right-0 top-9 z-50 w-72 rounded-lg bg-card p-1.5 shadow-[0_16px_48px_rgba(0,0,0,0.28)]",
                    ProjectAddItem {
                        icon: LineIcon::Plus,
                        title: translate("project-new"),
                        description: translate("project-new-description"),
                        onclick: move |_| {
                            open.set(false);
                            draft.set(String::new());
                            prompt.set(Some(ProjectPrompt::New));
                        },
                    }
                    ProjectAddItem {
                        icon: LineIcon::FileOutput,
                        title: translate("project-import"),
                        description: translate("project-import-description"),
                        onclick: move |_| {
                            open.set(false);
                            let _ = send(&ProjectImportRequest);
                        },
                    }
                    ProjectAddItem {
                        icon: LineIcon::GitFork,
                        title: translate("project-clone"),
                        description: translate("project-clone-description"),
                        onclick: move |_| {
                            open.set(false);
                            draft.set(String::new());
                            prompt.set(Some(ProjectPrompt::Clone));
                        },
                    }
                }
            }
        }
        if let Some(kind) = prompt() {
            DialogRoot {
                open: true,
                on_open_change: move |next: bool| {
                    if !next {
                        prompt.set(None);
                    }
                },
                attributes: vec![],
                DialogContent { class: "max-w-[420px] p-4", attributes: vec![],
                    DialogTitle { attributes: vec![],
                        {translate(if kind == ProjectPrompt::New { "project-new" } else { "project-clone" })}
                    }
                    input {
                        class: "w-full rounded-md bg-foreground/[0.045] px-3 py-2 text-sm text-foreground outline-none focus:bg-foreground/[0.065]",
                        autofocus: true,
                        value: "{draft}",
                        placeholder: translate(if kind == ProjectPrompt::New { "project-name-placeholder" } else { "project-repository-placeholder" }),
                        oninput: move |event: Event<FormData>| draft.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            event.stop_propagation();
                            if event.key() == Key::Enter && ready {
                                event.prevent_default();
                                ProjectPrompt::submit(kind, draft());
                                prompt.set(None);
                            }
                        },
                    }
                    div { class: "flex justify-end gap-2",
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Xs,
                            onclick: move |_| prompt.set(None),
                            {translate("common-cancel")}
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Xs,
                            disabled: !ready,
                            onclick: move |_| {
                                ProjectPrompt::submit(kind, draft());
                                prompt.set(None);
                            },
                            {translate(if kind == ProjectPrompt::New { "project-create" } else { "project-clone" })}
                        }
                    }
                }
            }
        }
    }
}

impl ProjectPrompt {
    fn submit(self, draft: String) {
        match self {
            Self::New => {
                let _ = send(&ProjectCreateRequest { name: draft });
            }
            Self::Clone => {
                let _ = send(&ProjectCloneRequest { repository: draft });
            }
        }
    }
}

#[component]
fn ProjectAddItem(
    icon: LineIcon,
    title: String,
    description: String,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-start gap-3 rounded-md px-2.5 py-2.5 text-left hover:bg-foreground/[0.055]",
            onclick: move |_| onclick.call(()),
            LineIconView { icon, class: "mt-0.5 size-4 shrink-0 text-muted-foreground".to_string() }
            div { class: "min-w-0 flex-1",
                div { class: "text-xs font-medium text-foreground", "{title}" }
                div { class: "mt-0.5 text-[10px] leading-4 text-muted-foreground", "{description}" }
            }
        }
    }
}

#[component]
pub(super) fn ProjectDashboard() -> Element {
    let files = FileUi::use_root();
    let directory = files.use_value(|state| state.document.directory.as_ref());
    let root = use_memo(move || {
        directory
            .read()
            .as_ref()
            .map(|directory| directory.abs_path.clone())
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
    let start_url = "vmux://start/".to_string();

    rsx! {
        PageShell {
            PageHeader {
                div { class: "flex min-h-10 items-center gap-3",
                div { class: "flex size-9 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.055]",
                    TypeIcon { path: root(), is_dir: true, class: "size-5".to_string() }
                }
                h1 { class: "min-w-0 flex-1 truncate text-sm font-semibold tracking-[-0.015em]", title: "{root}", "{project_name}" }
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
            }
            PageContent { content_class: "mx-auto flex w-full max-w-5xl flex-col gap-5".to_string(),
                ProjectSessionsPanel {}
                ProjectGitPanel { root: root() }
                ProjectRecentFilesPanel { root: root() }
                ProjectFilesPanel {}
            }
        }
    }
}

#[component]
fn ProjectSessionsPanel() -> Element {
    let sessions = use_ui_state::<SessionScopeUiState>();
    let projection = sessions.use_selector(|state| ProjectSessionList {
        sessions: state.sessions.iter().take(8).cloned().collect(),
        stages: state.stages.clone(),
    });
    rsx! { ProjectSessions { projection: projection() } }
}

#[derive(Clone, Default, PartialEq)]
struct ProjectSessionList {
    sessions: Vec<SessionSummary>,
    stages: Vec<StageSummary>,
}

#[component]
fn ProjectSessions(projection: ProjectSessionList) -> Element {
    rsx! {
        ProjectSection { title: translate("sessions-title"), count: projection.sessions.len(),
            if projection.sessions.is_empty() {
                div { class: "flex min-h-20 items-center justify-center px-3 text-xs text-muted-foreground",
                    {translate("sessions-inbox-empty")}
                }
            } else {
                div { class: "divide-y divide-foreground/[0.055]",
                    for session in projection.sessions {
                        ProjectSessionRow {
                            key: "{session.id}",
                            session,
                            stages: projection.stages.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectSessionRow(session: SessionSummary, stages: Vec<StageSummary>) -> Element {
    let projection =
        SessionSummaryProjection::new(&session, &stages, vmux_ecs::UnixMillis::now().0);
    let stage = if projection.stale {
        translate_humanized("stale")
    } else {
        stages
            .iter()
            .find(|stage| stage.id == session.stage)
            .map(|stage| translate_humanized(&stage.name))
            .unwrap_or_else(|| translate_humanized(&session.stage))
    };
    let age = ProjectTime::label(projection.age);
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
                div { class: "truncate text-xs font-medium text-foreground/90", "{projection.title}" }
                if !projection.preview.is_empty() {
                    div { class: "truncate text-[10px] text-muted-foreground/70", "{projection.preview}" }
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
fn ProjectGitPanel(root: String) -> Element {
    let git = use_ui_state::<GitUiState>();
    let reactive_root = root.clone();
    let selected_root = use_memo(use_reactive!(|(reactive_root,)| reactive_root.clone()));
    let projection = git.use_selector(move |state| ProjectGitProjection {
        repository: state
            .snapshot
            .repository
            .as_ref()
            .filter(|repository| ProjectPath::same(&repository.repo_root, &selected_root()))
            .cloned(),
        loading: state.snapshot.loading && state.snapshot.workspace == selected_root(),
    });
    rsx! {
        ProjectGit {
            root,
            repository: projection.read().repository.clone(),
            loading: projection.read().loading,
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct ProjectGitProjection {
    repository: Option<vmux_git::event::GitRepositorySnapshot>,
    loading: bool,
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
fn ProjectRecentFilesPanel(root: String) -> Element {
    let files = FileUi::current();
    let open_editors = files.use_value(|state| state.explorer.open_editors.as_ref());
    let recent = use_memo(use_reactive!(|(root,)| {
        open_editors
            .read()
            .as_ref()
            .map(|editors| {
                editors
                    .items
                    .iter()
                    .filter(|item| !item.is_dir && ProjectPath::contains(&root, &item.path))
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }));
    if recent.read().is_empty() {
        return rsx! {};
    }
    rsx! { ProjectRecentFiles { files: recent() } }
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
fn ProjectFilesPanel() -> Element {
    let files = FileUi::current();
    let directory = files.use_value(|state| state.document.directory.as_ref());
    let entries = use_memo(move || {
        directory
            .read()
            .as_ref()
            .map(|directory| directory.entries.clone())
            .unwrap_or_default()
    });
    rsx! { ProjectFiles { entries: entries() } }
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

struct ProjectTime;

impl ProjectTime {
    fn label(age: SessionAge) -> String {
        if age.0 < SessionAge::MINUTE {
            return translate("resume-age-now");
        }
        if age.0 < SessionAge::HOUR {
            return format!("{}m", age.0 / SessionAge::MINUTE);
        }
        if age.0 < SessionAge::DAY {
            return format!("{}h", age.0 / SessionAge::HOUR);
        }
        if age.0 < SessionAge::WEEK {
            return format!("{}d", age.0 / SessionAge::DAY);
        }
        format!("{}w", age.0 / SessionAge::WEEK)
    }
}
