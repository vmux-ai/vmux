use std::path::Path;

use dioxus::prelude::*;
use vmux_ecs::event::FileDirEntry;
use vmux_git::event::GitRepositoryRequest;
use vmux_git::state::GitUiState;
use vmux_session::state::SessionScopeUiState;
use vmux_session::{SessionSummary, SessionSummaryProjection, StageSummary};
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::avatar::IdentityOrb;
use vmux_ui::components::breadcrumb::{BreadcrumbItem, BreadcrumbSeparator};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use vmux_ui::components::filter::{FilterPill, FilterTrigger};
use vmux_ui::components::manager::{
    ManagerAddButton, ManagerCollection, ManagerEmpty, ManagerExpandableSearch, ManagerGridSize,
    ManagerList, ManagerPage, ManagerSearch, ManagerSidebarItem, ManagerSkeleton, ManagerView,
    ManagerViewToggle, WorkspaceLayout, WorkspacePage,
};
use vmux_ui::components::page::PageContent;
use vmux_ui::components::page_address::PageAddress;
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::tooltip::Tooltip;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{CollectionController, send, use_collection_controller, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};
use vmux_ui::navigation::{NavigationGesture, PageNavigation};

use super::project_manager::{
    ProjectFilter, ProjectFilterMenu, ProjectPresentation, ViewPreference,
};
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
    let pending = view.as_ref().is_some_and(|view| view.operation_pending);
    let controls =
        use_collection_controller::<ProjectFilter>(ViewPreference::manager(collection_view));
    let filters = controls.filters();
    let presentation = ProjectPresentation::new(&entries, &controls.query(), &filters);
    rsx! {
        ManagerPage {
            WorkspaceLayout {
                show_sidebar: entries.iter().any(|entry| entry.archived),
                header: rsx! {
                    PageAddress {
                        current: WorkspacePage::Projects.option(),
                        options: WorkspacePage::options(),
                        view_controls: rsx! { if !entries.is_empty() { ManagerViewToggle { view: controls.view(), list_label: translate("tools-view-list"), grid_label: translate("tools-view-grid"), onchange: move |view| { controls.set_view(view); send(&ViewPreference::request(view)); } } } },
                        actions: rsx! {
                            ProjectManagerActions {
                                entries: entries.clone(),
                                presentation: presentation.clone(),
                                controls,
                                pending,
                            }
                        },
                    }
                },
                sidebar: rsx! { ProjectCategories { entries: entries.clone(), controls } },
                ManagerList {
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
                        ProjectOverview { entries: entries.clone(), controls }
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
fn ProjectOverview(
    entries: Vec<ProjectEntry>,
    controls: CollectionController<ProjectFilter>,
) -> Element {
    let mut active = 0u32;
    let mut stale = 0u32;
    let mut archived = 0u32;
    for entry in &entries {
        active = active.saturating_add(entry.ongoing_sessions);
        stale = stale.saturating_add(entry.stale_sessions);
        archived += u32::from(entry.archived);
    }
    rsx! {
        div { class: "flex flex-wrap items-center gap-4 px-2 pb-1 text-xs text-muted-foreground",
            span { "{entries.len()} ", {translate("layout-projects")} }
            ProjectSessionLink { project: String::new(), status: "current", count: active, label: translate("session-stage-active") }
            ProjectSessionLink { project: String::new(), status: "stale", count: stale, label: translate("sessions-stale") }
            if archived > 0 {
                button { class: "flex items-center gap-1.5 hover:text-foreground hover:underline",
                    onclick: move |_| controls.update_filters(|filters| ProjectFilter::select(filters, Some(ProjectFilter::Archived))),
                    "{archived} ", {translate("common-archived")}
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
) -> Element {
    let mut search_open = use_signal(|| false);
    let open = search_open();
    let active = !controls.query().is_empty() || !presentation.selected_filters.is_empty();
    rsx! {
        div { class: "flex shrink-0 items-center gap-3",
            ManagerExpandableSearch {
                open,
                active,
                label: translate("layout-projects"),
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
                        entries: entries.clone(),
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
            ProjectAdd { pending }
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
    let class = if view == ManagerView::Grid {
        "group flex min-w-0 flex-col gap-3 rounded-lg bg-foreground/[0.025] p-3"
    } else {
        "group flex min-w-0 items-center gap-3 rounded-md px-2 py-3 hover:bg-foreground/[0.025]"
    };
    rsx! {
        article { class,
            div { class: "flex min-w-0 flex-1 items-center gap-3",
                IdentityOrb { seed: entry.identity_seed().to_string(), alt: entry.name.clone(), class: "size-6 shrink-0".to_string() }
                a { href: url.clone(), class: "min-w-0 flex-1 hover:text-primary",
                    onclick: move |event: MouseEvent| { event.prevent_default(); PageNavigation::click(url.clone(), &event); },
                    div { class: "truncate text-sm font-medium text-foreground", "{entry.name}" }
                    div { class: "mt-0.5 truncate text-[11px] text-muted-foreground", "{entry.source}" }
                }
                if view == ManagerView::Grid { ProjectActions { entry: entry.clone() } }
            }
            div { class: "flex shrink-0 flex-wrap items-center gap-4 text-xs text-muted-foreground",
                ProjectMeta { entry: entry.clone() }
                if view == ManagerView::List { ProjectActions { entry: entry.clone() } }
            }
        }
    }
}

#[component]
fn ProjectMeta(entry: ProjectEntry) -> Element {
    rsx! {
        if entry.archived {
            span { class: "text-[11px] text-muted-foreground", {translate("project-archived")} }
        }
        if entry.ongoing_sessions > 0 {
            ProjectSessionLink { project: entry.path.clone(), status: "current", count: entry.ongoing_sessions, label: translate("session-stage-active") }
        }
        if entry.stale_sessions > 0 {
            ProjectSessionLink { project: entry.path.clone(), status: "stale", count: entry.stale_sessions, label: translate("sessions-stale") }
        }
        if entry.ongoing_sessions == 0 && entry.stale_sessions == 0 {
            ProjectSessionLink { project: entry.path, status: "", count: 0, label: translate("sessions-title") }
        }
    }
}

#[component]
fn ProjectSessionLink(project: String, status: String, count: u32, label: String) -> Element {
    let url = vmux_session::Route::manager_for_project(&project, &status);
    rsx! {
        a { href: url.clone(), class: "flex items-center gap-1.5 rounded-md py-1 text-xs text-muted-foreground transition-colors hover:text-primary hover:underline",
            onclick: move |event: MouseEvent| { event.prevent_default(); event.stop_propagation(); PageNavigation::click(url.clone(), &event); },
            span { class: "tabular-nums text-foreground/80", "{count}" }
            span { "{label}" }
            LineIconView { icon: LineIcon::ChevronDown, class: "size-3 -rotate-90 shrink-0 opacity-60" }
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
                            ProjectEntry::open(resume_url.clone(), NavigationGesture::default());
                        },
                    }
                    ProjectActionMenuItem {
                        icon: if archived { LineIcon::RotateCcw } else { LineIcon::Archive },
                        label: archive_label.clone(),
                        danger: false,
                        onclick: move |_| {
                            open.set(false);
                            send(&ProjectArchiveRequest {
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
                            send(&ProjectDeleteRequest { url: delete_url.clone() });
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
    fn open(url: String, gesture: NavigationGesture) {
        PageNavigation::gesture(url, gesture);
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
                ManagerAddButton {
                    label: translate("common-add"),
                    disabled: pending,
                    active: open(),
                    onclick: move |_| open.set(!open()),
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
                            send(&ProjectImportRequest);
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
                            size: ButtonSize::Sm,
                            class: "h-8 px-4 text-xs".to_string(),
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
                send(&ProjectCreateRequest { name: draft });
            }
            Self::Clone => {
                send(&ProjectCloneRequest { repository: draft });
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
        send(&GitRepositoryRequest::at(requested_root.clone()));
    });
    let start_url = "vmux://start/".to_string();
    let header_root = root.clone();

    rsx! {
        ManagerPage {
            WorkspaceLayout {
                show_sidebar: false,
                header: rsx! {
                    PageAddress {
                        current: WorkspacePage::Projects.option(),
                        options: WorkspacePage::options(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Sm,
                                class: "h-8 gap-1.5 text-xs".to_string(),
                                onclick: move |_| PageNavigation::replace(start_url.clone()),
                                LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5".to_string() }
                                span { {translate("sessions-start")} }
                            }
                        },
                        BreadcrumbSeparator {}
                        BreadcrumbItem {
                            label: project_name.clone(),
                            title: header_root,
                            active: true,
                            leading: rsx! {
                                IdentityOrb {
                                    seed: project_identity,
                                    alt: project_name.clone(),
                                    class: "size-5".to_string(),
                                }
                            },
                        }
                    }
                },
                PageContent { content_class: "mx-auto flex w-full max-w-5xl flex-col gap-5".to_string(),
                    ProjectSessionsPanel {}
                    div { class: "grid grid-cols-1 gap-5 md:grid-cols-2",
                        ProjectGitPanel { root: root.clone() }
                        ProjectFilesPanel {}
                    }
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
                    preview: Some(5),
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
fn ProjectFilesPanel() -> Element {
    let files = use_ui_state::<crate::project::ProjectFilesUiState>().state;
    let projection = use_memo(move || {
        let files = files.read();
        ProjectFilesProjection {
            loaded: files.ready,
            entries: files.entries.clone(),
            root: files.root.clone(),
            error: files.error.clone(),
        }
    });
    rsx! { ProjectFiles { projection: projection() } }
}

#[derive(Clone, Default, PartialEq)]
struct ProjectFilesProjection {
    loaded: bool,
    entries: Vec<FileDirEntry>,
    root: String,
    error: String,
}

#[component]
fn ProjectFiles(projection: ProjectFilesProjection) -> Element {
    let mut expanded = use_signal(|| true);
    let visible = projection
        .entries
        .iter()
        .take(6)
        .cloned()
        .collect::<Vec<_>>();
    let open_root = projection.root.clone();
    rsx! {
        section { class: "overflow-hidden rounded-lg",
            div { class: "flex h-10 items-center gap-2 px-3",
                div { class: "flex min-w-0 flex-1 items-center gap-2",
                    vmux_ui::icon::BuiltinIconView { icon: vmux_api::BuiltinIcon::Code, class: "size-3.5 shrink-0 text-muted-foreground".to_string() }
                    button { class: "truncate text-xs font-semibold hover:text-primary hover:underline", disabled: open_root.is_empty(),
                        onclick: move |_| PageNavigation::stack(vmux_path::FileUrl::from_path(Path::new(&open_root), None, None, None)),
                        {translate("editor-editor")}
                    }
                    if projection.loaded && projection.error.is_empty() {
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
                    if !projection.error.is_empty() {
                        div { class: "px-3 pb-3 text-xs text-destructive", "{projection.error}" }
                    } else if projection.loaded && projection.entries.is_empty() {
                        div { class: "px-3 pb-3 text-xs text-muted-foreground", {translate("editor-explorer-empty")} }
                    } else if projection.loaded {
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
                PageNavigation::stack(vmux_path::FileUrl::from_path(std::path::Path::new(&open_path), None, None, None));
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
}
