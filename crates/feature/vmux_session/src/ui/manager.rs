use std::rc::Rc;

use dioxus::prelude::*;
#[cfg(test)]
use vmux_session::SessionSummaryProjection;
use vmux_session::{SessionSummary, StageSummary};
use vmux_ui::components::avatar::AgentThumbnail;
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use vmux_ui::components::filter::{FilterMenu, FilterOption, FilterPill, FilterTrigger};
use vmux_ui::components::inline_edit::EditableText;
use vmux_ui::components::manager::{
    ManagerArea, ManagerControls, ManagerEmpty, ManagerIconButton, ManagerPage, ManagerSearch,
    ManagerSearchPopover, ManagerSidebarItem, ManagerSidebarSection, ManagerView,
    ManagerViewToggle, WorkspaceLayout,
};
use vmux_ui::components::motion::Collapse;
use vmux_ui::components::sheet::{SheetHeaderAction, SheetHeaderChevron};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{
    CollectionController, send, use_collection_controller, use_optimistic_map, use_theme,
    use_ui_state,
};
use vmux_ui::i18n::{translate, translate_humanized};
use vmux_ui::icon::{GitIconView, LineIcon, LineIconView};
use vmux_ui::navigation::PageNavigation;

use crate::event::{
    SessionRepositories, SessionRepository, SessionsDescriptionUpdate, SessionsStageChange,
};
use crate::state::SessionCatalogUiState;

use super::manager_controller::*;
use super::manager_model::*;

#[component]
pub fn SessionsManager() -> Element {
    use_theme();
    let ui = use_ui_state::<SessionCatalogUiState>();
    let controls = use_collection_controller::<SessionFilter>(ManagerView::List);
    let filter_picker = SessionFilterPicker::use_state();
    let catalog = ui.use_selector(|state| state.catalog.clone());
    let repositories = ui.use_selector(|state| state.repositories.clone());
    let snapshot = catalog();
    let repositories = repositories();
    let optimistic_stages = use_optimistic_map(ManagerPresentation::stage_observations(&snapshot));
    let optimistic_archives =
        use_optimistic_map(ManagerPresentation::archive_observations(&snapshot));
    let stage_catalog = use_memo(move || catalog.read().stages.clone());
    let mutations = SessionMutations::new(optimistic_stages, optimistic_archives);
    use_context_provider(|| mutations);
    use_context_provider(|| stage_catalog);
    let filters = controls.filters();
    let presentation = Rc::new(ManagerPresentation::new(
        snapshot,
        repositories,
        &controls.query(),
        &filters,
        |id, stage| optimistic_stages.value(id, stage),
        |id, archived| optimistic_archives.value(id, &archived),
        |id| mutations.is_leaving(id),
    ));
    rsx! {
        ManagerPage {
            SessionsManagerContent { controls, filter_picker, presentation }
        }
    }
}

#[component]
pub fn SessionCollection(
    sessions: Vec<SessionSummary>,
    stages: Vec<StageSummary>,
    view: ManagerView,
) -> Element {
    let reactive_stages = stages.clone();
    let stage_catalog = use_memo(use_reactive!(|(reactive_stages,)| reactive_stages.clone()));
    let optimistic_stages = use_optimistic_map(
        sessions
            .iter()
            .map(|session| (session.id.clone(), session.stage.clone()))
            .collect(),
    );
    let optimistic_archives = use_optimistic_map(
        sessions
            .iter()
            .map(|session| (session.id.clone(), session.archived_at > 0))
            .collect(),
    );
    let mutations = SessionMutations::new(optimistic_stages, optimistic_archives);
    use_context_provider(|| mutations);
    use_context_provider(|| stage_catalog);
    let mut sessions = sessions;
    for session in &mut sessions {
        session.stage = optimistic_stages.value(&session.id, &session.stage);
        session.archived_at = if optimistic_archives.value(&session.id, &(session.archived_at > 0))
        {
            session.archived_at.max(1)
        } else {
            0
        };
    }
    sessions.retain(|session| session.archived_at == 0 || mutations.is_leaving(&session.id));
    let groups = StageGroup::inbox(&StageGroup::from_catalog(&stages, &sessions, ""));
    let repositories = SessionRepositories {
        ready: true,
        ..Default::default()
    };
    rsx! {
        SessionGroups { groups, repositories, view }
    }
}

#[component]
fn SessionsManagerActions(
    controls: CollectionController<SessionFilter>,
    filter_picker: SessionFilterPicker,
    presentation: Rc<ManagerPresentation>,
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
                label: translate("sessions-search"),
                align_left,
                ontoggle: move |_| {
                    search_open.set(!open);
                    if open && controls.query().starts_with('#') {
                        filter_picker.dismiss(controls);
                    }
                },
                ondismiss: move |_| {
                    search_open.set(false);
                    if controls.query().starts_with('#') {
                        filter_picker.dismiss(controls);
                    }
                },
                ManagerSearch {
                    value: controls.query(),
                    placeholder: translate("sessions-search"),
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
                                key: "filter-pill-{index}",
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
                            onclick: move |_| filter_picker.toggle(controls),
                        }
                    },
                }
                if presentation.filter_requested {
                    SessionFilterMenu {
                        groups: presentation.category_groups.clone(),
                        sessions: presentation.snapshot.sessions.clone(),
                        repositories: presentation.repositories.clone(),
                        needle: presentation.filter_needle.clone(),
                        step: filter_picker.filter_step(),
                        value: filter_picker.filter_value(),
                        onstep: move |step| filter_picker.select_step(controls, step),
                        onvalue: move |value| filter_picker.set_value(value),
                        onselect: move |filter| filter_picker.apply(controls, filter),
                    }
                }
            }
            ManagerIconButton {
                label: translate("sessions-start"),
                onclick: move |_| StartPage::open(),
                LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
            }
        }
    }
}

#[component]
fn SessionsManagerContent(
    controls: CollectionController<SessionFilter>,
    filter_picker: SessionFilterPicker,
    presentation: Rc<ManagerPresentation>,
) -> Element {
    rsx! {
        WorkspaceLayout {
            active: presentation.selected_stage.is_none().then_some(ManagerArea::Inbox),
            onnavigate: move |area: ManagerArea| {
                if area == ManagerArea::Inbox {
                    controls.update_filters(|filters| SessionFilter::set_stage(filters, None));
                    controls.set_query(String::new());
                    return;
                }
                PageNavigation::replace(area.url());
            },
            sidebar_actions: rsx! {
                SessionsManagerActions {
                    controls,
                    filter_picker,
                    presentation: Rc::clone(&presentation),
                    align_left: true,
                }
            },
            sidebar: rsx! {
                SessionCategories {
                    groups: presentation.category_groups.clone(),
                    selected: presentation.selected_stage.clone(),
                    onselect: move |stage| {
                        controls.update_filters(|filters| SessionFilter::set_stage(filters, stage));
                        controls.set_query(String::new());
                    },
                }
            },
            if !presentation.snapshot.sessions.is_empty() {
                div { class: "shrink-0 px-2 pb-2 pt-2",
                    ManagerControls {
                        ManagerViewToggle {
                            view: controls.view(),
                            list_label: translate("sessions-view-list"),
                            grid_label: translate("sessions-view-board"),
                            grid_icon: LineIcon::Columns3,
                            onchange: move |view| controls.set_view(view),
                        }
                    }
                }
            }
            if presentation.snapshot.sessions.is_empty() {
                ManagerEmpty {
                    title: translate("sessions-empty"),
                    detail: translate("sessions-empty-detail"),
                    actions: rsx! { StartSessionButton {} },
                }
            } else if !presentation.visible_sessions {
                ManagerEmpty {
                    title: translate("sessions-inbox-empty"),
                    detail: translate("sessions-inbox-empty-detail"),
                    actions: rsx! { StartSessionButton {} },
                }
            } else if controls.view() == ManagerView::List {
                SessionsList {
                    groups: presentation.groups.clone(),
                    repositories: presentation.repositories.clone(),
                }
            } else {
                SessionsBoard {
                    groups: presentation.groups.clone(),
                    repositories: presentation.repositories.clone(),
                }
            }
        }
    }
}

struct StartPage;

impl StartPage {
    fn open() {
        PageNavigation::replace("vmux://start/");
    }
}

#[component]
fn StartSessionButton() -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "vmux-primary-action mt-2 inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-[11px] font-medium",
            onclick: move |_| StartPage::open(),
            LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
            span { {translate("sessions-start")} }
        }
    }
}

#[component]
fn SessionCategories(
    groups: Vec<StageGroup>,
    selected: Option<String>,
    onselect: EventHandler<Option<String>>,
) -> Element {
    rsx! {
        ManagerSidebarSection { title: translate("common-status"),
            for group in groups {
                if !group.sessions.is_empty() {
                    ManagerSidebarItem {
                        key: "{group.stage.id}",
                        label: translate_humanized(&group.stage.name),
                        count: group.sessions.len(),
                        selected: selected.as_ref() == Some(&group.stage.id),
                        dot: StageVisual::for_id(&group.stage.id).dot.to_string(),
                        onclick: move |_| onselect.call(Some(group.stage.id.clone())),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionFilterMenu(
    groups: Vec<StageGroup>,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
    needle: String,
    step: SessionFilterStep,
    value: String,
    onstep: EventHandler<SessionFilterStep>,
    onvalue: EventHandler<String>,
    onselect: EventHandler<SessionFilter>,
) -> Element {
    match step {
        SessionFilterStep::Field => rsx! { SessionFilterFields { needle, onstep } },
        SessionFilterStep::Status => {
            rsx! { SessionStatusFilters { groups, needle, onstep, onselect } }
        }
        SessionFilterStep::Agent => rsx! {
            SessionValueFilters {
                title: translate("resume-column-agent"),
                kind: SessionFilterValueKind::Agent,
                values: SessionFilterMenuPresentation::agents(&sessions),
                needle,
                onstep,
                onselect,
            }
        },
        SessionFilterStep::Project => rsx! {
            SessionValueFilters {
                title: translate("resume-column-project"),
                kind: SessionFilterValueKind::Project,
                values: SessionFilterMenuPresentation::projects(&sessions, &repositories),
                needle,
                onstep,
                onselect,
            }
        },
        SessionFilterStep::Updated => rsx! { SessionUpdatedFilters { onstep } },
        SessionFilterStep::Date(comparison) => {
            let candidate = UpdatedFilter::from_date(comparison, &value);
            rsx! {
                SessionUpdatedFilterInput {
                    title: format!("{} · {}", translate("common-updated"), comparison.label()),
                    input_type: "date".to_string(),
                    value,
                    candidate,
                    onstep,
                    onvalue,
                    onselect,
                }
            }
        }
        SessionFilterStep::Within => {
            let candidate = value.parse().ok().and_then(UpdatedFilter::within);
            rsx! {
                SessionUpdatedFilterInput {
                    title: format!("{} · {}", translate("common-updated"), UpdatedComparison::Within.label()),
                    input_type: "number".to_string(),
                    suffix: translate("common-days"),
                    value,
                    candidate,
                    onstep,
                    onvalue,
                    onselect,
                }
            }
        }
    }
}

#[component]
fn SessionFilterFields(needle: String, onstep: EventHandler<SessionFilterStep>) -> Element {
    let options = [
        ("common-status", SessionFilterStep::Status),
        ("resume-column-agent", SessionFilterStep::Agent),
        ("resume-column-project", SessionFilterStep::Project),
        ("common-updated", SessionFilterStep::Updated),
    ];
    rsx! {
        FilterMenu { title: translate("common-filter"),
            for (label_id, step) in options {
                if needle.is_empty() || translate(label_id).to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        key: "field-{label_id}",
                        label: translate(label_id),
                        prefix: "#".to_string(),
                        next: true,
                        onclick: move |_| onstep.call(step),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionStatusFilters(
    groups: Vec<StageGroup>,
    needle: String,
    onstep: EventHandler<SessionFilterStep>,
    onselect: EventHandler<SessionFilter>,
) -> Element {
    rsx! {
        FilterMenu { title: translate("common-status"), onback: move |_| onstep.call(SessionFilterStep::Field),
            for group in groups {
                if needle.is_empty() || translate_humanized(&group.stage.name).to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        key: "filter-{group.stage.id}",
                        label: translate_humanized(&group.stage.name),
                        dot: StageVisual::for_id(&group.stage.id).dot.to_string(),
                        trailing: group.sessions.len().to_string(),
                        onclick: move |_| onselect.call(SessionFilter::Stage(group.stage.id.clone())),
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SessionFilterValueKind {
    Agent,
    Project,
}

impl SessionFilterValueKind {
    fn filter(self, value: String) -> SessionFilter {
        match self {
            Self::Agent => SessionFilter::Agent(value),
            Self::Project => SessionFilter::Project(value),
        }
    }
}

#[component]
fn SessionValueFilters(
    title: String,
    kind: SessionFilterValueKind,
    values: Vec<(String, usize)>,
    needle: String,
    onstep: EventHandler<SessionFilterStep>,
    onselect: EventHandler<SessionFilter>,
) -> Element {
    rsx! {
        FilterMenu { title, onback: move |_| onstep.call(SessionFilterStep::Field),
            for (value, count) in values {
                if needle.is_empty() || value.to_ascii_lowercase().contains(&needle) {
                    FilterOption {
                        key: "value-{value}",
                        label: value.clone(),
                        trailing: count.to_string(),
                        onclick: move |_| onselect.call(kind.filter(value.clone())),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionUpdatedFilters(onstep: EventHandler<SessionFilterStep>) -> Element {
    rsx! {
        FilterMenu { title: translate("common-updated"), onback: move |_| onstep.call(SessionFilterStep::Field),
            for comparison in [UpdatedComparison::Before, UpdatedComparison::OnOrBefore, UpdatedComparison::After, UpdatedComparison::OnOrAfter] {
                FilterOption {
                    key: "updated-{comparison:?}",
                    label: comparison.label(),
                    detail: comparison.detail(),
                    next: true,
                    onclick: move |_| onstep.call(SessionFilterStep::Date(comparison)),
                }
            }
            FilterOption {
                label: UpdatedComparison::Within.label(),
                next: true,
                onclick: move |_| onstep.call(SessionFilterStep::Within),
            }
        }
    }
}

#[component]
fn SessionUpdatedFilterInput(
    title: String,
    input_type: String,
    #[props(default)] suffix: String,
    value: String,
    candidate: Option<UpdatedFilter>,
    onstep: EventHandler<SessionFilterStep>,
    onvalue: EventHandler<String>,
    onselect: EventHandler<SessionFilter>,
) -> Element {
    rsx! {
        FilterMenu { title, onback: move |_| onstep.call(SessionFilterStep::Updated),
            div { class: "flex items-center gap-2 p-1",
                div { class: "flex h-8 min-w-0 flex-1 items-center rounded-md bg-foreground/[0.035] ring-1 ring-inset ring-foreground/[0.09] focus-within:ring-foreground/[0.18]",
                    input {
                        r#type: input_type,
                        min: "1",
                        class: "h-full min-w-0 flex-1 bg-transparent px-2 text-[11px] text-foreground outline-none",
                        value,
                        oninput: move |event| onvalue.call(event.value()),
                    }
                    if !suffix.is_empty() {
                        span { class: "shrink-0 pr-2 text-[10px] text-muted-foreground", "{suffix}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "vmux-primary-action h-8 shrink-0 rounded-md px-3 text-[11px] font-medium disabled:cursor-default disabled:opacity-35",
                    disabled: candidate.is_none(),
                    onclick: move |_| {
                        if let Some(updated) = candidate.clone() {
                            onselect.call(SessionFilter::Updated(updated));
                        }
                    },
                    {translate("common-apply")}
                }
            }
        }
    }
}

#[component]
fn SessionsList(groups: Vec<StageGroup>, repositories: SessionRepositories) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto",
            div { class: "w-full min-w-[44rem] space-y-2 p-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionGroup {
                            key: "{group.stage.id}",
                            view: ManagerView::List,
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionsBoard(groups: Vec<StageGroup>, repositories: SessionRepositories) -> Element {
    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto p-2",
            div { class: "flex min-h-full w-max items-start gap-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionGroup {
                            key: "{group.stage.id}",
                            view: ManagerView::Grid,
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionGroups(
    groups: Vec<StageGroup>,
    repositories: SessionRepositories,
    view: ManagerView,
) -> Element {
    if view == ManagerView::List {
        return rsx! { SessionsList { groups, repositories } };
    }
    rsx! { SessionsBoard { groups, repositories } }
}

#[component]
fn SessionGroup(
    view: ManagerView,
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let pager = SessionPager::use_state();
    let total = sessions.len();
    let visible = pager.visible(total);
    let group = StageGroupPresentation::new(&stage, &sessions);
    let section_class = if view == ManagerView::List {
        "rounded-lg bg-foreground/[0.014] p-1"
    } else {
        "glass w-[18rem] shrink-0 overflow-hidden rounded-lg p-1 backdrop-blur-xl"
    };
    rsx! {
        section { class: "group/session-section {section_class}",
            header { class: "glass sticky top-0 z-10 flex h-8 w-full items-center gap-2 rounded-md px-2 text-left backdrop-blur-xl transition-colors hover:bg-glass-hover",
                SheetHeaderChevron {
                    expanded: !collapsed(),
                    label: if collapsed() { translate("common-expand") } else { translate("common-collapse") },
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        collapsed.set(!collapsed());
                    },
                }
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 text-left",
                    aria_expanded: !collapsed(),
                    onclick: move |_| collapsed.set(!collapsed()),
                    span { class: "flex size-4 shrink-0 items-center justify-center",
                        span { class: "size-2 rounded-full {group.dot}" }
                    }
                    h2 { class: "truncate text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                    span { class: "text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
                }
                if stage.id == "stale" && !group.archivable.is_empty() {
                    div { class: "ml-auto opacity-0 transition-opacity group-hover/session-section:opacity-100 focus-within:opacity-100",
                        SessionArchiveAllMenu { targets: group.archivable }
                    }
                }
            }
            SessionGroupContents {
                view,
                collapsed: collapsed(),
                stage: stage.id,
                sessions,
                repositories,
                visible,
                total,
                on_more: move |event| pager.load_more(event, total),
            }
        }
    }
}

#[component]
fn SessionGroupContents(
    view: ManagerView,
    collapsed: bool,
    stage: String,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
    visible: usize,
    total: usize,
    on_more: EventHandler<Event<VisibleData>>,
) -> Element {
    let items_class = if view == ManagerView::List {
        "space-y-px pt-0.5"
    } else {
        "mt-1 space-y-1"
    };
    let collapse_class = if collapsed {
        "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
    } else {
        "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
    };
    rsx! {
        div { class: "{collapse_class}",
            div { class: "min-h-0 overflow-hidden",
                div { class: "{items_class}",
                    for session in sessions.into_iter().take(visible) {
                        SessionItem {
                            key: "{session.id}",
                            view,
                            repository: repositories.get(&session.id),
                            repository_ready: repositories.ready,
                            stage: stage.clone(),
                            session,
                        }
                    }
                    if visible < total {
                        div { class: "h-1", onvisible: move |event| on_more.call(event) }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionItem(
    view: ManagerView,
    session: SessionSummary,
    repository: Option<SessionRepository>,
    repository_ready: bool,
    stage: String,
) -> Element {
    let stage_catalog = use_context::<Memo<Vec<StageSummary>>>();
    let stages = stage_catalog.read();
    let leaving = use_context::<SessionMutations>().is_leaving(&session.id);
    let presentation = SessionPresentation::new(&session, repository, &stages, &stage);
    let resume = ResumeTarget::from(&session);
    rsx! {
        Collapse { leaving,
            if view == ManagerView::List {
                SessionListItem { session, presentation, resume, repository_ready }
            } else {
                SessionBoardItem { session, presentation, resume, repository_ready }
            }
        }
    }
}

#[component]
fn SessionListItem(
    session: SessionSummary,
    presentation: SessionPresentation,
    resume: ResumeTarget,
    repository_ready: bool,
) -> Element {
    let open_session = resume.clone();
    rsx! {
        div {
            class: "group relative flex min-h-11 cursor-pointer items-center gap-2 rounded-md px-2.5 py-1 text-xs transition-colors hover:bg-foreground/[0.03]",
            onclick: move |_| open_session.open(),
            span { class: "size-3 shrink-0" }
            AgentThumbnail {
                src: presentation.agent.icon.clone(),
                seed: presentation.agent.id.clone(),
                background: presentation.agent.color.clone(),
                alt: presentation.agent.name.clone(),
                class: "size-4 shrink-0 text-[5px]".to_string(),
            }
            SessionIdPill { label: presentation.compact_id.clone(), title: session.sid.clone() }
            span { class: "flex size-4 shrink-0 items-center justify-center",
                span { class: "size-2 rounded-full {presentation.stage_dot}" }
            }
            div { class: "min-w-0 flex-1",
                div { class: "truncate font-medium text-foreground/90", "{presentation.title}" }
                if !presentation.preview.is_empty() {
                    div { class: "truncate text-[10px] leading-4 text-muted-foreground/60", "{presentation.preview}" }
                }
            }
            div { class: "flex min-w-0 shrink-0 items-center gap-1.5",
                SessionContextView {
                    context: presentation.context.clone(),
                    loading: !repository_ready,
                }
                if presentation.runtime.visible {
                    MetadataPill {
                        dot: presentation.runtime.dot.to_string(),
                        label: runtime_label(&session.runtime),
                    }
                }
                time { class: "w-10 text-right text-[10px] tabular-nums text-muted-foreground/65", "{presentation.updated}" }
            }
            SessionActions {
                resume,
                git_url: presentation.context.git_url,
                cleanup_id: session.id,
                archived: presentation.archived,
                managed: session.managed,
            }
        }
    }
}

#[component]
fn SessionBoardItem(
    session: SessionSummary,
    presentation: SessionPresentation,
    resume: ResumeTarget,
    repository_ready: bool,
) -> Element {
    let open_session = resume.clone();
    rsx! {
        article {
            class: "glass group relative cursor-pointer rounded-md px-2.5 py-2.5 transition-colors hover:bg-glass-hover",
            onclick: move |_| open_session.open(),
            div { class: "flex items-start gap-2",
                AgentThumbnail {
                    src: presentation.agent.icon.clone(),
                    seed: presentation.agent.id.clone(),
                    background: presentation.agent.color.clone(),
                    alt: presentation.agent.name.clone(),
                    class: "size-5 text-[6px]".to_string(),
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-xs font-medium text-foreground/90", "{presentation.title}" }
                    if !presentation.agent.name.is_empty() {
                        div { class: "truncate text-[10px] text-muted-foreground/65", "{presentation.agent.name}" }
                    }
                }
                SessionActions {
                    resume,
                    git_url: presentation.context.git_url.clone(),
                    cleanup_id: session.id,
                    archived: presentation.archived,
                    managed: session.managed,
                }
            }
            if !presentation.preview.is_empty() {
                p { class: "mt-1 max-h-8 overflow-hidden text-[10px] leading-4 text-muted-foreground/65", "{presentation.preview}" }
            }
            div { class: "mt-3 flex min-w-0 flex-wrap items-center gap-1.5",
                if presentation.runtime.visible {
                    MetadataPill { dot: presentation.runtime.dot.to_string(), label: runtime_label(&session.runtime) }
                }
                SessionContextView {
                    context: presentation.context.clone(),
                    loading: !repository_ready,
                }
            }
            div { class: "mt-3 flex items-center gap-1.5 pt-1 text-[10px] text-muted-foreground/70",
                time { class: "tabular-nums", "{presentation.updated}" }
                div { class: "flex-1" }
            }
        }
    }
}

#[component]
pub(super) fn SessionMetaPanel(
    session: SessionSummary,
    repository: Option<SessionRepository>,
    stages: Vec<StageSummary>,
) -> Element {
    let presentation = SessionPresentation::new(&session, repository, &stages, &session.stage);
    rsx! {
        aside { class: "flex h-full min-h-0 w-[19rem] shrink-0 flex-col border-l border-foreground/[0.06] bg-background/35",
            header { class: "flex h-12 shrink-0 items-center px-5",
                h2 { class: "text-xs font-semibold text-foreground/90", {translate("editor-properties")} }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-5 pb-5",
                SessionDescription { session: session.clone() }
                SessionProperties { session, presentation, stages }
            }
        }
    }
}

#[component]
fn SessionDescription(session: SessionSummary) -> Element {
    rsx! {
        if session.managed {
            EditableDescription {
                session_id: session.id,
                value: session.description,
            }
        } else if !session.description.trim().is_empty() {
            p { class: "mb-4 px-1 text-xs leading-5 text-muted-foreground", "{session.description}" }
        }
    }
}

#[component]
fn SessionProperties(
    session: SessionSummary,
    presentation: SessionPresentation,
    stages: Vec<StageSummary>,
) -> Element {
    let SessionPresentation {
        context,
        stage_target,
        runtime,
        agent,
        compact_id,
        updated,
        ..
    } = presentation;
    rsx! {
        div { class: "grid gap-3",
            SessionDetailField { label: translate("sessions-stage"),
                if session.managed {
                    StagePicker {
                        target: stage_target,
                        selected: session.stage.clone(),
                        stages,
                    }
                } else {
                    div { class: "flex min-w-0 items-center gap-2 text-[10px] text-foreground/75",
                        span { class: "size-1.5 shrink-0 rounded-full {StageVisual::for_id(&session.stage).dot}" }
                        span { class: "truncate", {translate_humanized(&session.stage)} }
                    }
                }
            }
            if !agent.name.is_empty() {
                SessionDetailField { label: translate("resume-column-agent"),
                    div { class: "flex min-w-0 items-center gap-1.5",
                        AgentThumbnail {
                            src: agent.icon.clone(),
                            seed: agent.id.clone(),
                            background: agent.color.clone(),
                            alt: agent.name.clone(),
                            class: "size-4 shrink-0 text-[5px]".to_string(),
                        }
                        span { class: "truncate text-[10px] text-foreground/75", "{agent.name}" }
                    }
                }
            }
            SessionDetailField { label: translate("common-status"),
                div { class: "flex min-w-0 items-center gap-2 text-[10px] text-foreground/75",
                    span { class: "size-1.5 shrink-0 rounded-full {runtime.dot}" }
                    span { class: "truncate", {runtime_label(&session.runtime)} }
                }
            }
            if !context.project.is_empty() {
                SessionDetailField { label: translate("resume-column-project"),
                    div { class: "flex min-w-0 items-center gap-2 text-[10px] text-foreground/75",
                        LineIconView { icon: LineIcon::Package, class: "size-3 shrink-0 text-muted-foreground".to_string() }
                        span { class: "truncate", "{context.project}" }
                    }
                }
            }
            if !context.branch.is_empty() {
                SessionDetailField { label: translate("resume-column-branch"),
                    div { class: "flex min-w-0 items-center gap-2 text-[10px] text-foreground/75",
                        LineIconView { icon: LineIcon::GitBranch, class: "size-3 shrink-0 text-muted-foreground".to_string() }
                        code { class: "truncate font-mono", "{context.branch}" }
                    }
                }
            }
            SessionDetailField { label: translate("resume-column-updated"),
                span { class: "truncate text-[10px] text-foreground/75", "{updated}" }
            }
            SessionDetailField { label: translate("schema-id"),
                code { class: "truncate font-mono text-[10px] text-foreground/70", title: "{session.sid}", "{compact_id}" }
            }
            if !session.cwd.is_empty() {
                SessionDetailText {
                    label: translate("sessions-directory"),
                    value: session.cwd,
                    mono: true,
                }
            }
        }
    }
}

#[component]
fn SessionDetailField(label: String, children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3",
            span { class: "text-[10px] font-medium text-muted-foreground/70", "{label}" }
            {children}
        }
    }
}

#[component]
fn SessionDetailText(label: String, value: String, #[props(default)] mono: bool) -> Element {
    let class = if mono {
        "break-all font-mono text-[10px] text-foreground/70"
    } else {
        "break-words text-xs text-foreground/75"
    };
    rsx! {
        SessionDetailField { label,
            span { class: "{class}", "{value}" }
        }
    }
}

#[component]
fn EditableDescription(session_id: String, value: String) -> Element {
    let editing = use_signal(|| false);
    let draft = use_signal(|| value.clone());
    let empty = value.is_empty();
    let display_class = if empty {
        "mb-4 block w-full rounded-lg px-3 py-2.5 text-left text-xs leading-5 text-muted-foreground/50 hover:bg-foreground/[0.035]"
    } else {
        "mb-4 block w-full whitespace-pre-wrap rounded-lg px-3 py-2.5 text-left text-xs leading-5 text-foreground/65 hover:bg-foreground/[0.035]"
    };
    rsx! {
        EditableText {
            value,
            editing,
            draft,
            display_class: display_class.to_string(),
            input_class: "mb-4 min-h-20 w-full resize-y rounded-lg bg-foreground/[0.035] px-3 py-2.5 text-xs leading-5 text-foreground outline-none ring-1 ring-inset ring-primary/35".to_string(),
            title: translate("sessions-description"),
            placeholder: translate("sessions-description-placeholder"),
            empty_display: translate("sessions-description-placeholder"),
            multiline: true,
            allow_empty: true,
            trim: false,
            on_commit: move |description| {
                let _ = send(&SessionsDescriptionUpdate {
                    id: session_id.clone(),
                    description,
                });
            },
        }
    }
}

#[component]
fn SessionActions(
    resume: ResumeTarget,
    git_url: String,
    cleanup_id: String,
    archived: bool,
    managed: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let archive = ArchiveTarget {
        id: cleanup_id.clone(),
        archived,
    };
    let mutations = use_context::<SessionMutations>();
    let visibility = if open() {
        "opacity-100"
    } else {
        "opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
    };
    rsx! {
        Dropdown {
            open: open(),
            class: "flex w-[5.5rem] shrink-0 justify-end gap-0.5".to_string(),
            on_dismiss: move |_| open.set(false),
            SessionActionButton {
                label: translate("sessions-open-chat"),
                visibility: visibility.to_string(),
                onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    resume.open();
                },
                LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5".to_string() }
            }
            if !git_url.is_empty() {
                SessionActionButton {
                    label: translate("git-title"),
                    visibility: visibility.to_string(),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        PageNavigation::replace(git_url.clone());
                    },
                    GitIconView { class: "size-3.5".to_string() }
                }
            }
            SessionActionButton {
                label: translate("sessions-actions"),
                visibility: visibility.to_string(),
                onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    open.set(!open());
                },
                LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5".to_string() }
            }
            if open() {
                DropdownPopup {
                    align: DropdownAlign::End,
                    class: "w-36 p-1",
                    onclick: move |event: MouseEvent| event.stop_propagation(),
                    if !archived {
                        SessionArchiveButton {
                            onclick: move |_| {
                                open.set(false);
                                mutations.archive(vec![archive.clone()]);
                            },
                        }
                    }
                    if managed {
                        SessionDeleteButton {
                            onclick: move |_| {
                                open.set(false);
                                mutations.cleanup(cleanup_id.clone());
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionActionButton(
    label: String,
    visibility: String,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-all hover:bg-foreground/[0.08] hover:text-foreground {visibility}",
            title: label.clone(),
            aria_label: label,
            onclick: move |event| onclick.call(event),
            {children}
        }
    }
}

#[component]
fn SessionArchiveAllMenu(targets: Vec<ArchiveTarget>) -> Element {
    let mut open = use_signal(|| false);
    let label = translate("sessions-actions");
    let mutations = use_context::<SessionMutations>();
    rsx! {
        Dropdown {
            open: open(),
            class: "shrink-0".to_string(),
            on_dismiss: move |_| open.set(false),
            SheetHeaderAction {
                label,
                onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    open.set(!open());
                },
                LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5 shrink-0".to_string() }
            }
            if open() {
                DropdownPopup {
                    align: DropdownAlign::End,
                    class: "w-40 p-1",
                    onclick: move |event: MouseEvent| event.stop_propagation(),
                    button {
                        r#type: "button",
                        class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-foreground/75 transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
                        onclick: move |_| {
                            open.set(false);
                            mutations.archive(targets.clone());
                        },
                        LineIconView { icon: LineIcon::Archive, class: "size-3.5 shrink-0".to_string() }
                        span { {translate("sessions-archive-all")} }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionArchiveButton(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-foreground/75 transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
            onclick: move |event| onclick.call(event),
            LineIconView { icon: LineIcon::Archive, class: "size-3.5 shrink-0".to_string() }
            span { {translate("common-archive")} }
        }
    }
}

#[component]
fn SessionDeleteButton(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-ansi-1 transition-colors hover:bg-ansi-1/10",
            onclick: move |event| onclick.call(event),
            LineIconView { icon: LineIcon::Trash, class: "size-3.5 shrink-0".to_string() }
            span { {translate("sessions-delete")} }
        }
    }
}

#[component]
fn StagePicker(target: StageTarget, selected: String, stages: Vec<StageSummary>) -> Element {
    let mut open = use_signal(|| false);
    let stage_dot = StageVisual::for_id(&selected).dot;
    let selected_name = stages
        .iter()
        .find(|stage| stage.id == selected)
        .map(|stage| translate_humanized(&stage.name))
        .unwrap_or_else(|| translate_humanized(&selected));
    rsx! {
        Dropdown {
            open: open(),
            class: "inline-flex w-fit max-w-full min-w-0 justify-self-start".to_string(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: "inline-flex h-7 max-w-full min-w-0 cursor-pointer items-center gap-2 rounded-md px-1 text-[10px] text-foreground/75 outline-none hover:bg-foreground/[0.045] focus-visible:bg-foreground/[0.055]",
                title: translate("sessions-stage"),
                aria_expanded: open(),
                onclick: move |_| open.set(!open()),
                span { class: "size-1.5 shrink-0 rounded-full {stage_dot}" }
                span { class: "min-w-0 truncate", "{selected_name}" }
                LineIconView {
                    icon: LineIcon::ChevronDown,
                    class: if open() {
                        "size-2.5 shrink-0 rotate-180 text-muted-foreground".to_string()
                    } else {
                        "size-2.5 shrink-0 text-muted-foreground".to_string()
                    },
                }
            }
            if open() {
                DropdownPopup { match_width: true, class: "min-w-full p-1",
                    for stage in stages {
                        StagePickerOption {
                            target: target.clone(),
                            selected_id: selected.clone(),
                            stage,
                            open,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StagePickerOption(
    target: StageTarget,
    selected_id: String,
    stage: StageSummary,
    mut open: Signal<bool>,
) -> Element {
    let selected = stage.id == selected_id;
    let stage_id = stage.id.clone();
    let dot = StageVisual::for_id(&stage.id).dot;
    let name = translate_humanized(&stage.name);
    let mutations = try_consume_context::<SessionMutations>();
    rsx! {
        button {
            r#type: "button",
            class: if selected {
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[10px] text-foreground bg-foreground/[0.07]"
            } else {
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[10px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground"
            },
            onclick: move |_| {
                open.set(false);
                if let Some(mutations) = mutations {
                    mutations.change(target.clone(), stage_id.clone());
                } else {
                    let _ = send(&SessionsStageChange {
                        id: target.id.clone(),
                        kind: target.kind.clone(),
                        sid: target.sid.clone(),
                        stage: stage_id.clone(),
                    });
                }
            },
            span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            span { class: "whitespace-nowrap", "{name}" }
        }
    }
}

#[component]
fn SessionContextView(context: SessionContext, loading: bool) -> Element {
    rsx! {
        div { class: "flex min-w-0 items-center gap-1.5",
            if loading {
                Skeleton { class: "h-[18px] w-16 rounded-md bg-foreground/[0.055]" }
                Skeleton { class: "h-[18px] w-12 rounded-md bg-foreground/[0.045]" }
            } else if !context.project.is_empty() {
                MetadataPill { label: context.project, icon: Some(LineIcon::Package) }
            }
            if !loading && !context.branch.is_empty() {
                MetadataPill { label: context.branch, icon: Some(LineIcon::GitBranch), mono: true }
            }
        }
    }
}

#[component]
fn MetadataPill(
    label: String,
    #[props(default)] title: String,
    #[props(default)] dot: String,
    #[props(default)] icon: Option<LineIcon>,
    #[props(default)] mono: bool,
) -> Element {
    let label_class = if mono { "font-mono" } else { "" };
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    rsx! {
        span { class: "inline-flex h-[18px] max-w-full min-w-0 items-center gap-1 rounded-md bg-foreground/[0.045] px-1.5 text-[10px] text-muted-foreground", title,
            if !dot.is_empty() {
                span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            }
            if let Some(icon) = icon {
                LineIconView { icon, class: "size-2.5 shrink-0 opacity-65".to_string() }
            }
            span { class: "max-w-32 truncate {label_class}", "{label}" }
        }
    }
}

#[component]
fn SessionIdPill(label: String, title: String) -> Element {
    rsx! {
        span {
            class: "inline-flex h-[18px] w-[6.75rem] shrink-0 items-center justify-center rounded-md bg-foreground/[0.025] px-1.5 font-mono text-[9px] text-muted-foreground/55",
            title,
            "{label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_without_stage_definition_remains_visible() {
        let session = SessionSummary {
            id: "session-1".into(),
            name: "Task".into(),
            stage: "active".into(),
            ..Default::default()
        };

        let groups = StageGroup::from_catalog(&[], std::slice::from_ref(&session), "");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].stage.id, "active");
        assert_eq!(groups[0].sessions, vec![session]);
    }

    #[test]
    fn session_identity_preserves_both_ends() {
        let id = SessionIdentity::new("01a0fc43-68f9-7121-a388-59e1891fdc41");

        assert_eq!(id.compact(), "01a0fc…dc41");
    }

    #[test]
    fn stale_requires_three_days_without_active_work() {
        let now = 10 * SessionSummaryProjection::STALE_AFTER as i64 * 1_000;
        let stages = vec![StageSummary {
            id: "active".into(),
            terminal: false,
            ..Default::default()
        }];
        let mut session = SessionSummary {
            stage: "active".into(),
            runtime: "idle".into(),
            last_activated_at: now - (SessionSummaryProjection::STALE_AFTER as i64 * 1_000),
            ..Default::default()
        };

        assert!(SessionSummaryProjection::new(&session, &stages, now).stale);
        session.last_activated_at += 1_000;
        assert!(!SessionSummaryProjection::new(&session, &stages, now).stale);
        session.last_activated_at -= 1_000;
        session.runtime = "streaming".into();
        assert!(!SessionSummaryProjection::new(&session, &stages, now).stale);
        session.runtime = "idle".into();
        let terminal = vec![StageSummary {
            id: "active".into(),
            terminal: true,
            ..Default::default()
        }];
        assert!(!SessionSummaryProjection::new(&session, &terminal, now).stale);
    }

    #[test]
    fn stale_session_moves_out_of_its_persisted_stage() {
        let stages = vec![StageSummary {
            id: "in_progress".into(),
            name: "session-stage-active".into(),
            order: 10,
            terminal: false,
        }];
        let session = SessionSummary {
            id: "session-1".into(),
            stage: "in_progress".into(),
            runtime: "idle".into(),
            last_activated_at: vmux_ecs::UnixMillis::now().0
                - (SessionSummaryProjection::STALE_AFTER as i64 * 1_000),
            ..Default::default()
        };

        let groups = StageGroup::from_catalog(&stages, std::slice::from_ref(&session), "");
        let active = groups
            .iter()
            .find(|group| group.stage.id == "in_progress")
            .unwrap();
        let stale = groups
            .iter()
            .find(|group| group.stage.id == "stale")
            .unwrap();

        assert!(active.sessions.is_empty());
        assert_eq!(stale.sessions, vec![session]);
    }

    #[test]
    fn stage_and_updated_filters_compose() {
        let recent = SessionSummary {
            id: "recent".into(),
            stage: "in_progress".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let old = SessionSummary {
            id: "old".into(),
            stage: "in_progress".into(),
            last_activated_at: 50,
            ..Default::default()
        };
        let done = SessionSummary {
            id: "done".into(),
            stage: "done".into(),
            last_activated_at: 200,
            ..Default::default()
        };
        let groups = vec![
            StageGroup {
                stage: StageSummary {
                    id: "in_progress".into(),
                    ..Default::default()
                },
                sessions: vec![recent.clone(), old],
            },
            StageGroup {
                stage: StageSummary {
                    id: "done".into(),
                    ..Default::default()
                },
                sessions: vec![done],
            },
        ];
        let filters = vec![
            SessionFilter::Stage("in_progress".into()),
            SessionFilter::Updated(UpdatedFilter {
                comparison: UpdatedComparison::OnOrAfter,
                threshold: 100,
                value_label: String::new(),
            }),
        ];

        let filtered = StageGroup::filtered(&groups, &filters, &SessionRepositories::default());

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].sessions, vec![recent]);
    }

    #[test]
    fn agent_and_project_filters_compose() {
        let codex = SessionSummary {
            id: "codex".into(),
            stage: "in_progress".into(),
            agent_name: "Codex".into(),
            ..Default::default()
        };
        let claude = SessionSummary {
            id: "claude".into(),
            stage: "in_progress".into(),
            agent_name: "Claude".into(),
            ..Default::default()
        };
        let groups = vec![StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                ..Default::default()
            },
            sessions: vec![codex.clone(), claude],
        }];
        let filters = vec![
            SessionFilter::Agent("Codex".into()),
            SessionFilter::Project("vmux".into()),
        ];
        let repositories = SessionRepositories {
            ready: true,
            sessions: vec![SessionRepository {
                id: "codex".into(),
                project: "vmux".into(),
                ..Default::default()
            }],
        };

        let filtered = StageGroup::filtered(&groups, &filters, &repositories);

        assert_eq!(filtered[0].sessions, vec![codex]);
    }

    #[test]
    fn inbox_excludes_terminal_stages() {
        let active = StageGroup {
            stage: StageSummary {
                id: "in_progress".into(),
                terminal: false,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "active".into(),
                ..Default::default()
            }],
        };
        let completed = StageGroup {
            stage: StageSummary {
                id: "done".into(),
                terminal: true,
                ..Default::default()
            },
            sessions: vec![SessionSummary {
                id: "completed".into(),
                ..Default::default()
            }],
        };

        let inbox = StageGroup::inbox(&[active.clone(), completed]);

        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].stage.id, active.stage.id);
        assert_eq!(inbox[0].sessions, active.sessions);
    }

    #[test]
    fn archived_session_remains_during_its_leave_animation() {
        let snapshot = vmux_session::CatalogSnapshot {
            stages: vec![StageSummary {
                id: "in_progress".into(),
                terminal: false,
                ..Default::default()
            }],
            sessions: vec![SessionSummary {
                id: "leaving".into(),
                stage: "in_progress".into(),
                archived_at: 10,
                ..Default::default()
            }],
            ..Default::default()
        };

        let leaving = ManagerPresentation::new(
            snapshot.clone(),
            SessionRepositories::default(),
            "",
            &[],
            |_, stage| stage.clone(),
            |_, archived| archived,
            |id| id == "leaving",
        );
        let settled = ManagerPresentation::new(
            snapshot,
            SessionRepositories::default(),
            "",
            &[],
            |_, stage| stage.clone(),
            |_, archived| archived,
            |_| false,
        );

        assert!(leaving.visible_sessions);
        assert!(!settled.visible_sessions);
    }
}
