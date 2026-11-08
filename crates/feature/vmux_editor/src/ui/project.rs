use std::path::Path;

use dioxus::prelude::*;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_git::event::{RepositorySummary, RepositorySummaryRequest, RepositorySummaryUiState};
use vmux_session::state::SessionScopeUiState;
use vmux_session::{SessionSummary, SessionSummaryProjection, StageSummary};
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::avatar::IdentityOrb;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use vmux_ui::components::manager::{
    ManagerAddButton, ManagerCollection, ManagerEmpty, ManagerGridSize, ManagerList, ManagerPage,
    ManagerSidebarItem, ManagerSkeleton, ManagerView, ManagerViewToggle, WorkspaceLayout,
};
use vmux_ui::components::page::PageContent;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::tooltip::Tooltip;
use vmux_ui::hooks::{CollectionController, send, use_collection_controller, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{GitSymbol, GitSymbolView, LineIcon, LineIconView};
use vmux_ui::navigation::{NavigationGesture, PageNavigation};

use super::project_manager::{
    ProjectFilter, ProjectFilterMenu, ProjectPresentation, ViewPreference,
};
use crate::project::{
    ProjectArchiveRequest, ProjectCatalogView, ProjectCloneRequest, ProjectCollectionViewRequest,
    ProjectCreateRequest, ProjectDeleteRequest, ProjectDetailView, ProjectEntry,
    ProjectImportRequest, ProjectUiState, ProjectView,
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
                search: rsx! { ProjectSearch { entries: entries.clone(), presentation: presentation.clone(), controls } },
                show_sidebar: entries.iter().any(|entry| entry.archived),
                toolbar: rsx! {
                    ProjectManagerActions {
                        entries: entries.clone(), presentation: presentation.clone(), controls, pending,
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
    let mut toolbar = Vec::new();
    for (index, filter) in presentation.selected_filters.iter().enumerate() {
        toolbar.push(
            ToolbarControl::button(filter.label.clone(), move |_| controls.remove_filter(index))
                .icon(LineIcon::X)
                .text(),
        );
    }
    if !entries.is_empty() {
        toolbar.extend(ToolbarControl::view(
            controls.view(),
            translate("tools-view-list"),
            translate("tools-view-grid"),
            move |view| {
                controls.set_view(view);
                let request: ProjectCollectionViewRequest = ViewPreference::request(view);
                let _ = send(&request);
            },
        ));
    }
    rsx! {
        ProjectAdd { pending, toolbar: Some(toolbar) }
    }
}

#[component]
fn ProjectSearch(
    entries: Vec<ProjectEntry>,
    presentation: ProjectPresentation,
    controls: CollectionController<ProjectFilter>,
) -> Element {
    rsx! {
        Dropdown { open: presentation.filter_requested, keyboard_navigation: true, on_dismiss: move |_| controls.dismiss_filter(),
            vmux_ui::components::manager::ManagerSearchRow {
                vmux_ui::components::manager::ManagerSearch {
                    value: controls.query(), placeholder: translate("layout-projects"),
                    onsearch: move |query| controls.set_query(query),
                    onkeydown: move |event: KeyboardEvent| {
                        if event.key() == Key::Backspace && controls.query().is_empty() { controls.pop_filter(); }
                    },
                    suffix: rsx! { vmux_ui::components::filter::FilterTrigger {
                        active: presentation.filter_requested, label: translate("common-filter"),
                        onclick: move |_| controls.toggle_filter(),
                    } },
                }
            }
            if presentation.filter_requested {
            DropdownPopup { class: "w-72 p-0",
                ProjectFilterMenu {
                    entries, needle: presentation.filter_needle,
                    onselect: move |filter| {
                        controls.update_filters(|filters| ProjectFilter::select(filters, Some(filter)));
                        controls.dismiss_filter();
                    },
                }
            }
            }
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
fn ProjectAdd(
    pending: bool,
    #[props(default)] compact: bool,
    #[props(default)] toolbar: Option<Vec<ToolbarControl>>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut prompt = use_signal(|| None::<ProjectPrompt>);
    let mut draft = use_signal(String::new);
    let ready = !draft().trim().is_empty();
    let mut header = None;
    if let Some(mut controls) = toolbar {
        let mut options = Vec::new();
        for (value, label, icon) in [
            ("new", "project-new", LineIcon::Plus),
            ("import", "project-import", LineIcon::FileOutput),
            ("clone", "project-clone", LineIcon::GitFork),
        ] {
            options.push(vmux_ui::components::page_address::ToolbarChoice {
                value: value.into(),
                label: translate(label),
                icon: format!("{icon:?}"),
            });
        }
        controls.push(
            ToolbarControl::menu(translate("common-add"), options, move |value| {
                draft.set(String::new());
                match value.as_str() {
                    "new" => prompt.set(Some(ProjectPrompt::New)),
                    "clone" => prompt.set(Some(ProjectPrompt::Clone)),
                    "import" => {
                        let _ = send(&ProjectImportRequest);
                    }
                    _ => {}
                }
            })
            .icon(LineIcon::Plus)
            .disabled(pending),
        );
        header = Some(controls);
    }
    rsx! {
        if let Some(controls) = header {
            PageToolbar { controls: controls }
        } else {
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
    let start_url = "vmux://start/".to_string();
    let address = PageAddress {
        current: AddressOption {
            label_key: "layout-projects".into(),
            icon: Some(vmux_api::BuiltinIcon::Project),
            intent: Some(AddressIntent::Navigate {
                url: crate::project::ProjectRoute::ROOT.to_string(),
            }),
            ..Default::default()
        },
        fixed: vec![AddressCrumb {
            entry: AddressOption {
                label: project_name.clone(),
                title: root.clone(),
                identity: root.clone(),
                intent: Some(AddressIntent::Navigate {
                    url: try_consume_context::<vmux_ecs::PageMetadata>()
                        .map(|metadata| metadata.url)
                        .unwrap_or_else(|| {
                            format!("{}{project_identity}", crate::project::ProjectRoute::ROOT)
                        }),
                }),
                ..Default::default()
            },
            active: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    rsx! {
        ManagerPage {
            PageBreadcrumbs { state: address }
            WorkspaceLayout {
                show_sidebar: false,
                toolbar: rsx! {
                    PageToolbar { controls: vec![
                        ToolbarControl::button(translate("sessions-start"), move |_| PageNavigation::replace(start_url.clone())).icon(LineIcon::MessagesRound).primary(),
                    ] }
                },
                PageContent { content_class: "mx-auto flex w-full max-w-5xl flex-col gap-5".to_string(),
                    ProjectSessionsPanel {}
                    div { class: "grid grid-cols-1 gap-5 md:grid-cols-2",
                        ProjectGitPanel { root: root.clone() }
                        ProjectCode { root: root.clone(), name: project_name.clone() }
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
    let selected_view = view();
    let visible = projection.visible_len();
    rsx! {
        ProjectSection {
            title: translate("sessions-title"),
            count: visible,
            actions: rsx! {
                if visible > 0 {
                    ManagerViewToggle {
                        view: selected_view,
                        list_label: translate("tools-view-list"),
                        grid_label: translate("sessions-view-board"),
                        grid_icon: LineIcon::Columns3,
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
                    view: selected_view,
                    preview: Some(5),
                }
            }
        }
    }
}

#[component]
fn ProjectGitPanel(root: String) -> Element {
    let state = use_ui_state::<RepositorySummaryUiState>().state;
    let query_root = root.clone();
    let revision =
        vmux_ui::transport::HostBinding::current().map_or(0, |host| host.navigation_revision());
    use_effect(use_reactive!(|(query_root, revision)| {
        let _ = revision;
        let _ = vmux_ui::hooks::DeferredSend::new(&RepositorySummaryRequest {
            roots: vec![query_root.clone()],
        })
        .send();
    }));
    use_drop(move || {
        let _ = vmux_ui::hooks::DeferredSend::new(&RepositorySummaryRequest { roots: Vec::new() })
            .send();
    });
    let state = state.read();
    let result = state
        .repositories
        .iter()
        .find(|result| ProjectPath::same(&result.root, &root));
    rsx! { ProjectGit {
        root,
        repository: result.and_then(|result| result.summary.clone()),
        loading: result.is_none_or(|result| result.loading),
    } }
}

#[component]
fn ProjectGit(root: String, repository: Option<RepositorySummary>, loading: bool) -> Element {
    let git_url = vmux_git::GitUrl::from_path(Path::new(&root));
    rsx! {
        ProjectSection { title: translate("git-title"), count: repository.as_ref().map(|repository| repository.changed_files as usize).unwrap_or_default(),
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
                            span { class: "truncate text-xs font-medium", "{repository.name}" }
                            span { class: "truncate font-mono text-[10px] text-muted-foreground", "{repository.branch}" }
                        }
                        div { class: "mt-1 flex items-center gap-3 text-[10px] text-muted-foreground",
                            span { {format!("{} {}", repository.changed_files, translate("git-changes"))} }
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
fn ProjectCode(root: String, name: String) -> Element {
    let url = format!(
        "{}?vmux-entry=1",
        vmux_path::FileUrl::from_path(Path::new(&root), None, None, None)
    );
    rsx! {
        ProjectSection { title: translate("editor-editor"), count: 0,
            a {
                href: url.clone(),
                class: "group flex min-h-20 w-full min-w-0 items-center gap-3 rounded-md px-3 py-3 hover:bg-foreground/[0.035]",
                onclick: move |event: MouseEvent| {
                    event.prevent_default();
                    PageNavigation::click(url.clone(), &event);
                },
                vmux_ui::icon::BuiltinIconView {
                    icon: vmux_api::BuiltinIcon::Code,
                    class: "size-4 shrink-0 text-muted-foreground".to_string(),
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-xs font-medium", "{name}" }
                    div { class: "mt-1 truncate text-[10px] text-muted-foreground", "{root}" }
                }
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: "size-3.5 -rotate-90 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100".to_string(),
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
                div { class: "ml-auto flex shrink-0 items-center gap-1", {actions} }
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

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};

    #[test]
    fn project_sessions_switch_to_board_on_the_first_click() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let _runtime = runtime.enter();
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let projection = ProjectSessionList {
            sessions: vec![SessionSummary {
                id: "test".into(),
                sid: "test".into(),
                name: "Example session".into(),
                stage: "active".into(),
                ..Default::default()
            }],
            stages: vec![StageSummary {
                id: "active".into(),
                name: "Active".into(),
                order: 0,
                terminal: false,
            }],
        };
        let mut dom =
            VirtualDom::new_with_props(ProjectSessions, ProjectSessionsProps { projection });
        let first = dom.rebuild_to_vec();
        let title = translate("sessions-view-board");
        let button = first
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "title",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &title => Some(*id),
                _ => None,
            })
            .expect("board toggle");
        assert!(!dioxus_ssr::render(&dom).contains("<article"));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            button,
        );
        dom.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&dom).contains("<article"));
    }
}
