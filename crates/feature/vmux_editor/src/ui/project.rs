use std::path::Path;

use dioxus::prelude::*;
use vmux_ecs::event::{FileDirEntry, FileDirectoryOpenRequest, OpenEditorItem};
use vmux_git::event::GitRepositoryRequest;
use vmux_git::state::GitUiState;
use vmux_session::state::SessionScopeUiState;
use vmux_session::{SessionSummary, SessionSummaryProjection, StageSummary};
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::avatar::Identicon;
use vmux_ui::components::breadcrumb::{BreadcrumbItem, BreadcrumbSeparator, Breadcrumbs};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use vmux_ui::components::filter::{FilterPill, FilterTrigger};
use vmux_ui::components::manager::{
    ManagerArea, ManagerBadge, ManagerCard, ManagerCollection, ManagerControls, ManagerEmpty,
    ManagerGridSize, ManagerIconButton, ManagerList, ManagerPage, ManagerRow, ManagerSearch,
    ManagerSearchPopover, ManagerSidebarItem, ManagerSkeleton, ManagerTone, ManagerView,
    ManagerViewToggle, WorkspaceLayout,
};
use vmux_ui::components::page::{PageContent, PageHeader};
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::tooltip::Tooltip;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{CollectionController, send, use_collection_controller, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};
use vmux_ui::navigation::PageNavigation;

use super::project_manager::{
    ProjectFilter, ProjectFilterMenu, ProjectPresentation, ViewPreference,
};
use super::state::FileUi;
use crate::project::{
    ProjectArchiveRequest, ProjectCatalogView, ProjectCloneRequest, ProjectCreateRequest,
    ProjectDeleteRequest, ProjectDetailView, ProjectEntry, ProjectImportRequest, ProjectUiState,
    ProjectView,
};

#[component]
pub(super) fn ProjectPage() -> Element {
    let state = use_ui_state::<ProjectUiState>().state;
    let snapshot = state();
    match snapshot.view {
        ProjectView::Detail(detail) => rsx! { ProjectDashboard { detail } },
        ProjectView::Loading => {
            rsx! { ProjectList { view: None, collection_view: snapshot.collection_view } }
        }
        ProjectView::Catalog(view) => {
            rsx! { ProjectList { view: Some(view), collection_view: snapshot.collection_view } }
        }
        ProjectView::Error(error) => {
            rsx! { ProjectList { error, collection_view: snapshot.collection_view } }
        }
    }
}

#[component]
pub(super) fn ProjectList(
    #[props(default)] view: Option<ProjectCatalogView>,
    #[props(default)] error: String,
    collection_view: vmux_ecs::CollectionView,
) -> Element {
    let entries = view
        .as_ref()
        .map(|view| view.entries.clone())
        .unwrap_or_default();
    let count = entries.len();
    let pending = view.as_ref().is_some_and(|view| view.operation_pending);
    let controls =
        use_collection_controller::<ProjectFilter>(ViewPreference::manager(collection_view));
    let filters = controls.filters();
    let presentation = ProjectPresentation::new(&entries, &controls.query(), &filters);
    rsx! {
        ManagerPage {
            WorkspaceLayout {
                active: ProjectFilter::selected(&filters).is_none().then_some(ManagerArea::Projects),
                onnavigate: move |area: ManagerArea| {
                    if area == ManagerArea::Projects {
                        controls.replace_filters(Vec::new());
                        return;
                    }
                    PageNavigation::replace(area.url());
                },
                sidebar_actions: rsx! {
                    ProjectManagerActions {
                        entries: entries.clone(),
                        presentation: presentation.clone(),
                        controls,
                        pending,
                        align_left: true,
                    }
                },
                sidebar: rsx! { ProjectCategories { entries: entries.clone(), controls } },
                ManagerList {
                    if count > 0 {
                        ManagerControls {
                            ManagerViewToggle {
                                view: controls.view(),
                                list_label: translate("tools-view-list"),
                                grid_label: translate("tools-view-grid"),
                                onchange: move |view| {
                                    controls.set_view(view);
                                    let _ = send(&ViewPreference::request(view));
                                },
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
                    } else if presentation.entries.is_empty() {
                        ManagerEmpty {
                            title: translate("agent-project-none"),
                            detail: controls.query(),
                        }
                    } else {
                        ManagerCollection { view: controls.view(), grid_size: ManagerGridSize::Comfortable,
                            for entry in presentation.entries {
                                ProjectItem {
                                    key: "project-{entry.path}",
                                    entry: entry.clone(),
                                    view: controls.view(),
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
fn ProjectManagerActions(
    entries: Vec<ProjectEntry>,
    presentation: ProjectPresentation,
    controls: CollectionController<ProjectFilter>,
    pending: bool,
    align_left: bool,
) -> Element {
    let mut search_open = use_signal(|| false);
    let open = search_open();
    let active = !controls.query().is_empty() || !presentation.selected_filters.is_empty();
    rsx! {
        div { class: "flex shrink-0 items-center gap-1",
            ManagerSearchPopover {
                open,
                active,
                label: translate("layout-projects"),
                align_left,
                ontoggle: move |_| {
                    search_open.set(!open);
                    if open && controls.query().starts_with('#') {
                        controls.dismiss_filter();
                    }
                },
                ondismiss: move |_| {
                    search_open.set(false);
                    if controls.query().starts_with('#') {
                        controls.dismiss_filter();
                    }
                },
                ManagerSearch {
                    value: controls.query(),
                    placeholder: translate("layout-projects"),
                    autofocus: true,
                    onsearch: move |query: String| controls.set_query(query),
                    onkeydown: Some(EventHandler::new(move |event: KeyboardEvent| {
                        if event.key() == Key::Backspace && controls.query().is_empty() {
                            controls.pop_filter();
                        }
                    })),
                    prefix: rsx! {
                        for (index, filter) in presentation.selected_filters.iter().cloned().enumerate() {
                            FilterPill {
                                key: "project-filter-pill-{index}",
                                label: filter.label,
                                dot: filter.dot,
                                clear_label: translate("common-remove-filter"),
                                onclear: move |_| controls.remove_filter(index),
                            }
                        }
                    },
                    suffix: rsx! {
                        FilterTrigger {
                            active: presentation.filter_requested,
                            label: translate("common-filter"),
                            onclick: move |_| controls.toggle_filter(),
                        }
                    },
                }
                if presentation.filter_requested {
                    ProjectFilterMenu {
                        entries,
                        needle: presentation.filter_needle,
                        onselect: move |filter| {
                            controls.update_filters(|filters| {
                                ProjectFilter::select(filters, Some(filter));
                            });
                            controls.dismiss_filter();
                        },
                    }
                }
            }
            ProjectAdd { pending, compact: true }
        }
    }
}

#[component]
fn ProjectCategories(
    entries: Vec<ProjectEntry>,
    controls: CollectionController<ProjectFilter>,
) -> Element {
    let selected = ProjectFilter::selected(&controls.filters());
    let filter = ProjectFilter::Archived;
    rsx! {
        if filter.count(&entries) > 0 {
            ManagerSidebarItem {
                label: filter.label(),
                count: filter.count(&entries),
                selected: selected == Some(filter),
                icon: LineIcon::Archive,
                onclick: move |_| {
                    controls.update_filters(|filters| {
                        ProjectFilter::select(filters, Some(filter));
                    });
                },
            }
        }
    }
}

#[component]
fn ProjectItem(entry: ProjectEntry, view: ManagerView) -> Element {
    let url = entry.url.clone();
    let select = move |_| ProjectEntry::open(url.clone());
    let icon = rsx! {
        Identicon {
            seed: entry.identity_seed().to_string(),
            alt: entry.name.clone(),
            class: "size-6 shrink-0 rounded-lg".to_string(),
        }
    };
    let meta = rsx! { ProjectMeta { entry: entry.clone() } };
    let actions = rsx! { ProjectActions { entry: entry.clone() } };
    if view == ManagerView::Grid {
        return rsx! {
            ManagerCard {
                selected: false,
                onselect: select,
                icon,
                title: entry.name,
                subtitle: entry.source,
                meta,
                actions,
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
            actions,
        }
    }
}

#[component]
fn ProjectMeta(entry: ProjectEntry) -> Element {
    let ongoing = format!(
        "{} {}",
        entry.ongoing_sessions,
        translate("session-stage-in-progress")
    );
    let stale = format!("{} {}", entry.stale_sessions, translate("sessions-stale"));
    rsx! {
        if entry.archived {
            ManagerBadge { tone: ManagerTone::Neutral,
                uppercase: false,
                span { {translate("project-archived")} }
            }
        }
        if entry.ongoing_sessions > 0 {
            ManagerBadge { tone: ManagerTone::Green,
                uppercase: false,
                span { "{ongoing}" }
            }
        }
        if entry.stale_sessions > 0 {
            ManagerBadge { tone: ManagerTone::Amber,
                uppercase: false,
                span { "{stale}" }
            }
        }
    }
}

#[component]
fn ProjectActions(entry: ProjectEntry) -> Element {
    let mut open = use_signal(|| false);
    let mut confirm_open = use_signal(|| false);
    let archived = entry.archived;
    let archive_label = if archived {
        translate("project-restore")
    } else {
        translate("common-archive")
    };
    let resume_url = entry.url.clone();
    let archive_url = entry.url.clone();
    let delete_url = entry.url.clone();
    let name = entry.name.clone();
    let confirm = use_memo(move || Some(confirm_open()));
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            Tooltip { label: translate("sessions-actions"),
                button {
                    r#type: "button",
                    class: if open() {
                        "flex size-7 items-center justify-center rounded-md bg-foreground/[0.08] text-foreground"
                    } else {
                        "flex size-7 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-all hover:bg-foreground/[0.08] hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100"
                    },
                    aria_label: translate("sessions-actions"),
                    onclick: move |event| {
                        event.stop_propagation();
                        open.set(!open());
                    },
                    LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5".to_string() }
                }
            }
            if open() {
                DropdownPopup {
                    align: DropdownAlign::End,
                    class: "w-40 p-1",
                    onclick: move |event: MouseEvent| event.stop_propagation(),
                    ProjectActionMenuItem {
                        icon: LineIcon::MessagesRound,
                        label: translate("sessions-resume"),
                        danger: false,
                        onclick: move |_| {
                            open.set(false);
                            ProjectEntry::open(resume_url.clone());
                        },
                    }
                    ProjectActionMenuItem {
                        icon: if archived { LineIcon::RotateCcw } else { LineIcon::Archive },
                        label: archive_label.clone(),
                        danger: false,
                        onclick: move |_| {
                            open.set(false);
                            let _ = send(&ProjectArchiveRequest {
                                url: archive_url.clone(),
                                archived: !archived,
                            });
                        },
                    }
                    ProjectActionMenuItem {
                        icon: LineIcon::Trash,
                        label: translate("common-delete"),
                        danger: true,
                        onclick: move |_| {
                            open.set(false);
                            confirm_open.set(true);
                        },
                    }
                }
            }
        }
        AlertDialogRoot {
            open: Into::<ReadSignal<Option<bool>>>::into(confirm),
            on_open_change: Callback::new(move |next| confirm_open.set(next)),
            default_open: false,
            attributes: vec![],
            AlertDialogContent { attributes: vec![],
                AlertDialogTitle { attributes: vec![], {translate("common-delete")} }
                AlertDialogDescription { attributes: vec![],
                    {translate_with(
                        "editor-delete-confirm",
                        &[("name", TranslationValue::String(&name))],
                    )}
                }
                AlertDialogActions { attributes: vec![],
                    AlertDialogCancel {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |event: MouseEvent| {
                            event.stop_propagation();
                            confirm_open.set(false);
                        })),
                        {translate("common-cancel")}
                    }
                    AlertDialogAction {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |event: MouseEvent| {
                            event.stop_propagation();
                            confirm_open.set(false);
                            let _ = send(&ProjectDeleteRequest { url: delete_url.clone() });
                        })),
                        {translate("common-delete")}
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectActionMenuItem(
    icon: LineIcon,
    label: String,
    danger: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if danger {
        "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-ansi-1 hover:bg-ansi-1/10"
    } else {
        "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-[11px] text-foreground/80 hover:bg-foreground/[0.06] hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            class,
            onclick: move |event| onclick.call(event),
            LineIconView { icon, class: "size-3.5 shrink-0".to_string() }
            span { class: "truncate", "{label}" }
        }
    }
}

impl ProjectEntry {
    fn open(url: String) {
        PageNavigation::replace(url);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ProjectPrompt {
    New,
    Clone,
}

#[component]
fn ProjectAdd(pending: bool, #[props(default)] compact: bool) -> Element {
    let mut open = use_signal(|| false);
    let mut prompt = use_signal(|| None::<ProjectPrompt>);
    let mut draft = use_signal(String::new);
    let ready = !draft().trim().is_empty();
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            if compact {
                ManagerIconButton {
                    label: translate("common-add"),
                    disabled: pending,
                    active: open(),
                    onclick: move |_| open.set(!open()),
                    LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                }
            } else {
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Sm,
                    class: "h-8 gap-1.5 text-xs".to_string(),
                    disabled: pending,
                    onclick: move |_| open.set(!open()),
                    LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                    span { {translate("common-add")} }
                }
            }
            if open() {
                DropdownPopup { align: DropdownAlign::End, class: "w-72 p-1.5",
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
pub(super) fn ProjectDashboard(detail: ProjectDetailView) -> Element {
    let root = detail.path;
    let project_name = detail.name;
    let project_identity = detail.id.0;
    let mut requested_git = use_signal(String::new);
    let requested_root = root.clone();
    use_effect(move || {
        if requested_root.is_empty() || requested_git() == requested_root {
            return;
        }
        requested_git.set(requested_root.clone());
        let _ = send(&GitRepositoryRequest {
            path: requested_root.clone(),
        });
    });
    let start_url = "vmux://start/".to_string();
    let header_root = root.clone();

    rsx! {
        ManagerPage {
            WorkspaceLayout {
                active: Some(ManagerArea::Projects),
                onnavigate: move |area: ManagerArea| {
                    PageNavigation::replace(area.url());
                },
                PageHeader {
                    div { class: "flex min-h-10 items-center gap-2",
                        Identicon {
                            seed: project_identity,
                            alt: project_name.clone(),
                            class: "size-7 shrink-0 rounded-lg".to_string(),
                        }
                        Breadcrumbs { class: "min-w-0 flex-1".to_string(),
                            BreadcrumbItem {
                                label: translate("layout-projects"),
                                onclick: move |_| PageNavigation::replace("vmux://projects/"),
                            }
                            BreadcrumbSeparator {}
                            BreadcrumbItem {
                                label: project_name.clone(),
                                title: header_root,
                                active: true,
                            }
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Sm,
                            class: "h-8 gap-1.5 text-xs".to_string(),
                            onclick: move |_| PageNavigation::replace(start_url.clone()),
                            LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                            span { {translate("sessions-start")} }
                        }
                    }
                }
                PageContent { content_class: "mx-auto flex w-full max-w-5xl flex-col gap-5".to_string(),
                    ProjectSessionsPanel {}
                    ProjectGitPanel { root: root.clone() }
                    ProjectRecentFilesPanel { root: root.clone() }
                    ProjectFilesPanel {}
                }
            }
        }
    }
}

#[component]
fn ProjectSessionsPanel() -> Element {
    let sessions = use_ui_state::<SessionScopeUiState>();
    let projection = sessions.use_selector(|state| {
        let now = vmux_ecs::UnixMillis::now().0;
        ProjectSessionList {
            sessions: state
                .sessions
                .iter()
                .filter(|session| {
                    !SessionSummaryProjection::new(session, &state.stages, now).terminal
                })
                .cloned()
                .collect(),
            stages: state.stages.clone(),
        }
    });
    rsx! { ProjectSessions { projection: projection() } }
}

#[derive(Clone, Default, PartialEq)]
struct ProjectSessionList {
    sessions: Vec<SessionSummary>,
    stages: Vec<StageSummary>,
}

impl ProjectSessionList {
    fn visible_len(&self) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.archived_at == 0)
            .count()
    }
}

#[component]
fn ProjectSessions(projection: ProjectSessionList) -> Element {
    let mut view = use_signal(ManagerView::default);
    let visible = projection.visible_len();
    rsx! {
        ProjectSection {
            title: translate("sessions-title"),
            count: visible,
            actions: rsx! {
                if visible > 0 {
                    ManagerViewToggle {
                        view: view(),
                        list_label: translate("tools-view-list"),
                        grid_label: translate("tools-view-grid"),
                        onchange: move |next| view.set(next),
                    }
                }
            },
            if visible == 0 {
                div { class: "flex min-h-20 items-center justify-center px-3 text-xs text-muted-foreground",
                    {translate("sessions-inbox-empty")}
                }
            } else {
                vmux_session::ui::SessionCollection {
                    sessions: projection.sessions,
                    stages: projection.stages,
                    view: view(),
                }
            }
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
        loading: !selected_root().is_empty()
            && (state.snapshot.workspace != selected_root() || state.snapshot.loading),
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
                            PageNavigation::replace(url);
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
    let projection = use_memo(move || {
        let directory = directory.read();
        ProjectFilesProjection {
            loaded: directory.is_some(),
            entries: directory
                .as_ref()
                .map(|directory| directory.entries.clone())
                .unwrap_or_default(),
        }
    });
    rsx! { ProjectFiles { projection: projection() } }
}

#[derive(Clone, Default, PartialEq)]
struct ProjectFilesProjection {
    loaded: bool,
    entries: Vec<FileDirEntry>,
}

#[component]
fn ProjectFiles(projection: ProjectFilesProjection) -> Element {
    let mut expanded = use_signal(|| true);
    let visible = projection
        .entries
        .iter()
        .take(18)
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        section { class: "overflow-hidden rounded-lg",
            div { class: "flex h-10 items-center gap-2 px-3",
                div { class: "flex min-w-0 flex-1 items-center gap-2",
                    LineIconView { icon: LineIcon::File, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                    h2 { class: "truncate text-xs font-semibold", {translate("git-files")} }
                    if projection.loaded {
                        span { class: "text-[10px] tabular-nums text-muted-foreground", "{projection.entries.len()}" }
                    }
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
                    if projection.loaded {
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
                    } else {
                        div { class: "grid grid-cols-1 gap-2 px-3 pb-3 sm:grid-cols-2 lg:grid-cols-3",
                            for index in 0..6 {
                                Skeleton { key: "project-file-skeleton-{index}", class: "h-8 rounded-md bg-foreground/[0.045]" }
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
fn ProjectSection(
    title: String,
    count: usize,
    #[props(default = rsx! {})] actions: Element,
    children: Element,
) -> Element {
    rsx! {
        section { class: "overflow-hidden rounded-lg",
            header { class: "flex h-10 items-center gap-2 px-3",
                h2 { class: "truncate text-xs font-semibold", "{title}" }
                if count > 0 {
                    span { class: "text-[10px] tabular-nums text-muted-foreground", "{count}" }
                }
                div { class: "flex-1" }
                {actions}
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
}
