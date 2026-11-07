use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
#[cfg(test)]
use vmux_session::SessionSummaryProjection;
use vmux_session::{SessionSummary, StageSummary};
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::avatar::{AgentThumbnail, IdentityOrb};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::dropdown::{Dropdown, DropdownAlign, DropdownPopup};
use vmux_ui::components::filter::{FilterMenu, FilterOption};
use vmux_ui::components::inline_edit::EditableText;
use vmux_ui::components::manager::{
    ManagerEmpty, ManagerPage, ManagerSidebarItem, ManagerSidebarSection, ManagerView,
    WorkspaceLayout,
};
use vmux_ui::components::motion::{Collapse, Collapsible};
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::components::sheet::{SheetHeaderAction, SheetHeaderChevron};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::hooks::{
    CollectionController, send, use_collection_controller, use_optimistic_map, use_theme,
    use_ui_state,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_humanized, translate_with};
use vmux_ui::icon::{BuiltinIconView, GitIconView, LineIcon, LineIconView};
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
    let route = try_consume_context::<vmux_ecs::PageMetadata>()
        .map(|metadata| metadata.url)
        .unwrap_or_default();
    use_hook(move || {
        controls.update_filters(|filters| {
            if let Some(project) = crate::Route::manager_filter(&route, "project") {
                SessionFilter::upsert(filters, SessionFilter::Project(project));
            }
            match crate::Route::manager_filter(&route, "status").as_deref() {
                Some("current") => SessionFilter::upsert(filters, SessionFilter::Current),
                Some("stale") => {
                    SessionFilter::upsert(filters, SessionFilter::Stage("stale".to_string()))
                }
                Some("archived") => SessionFilter::upsert(filters, SessionFilter::Archived),
                _ => {}
            }
        });
    });
    let filter_picker = SessionFilterPicker::use_state();
    let catalog = ui.use_selector(|state| state.catalog.clone());
    let repositories = ui.use_selector(|state| state.repositories.clone());
    let loading = ui.use_selector(|state| state.loading)();
    let display = use_hook(|| Rc::new(std::cell::RefCell::new(CatalogDisplay::default())));
    let incoming = catalog();
    let settled = display.borrow_mut().update(incoming.clone(), loading);
    let waiting = settled.is_none();
    let snapshot = settled.unwrap_or(vmux_session::CatalogSnapshot {
        stages: incoming.stages,
        sessions: Vec::new(),
    });
    let repositories = repositories();
    let optimistic_stages = use_optimistic_map(ManagerPresentation::stage_observations(&snapshot));
    let optimistic_archives =
        use_optimistic_map(ManagerPresentation::archive_observations(&snapshot));
    let stage_catalog = use_memo(move || catalog.read().stages.clone());
    let mutations = SessionMutations::new(optimistic_stages, optimistic_archives);
    let selection = SessionSelection::use_state();
    use_context_provider(|| mutations);
    use_context_provider(|| selection);
    use_context_provider(|| stage_catalog);
    let filters = controls.filters();
    let mut presentation = ManagerPresentation::new(
        snapshot,
        repositories,
        &controls.query(),
        &filters,
        |id, stage| optimistic_stages.value(id, stage),
        |id, archived| optimistic_archives.value(id, &archived),
        |id| mutations.is_leaving(id),
    );
    presentation.loading = waiting;
    let presentation = Rc::new(presentation);
    let visible_ids = presentation.visible_ids.clone();
    use_effect(use_reactive!(|(visible_ids,)| {
        selection.retain(&visible_ids);
    }));
    rsx! {
        ManagerPage {
            SessionsManagerContent { controls, filter_picker, presentation }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct SessionSelection(Signal<HashSet<String>>);

impl SessionSelection {
    fn use_state() -> Self {
        Self(use_signal(HashSet::new))
    }

    fn ids(self) -> HashSet<String> {
        self.0.read().clone()
    }

    fn toggle(mut self, id: String, selected: bool) {
        if selected {
            self.0.write().insert(id);
        } else {
            self.0.write().remove(&id);
        }
    }

    fn clear(mut self) {
        self.0.write().clear();
    }

    fn retain(mut self, visible: &HashSet<String>) {
        if self.0.peek().iter().all(|id| visible.contains(id)) {
            return;
        }
        self.0.write().retain(|id| visible.contains(id));
    }
}

#[component]
pub fn SessionCollection(
    sessions: Vec<SessionSummary>,
    stages: Vec<StageSummary>,
    view: ManagerView,
    #[props(default)] preview: Option<usize>,
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
        SessionGroups { groups, repositories, view, preview }
    }
}

#[component]
fn SessionsManagerActions(
    controls: CollectionController<SessionFilter>,
    location: PageAddress,
    presentation: Rc<ManagerPresentation>,
) -> Element {
    let mut delete_confirm_open = use_signal(|| false);
    let selection = use_context::<SessionSelection>();
    let selected_ids = selection.ids();
    let selected_sessions = presentation
        .snapshot
        .sessions
        .iter()
        .filter(|session| {
            selected_ids.contains(&session.id) && presentation.visible_ids.contains(&session.id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let selected_count = selected_sessions.len();
    let selected_archived = selected_count > 0
        && selected_sessions
            .iter()
            .all(|session| session.archived_at > 0);
    let selected_targets = selected_sessions
        .iter()
        .map(ArchiveTarget::from_session)
        .collect::<Vec<_>>();
    let selected_deletable = selected_sessions
        .iter()
        .map(DeleteTarget::from_session)
        .collect::<Vec<_>>();
    let mutations = use_context::<SessionMutations>();
    let deleted_provider_count = mutations.deleted_provider_count();
    let delete_confirm = use_memo(move || Some(delete_confirm_open()));
    let mut toolbar = Vec::new();
    if deleted_provider_count > 0 {
        toolbar.push(
            ToolbarControl::button(translate("sessions-restore-deleted"), move |_| {
                mutations.restore_deleted()
            })
            .icon(LineIcon::RotateCcw),
        );
    }
    if selected_count > 0 {
        toolbar.push(ToolbarControl::badge(selected_count.to_string()));
        toolbar.push(
            ToolbarControl::button(
                translate(if selected_archived {
                    "common-restore"
                } else {
                    "common-archive"
                }),
                move |_| {
                    if selected_archived {
                        mutations.restore(selected_targets.clone());
                    } else {
                        mutations.archive(selected_targets.clone());
                    }
                    selection.clear();
                },
            )
            .icon(if selected_archived {
                LineIcon::RotateCcw
            } else {
                LineIcon::Archive
            }),
        );
        if !selected_deletable.is_empty() {
            toolbar.push(
                ToolbarControl::button(translate("sessions-delete"), move |_| {
                    delete_confirm_open.set(true)
                })
                .icon(LineIcon::Trash),
            );
        }
        toolbar.push(
            ToolbarControl::button(translate("common-cancel"), move |_| selection.clear())
                .icon(LineIcon::X),
        );
    }
    for (index, filter) in presentation.selected_filters.iter().enumerate() {
        toolbar.push(
            ToolbarControl::button(filter.label.clone(), move |_| controls.remove_filter(index))
                .icon(LineIcon::X)
                .text(),
        );
    }
    toolbar.extend(ToolbarControl::view(
        controls.view(),
        translate("sessions-view-list"),
        translate("sessions-view-board"),
        move |view| controls.set_view(view),
    ));
    toolbar.push(
        ToolbarControl::button(translate("common-add"), move |_| StartPage::open())
            .icon(LineIcon::Plus)
            .primary(),
    );
    rsx! {
        PageBreadcrumbs { state: location,
            onintent: move |intent| {
                let AddressIntent::SessionLocation { value } = intent else { return; };
                controls.update_filters(|filters| {
                    if value == SessionLocationBreadcrumb::ARCHIVED {
                        SessionFilter::upsert(filters, SessionFilter::Archived);
                    } else if value == SessionLocationBreadcrumb::INBOX {
                        SessionFilter::set_stage(filters, None);
                    } else {
                        SessionFilter::set_stage(filters, Some(value));
                    }
                });
                controls.set_query(String::new());
            },
        }
        PageToolbar { controls: toolbar }
        AlertDialogRoot {
            open: Into::<ReadSignal<Option<bool>>>::into(delete_confirm),
            on_open_change: Callback::new(move |next| delete_confirm_open.set(next)),
            default_open: false,
            attributes: vec![],
            AlertDialogContent { attributes: vec![],
                AlertDialogTitle { attributes: vec![], {translate("sessions-delete-selected-title")} }
                AlertDialogDescription { attributes: vec![],
                    {translate("sessions-delete-selected-detail")}
                }
                AlertDialogActions { attributes: vec![],
                    AlertDialogCancel {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |event: MouseEvent| {
                            event.stop_propagation();
                            delete_confirm_open.set(false);
                        })),
                        {translate("common-cancel")}
                    }
                    AlertDialogAction {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |event: MouseEvent| {
                            event.stop_propagation();
                            delete_confirm_open.set(false);
                            mutations.cleanup_many(selected_deletable.clone());
                            selection.clear();
                        })),
                        {translate("sessions-delete")}
                    }
                }
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
    let location = SessionLocationBreadcrumb::new(&presentation);
    rsx! {
        WorkspaceLayout {
            toolbar: rsx! {
                SessionsManagerActions { controls, location: location.address(), presentation: Rc::clone(&presentation) }
            },
            sidebar: rsx! {
                SessionSearch { controls, filter_picker, presentation: Rc::clone(&presentation) }
                SessionCategories {
                    groups: presentation.category_groups.clone(),
                    selected: presentation.selected_stage.clone(),
                    archived: presentation.selected_archived,
                    archived_count: presentation.archived_count,
                    onselect: move |stage| {
                        controls.update_filters(|filters| SessionFilter::set_stage(filters, stage));
                        controls.set_query(String::new());
                    },
                    onarchived: move |_| {
                        controls.update_filters(|filters| {
                            SessionFilter::upsert(filters, SessionFilter::Archived);
                        });
                        controls.set_query(String::new());
                    },
                }
            },
            div { id: "sessions-scroll", class: "flex min-h-0 flex-1 flex-col overflow-auto [scrollbar-gutter:stable]",
            if presentation.loading {
                SessionsSkeleton {}
            } else if presentation.snapshot.sessions.is_empty() {
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
            } else {
                SessionGroups {
                    groups: presentation.groups.clone(),
                    repositories: presentation.repositories.clone(),
                    view: controls.view(),
                    scrollable: false,
                }
            }
            }
        }
    }
}

struct SessionLocationBreadcrumb {
    label: String,
    value: String,
    options: Vec<AddressOption>,
    icon: vmux_api::BuiltinIcon,
}

impl SessionLocationBreadcrumb {
    const INBOX: &str = "inbox";
    const ARCHIVED: &str = "archived";

    fn address(&self) -> PageAddress {
        PageAddress {
            fixed: vec![AddressCrumb {
                entry: Self::option(self.label.clone(), &self.value, self.icon),
                options: self.options.clone(),
                active: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn new(presentation: &ManagerPresentation) -> Self {
        let mut options = vec![Self::option(
            translate("sessions-inbox"),
            Self::INBOX,
            vmux_api::BuiltinIcon::MessagesRound,
        )];
        for group in &presentation.category_groups {
            if group.sessions.is_empty() {
                continue;
            }
            options.push(Self::option(
                translate_humanized(&group.stage.name),
                &group.stage.id,
                Self::stage_icon(&group.stage),
            ));
        }
        if presentation.archived_count > 0 {
            options.push(Self::option(
                translate("common-archived"),
                Self::ARCHIVED,
                vmux_api::BuiltinIcon::Archive,
            ));
        }
        if presentation.selected_archived {
            return Self {
                label: translate("common-archived"),
                value: Self::ARCHIVED.to_string(),
                options,
                icon: vmux_api::BuiltinIcon::Archive,
            };
        }
        if let Some(selected) = presentation.selected_stage.as_deref()
            && let Some(group) = presentation
                .category_groups
                .iter()
                .find(|group| group.stage.id == selected)
        {
            return Self {
                label: translate_humanized(&group.stage.name),
                value: selected.to_string(),
                options,
                icon: Self::stage_icon(&group.stage),
            };
        }
        Self {
            label: translate("sessions-inbox"),
            value: Self::INBOX.to_string(),
            options,
            icon: vmux_api::BuiltinIcon::MessagesRound,
        }
    }

    fn option(label: String, value: &str, icon: vmux_api::BuiltinIcon) -> AddressOption {
        AddressOption {
            label,
            intent: Some(AddressIntent::SessionLocation {
                value: value.to_string(),
            }),
            icon: Some(icon),
            ..Default::default()
        }
    }

    fn stage_icon(stage: &StageSummary) -> vmux_api::BuiltinIcon {
        if stage.terminal {
            return vmux_api::BuiltinIcon::CheckCircle;
        }
        if stage.id == "stale" {
            return vmux_api::BuiltinIcon::Clock;
        }
        vmux_api::BuiltinIcon::Activity
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
fn SessionSearch(
    controls: CollectionController<SessionFilter>,
    filter_picker: SessionFilterPicker,
    presentation: Rc<ManagerPresentation>,
) -> Element {
    rsx! {
        div { class: "relative z-40 flex min-w-0 px-2 pb-2 pt-1",
            vmux_ui::components::manager::ManagerSearch {
                value: controls.query(),
                placeholder: translate("sessions-search"),
                onsearch: move |query| controls.set_query(query),
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Backspace && controls.query().is_empty() {
                        controls.pop_filter();
                    }
                    if event.key() == Key::Escape {
                        controls.set_query(String::new());
                        controls.dismiss_filter();
                    }
                },
                suffix: rsx! {
                    vmux_ui::components::button::Button {
                        aria_label: translate("common-filter"),
                        size: vmux_ui::components::button::ButtonSize::Icon,
                        variant: vmux_ui::components::button::ButtonVariant::Ghost,
                        class: if presentation.filter_requested { "size-5 bg-foreground/10" } else { "size-5" },
                        onmousedown: move |event: MouseEvent| event.prevent_default(),
                        onclick: move |_| filter_picker.toggle(controls),
                        LineIconView { icon: LineIcon::Hash, class: "size-3.5".to_string() }
                    }
                },
            }
            if presentation.filter_requested {
                div { class: "absolute left-2 top-full z-40 w-72 [&>div]:static",
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
        }
    }
}

#[component]
fn SessionCategories(
    groups: Vec<StageGroup>,
    selected: Option<String>,
    archived: bool,
    archived_count: usize,
    onselect: EventHandler<Option<String>>,
    onarchived: EventHandler<()>,
) -> Element {
    let inbox_count = groups
        .iter()
        .filter(|group| !group.stage.terminal)
        .map(|group| group.sessions.len())
        .sum();
    rsx! {
        div { class: "flex flex-col gap-3 pt-1",
            ManagerSidebarItem {
                label: translate("sessions-inbox"),
                count: inbox_count,
                selected: selected.is_none() && !archived,
                icon: LineIcon::MessagesRound,
                onclick: move |_| onselect.call(None),
            }
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
                if archived_count > 0 {
                    ManagerSidebarItem {
                        label: translate("common-archived"),
                        count: archived_count,
                        selected: archived,
                        icon: Some(LineIcon::Archive),
                        onclick: move |_| onarchived.call(()),
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
fn SessionsSkeleton() -> Element {
    rsx! {
        div { class: "min-w-[44rem] shrink-0 p-3", aria_busy: true, "data-vmux-loading-surface": "",
            div { class: "flex h-8 items-center gap-2", Skeleton { class: "h-3 w-24" } }
            for row in 0..10 {
                div { key: "{row}", class: "flex h-11 items-center gap-2.5 px-2",
                    Skeleton { class: "size-4 shrink-0 rounded-full" }
                    Skeleton { class: "h-3 w-10 shrink-0" }
                    div { class: "min-w-0 flex-1 space-y-1.5",
                        Skeleton { class: "h-3 w-1/2" }
                        Skeleton { class: "h-2 w-1/3" }
                    }
                    Skeleton { class: "h-4 w-20 shrink-0" }
                    Skeleton { class: "h-3 w-10 shrink-0" }
                }
            }
        }
    }
}

#[component]
fn SessionsList(
    groups: Vec<StageGroup>,
    repositories: SessionRepositories,
    selection: Option<SessionSelection>,
    preview: Option<usize>,
    scrollable: bool,
) -> Element {
    rsx! {
        div { class: if scrollable { "min-h-0 flex-1 overflow-auto" } else { "contents" },
            div { class: "w-full min-w-[44rem] space-y-2 p-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionGroup {
                            key: "{group.stage.id}",
                            view: ManagerView::List,
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                            selection,
                            preview,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionsBoard(
    mut groups: Vec<StageGroup>,
    repositories: SessionRepositories,
    selection: Option<SessionSelection>,
    preview: Option<usize>,
    scrollable: bool,
) -> Element {
    groups.sort_by_key(|group| match group.stage.id.as_str() {
        "stale" => (0, 0),
        _ => (1, group.stage.order),
    });
    rsx! {
        div { class: if scrollable { "min-h-0 flex-1 overflow-auto px-2 pb-2" } else { "shrink-0 px-2 pb-2" },
            div { class: "flex min-h-full w-max items-start gap-2",
                for group in groups {
                    if !group.sessions.is_empty() {
                        SessionGroup {
                            key: "{group.stage.id}",
                            view: ManagerView::Grid,
                            stage: group.stage,
                            sessions: group.sessions,
                            repositories: repositories.clone(),
                            selection,
                            preview,
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
    #[props(default)] preview: Option<usize>,
    #[props(default = true)] scrollable: bool,
) -> Element {
    let selection = try_consume_context::<SessionSelection>();
    if view == ManagerView::List {
        return rsx! { SessionsList { groups, repositories, selection, preview, scrollable } };
    }
    rsx! { SessionsBoard { groups, repositories, selection, preview, scrollable } }
}

#[component]
fn SessionGroup(
    view: ManagerView,
    stage: StageSummary,
    sessions: Vec<SessionSummary>,
    repositories: SessionRepositories,
    selection: Option<SessionSelection>,
    preview: Option<usize>,
) -> Element {
    let initially_collapsed = preview.is_some() && stage.id == "stale";
    let mut collapsed = use_signal(move || initially_collapsed);
    let pager = SessionPager::use_state(preview);
    let total = sessions.len();
    let visible = pager.visible(total);
    let group = use_memo(use_reactive((&stage, &sessions), |(stage, sessions)| {
        StageGroupPresentation::new(&stage, &sessions)
    }));
    let group = group.read();
    let section_class = if view == ManagerView::List {
        "p-1"
    } else {
        "w-[20rem] shrink-0 px-1 pb-1"
    };
    rsx! {
        section { class: "group/session-section {section_class}",
            ContextMenu {
                attributes: attributes!(div { class: "contents" }),
                ContextMenuTrigger {
                    attributes: attributes!(div { class: "contents", aria_label: translate_humanized(&stage.name) }),
            header { class: "sticky top-0 z-10 flex h-8 w-full items-center gap-2 bg-background px-1 text-left",
                div { class: "flex h-7 w-fit max-w-full min-w-0 items-center gap-1 px-1",
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
                        class: "flex min-w-0 items-center gap-2 pr-1 text-left",
                        aria_expanded: !collapsed(),
                        onclick: move |_| collapsed.set(!collapsed()),
                        span { class: "flex size-4 shrink-0 items-center justify-center",
                            span { class: "size-2 rounded-full {group.dot}" }
                        }
                        h2 { class: "truncate text-[11px] font-medium text-foreground/85", {translate_humanized(&stage.name)} }
                        span { class: "text-[10px] tabular-nums text-muted-foreground/75", "{sessions.len()}" }
                    }
                }
                if !group.archivable.is_empty() || !group.restorable.is_empty() {
                    div { class: "ml-auto opacity-0 transition-opacity group-hover/session-section:opacity-100 focus-within:opacity-100",
                        SessionArchiveAllMenu {
                            targets: group.archivable.clone(),
                            restorable: group.restorable.clone(),
                            collapsed: collapsed(),
                            on_toggle: move |_| collapsed.set(!collapsed()),
                        }
                    }
                }
            }
                }
                ContextMenuContent { attributes: vec![],
                    SessionGroupMenuItems {
                        context: true,
                        targets: group.archivable.clone(), restorable: group.restorable.clone(),
                        collapsed: collapsed(),
                        on_toggle: move |_| collapsed.set(!collapsed()),
                    }
                }
            }
            SessionGroupContents {
                view,
                collapsed: collapsed(),
                stage: stage.id,
                sessions,
                repositories,
                selection,
                visible,
                total,
                on_more: move |event| pager.load_more(event, total),
                manual: preview.is_some(),
                on_manual_more: move |_| pager.more(total),
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
    selection: Option<SessionSelection>,
    visible: usize,
    total: usize,
    on_more: EventHandler<Event<VisibleData>>,
    manual: bool,
    on_manual_more: EventHandler<()>,
) -> Element {
    let repository_ready = repositories.ready;
    let repository_index = use_memo(use_reactive((&repositories,), |(repositories,)| {
        let mut index = HashMap::with_capacity(repositories.sessions.len());
        for repository in repositories.sessions {
            index.insert(repository.id.clone(), repository);
        }
        index
    }));
    let items_class = if view == ManagerView::List {
        "space-y-px pt-0.5"
    } else {
        "mt-1 space-y-1"
    };
    rsx! {
        Collapsible { expanded: !collapsed,
            div { class: "{items_class}",
                for session in sessions.into_iter().take(visible) {
                    SessionItem {
                        key: "{session.id}",
                        view,
                        repository: repository_index.read().get(&session.id).cloned(),
                        repository_ready,
                        stage: stage.clone(),
                        selection,
                        session,
                    }
                }
                if visible < total {
                    if manual {
                        button { class: "mt-1 flex h-8 w-full items-center justify-center rounded-md text-xs text-muted-foreground hover:bg-foreground/[0.045] hover:text-foreground",
                            onclick: move |_| on_manual_more.call(()),
                            {translate("common-show-more")}
                        }
                    } else {
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
    selection: Option<SessionSelection>,
) -> Element {
    let stage_catalog = use_context::<Memo<Vec<StageSummary>>>();
    let stages = stage_catalog.read();
    let leaving = use_context::<SessionMutations>().is_leaving(&session.id);
    let presentation = SessionPresentation::new(&session, repository, &stages, &stage);
    let resume = ResumeTarget::from(&session);
    let menu_title = presentation.title.clone();
    rsx! {
        Collapse { leaving,
            SessionRowMenu { session: session.clone(), title: menu_title,
            if view == ManagerView::List {
                SessionListItem { session, presentation, resume, repository_ready, selection }
            } else {
                SessionBoardItem { session, presentation, resume, repository_ready, selection }
            }
            }
        }
    }
}

#[component]
fn SessionSelectionToggle(selection: SessionSelection, id: String, size: String) -> Element {
    let selected = selection.0.read().contains(&id);
    let visibility = if selected {
        "opacity-100"
    } else {
        "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
    };
    rsx! {
        div {
            class: "flex {size} shrink-0 items-center justify-center transition-opacity {visibility}",
            onclick: move |event: MouseEvent| event.stop_propagation(),
            Checkbox {
                checked: selected,
                tone: CheckboxTone::Neutral,
                aria_label: translate("common-select"),
                on_checked_change: move |checked| selection.toggle(id.clone(), checked),
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
    selection: Option<SessionSelection>,
) -> Element {
    let open_session = resume.clone();
    rsx! {
        div {
            class: "group relative flex min-h-11 cursor-pointer items-center gap-2 rounded-md px-2.5 py-1 text-xs transition-colors hover:bg-foreground/[0.03]",
            onclick: move |event| open_session.click(&event),
            if let Some(selection) = selection {
                SessionSelectionToggle { selection, id: session.id.clone(), size: "size-4".to_string() }
            }
            IdentityOrb {
                seed: session.id.clone(),
                alt: presentation.title.clone(),
                class: "size-4".to_string(),
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
                AgentThumbnail {
                    src: presentation.agent.icon.clone(),
                    seed: presentation.agent.id.clone(),
                    background: presentation.agent.color.clone(),
                    alt: presentation.agent.name.clone(),
                    class: "size-4 shrink-0 text-[5px]".to_string(),
                }
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
                editor_url: presentation.context.editor_url,
                git_url: presentation.context.git_url,
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
    selection: Option<SessionSelection>,
) -> Element {
    let open_session = resume.clone();
    rsx! {
        article {
            class: "glass group relative cursor-pointer rounded-md px-2.5 py-2.5 transition-colors hover:bg-glass-hover",
            onclick: move |event| open_session.click(&event),
            div { class: "flex items-start gap-2",
                if let Some(selection) = selection {
                    SessionSelectionToggle { selection, id: session.id.clone(), size: "size-5".to_string() }
                }
                IdentityOrb {
                    seed: session.id.clone(),
                    alt: presentation.title.clone(),
                    class: "size-5".to_string(),
                }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-xs font-medium text-foreground/90", "{presentation.title}" }
                    if !presentation.agent.name.is_empty() {
                        div { class: "truncate text-[10px] text-muted-foreground/65", "{presentation.agent.name}" }
                    }
                }
            }
            if !presentation.preview.is_empty() {
                p { class: "mt-1 max-h-8 overflow-hidden text-[10px] leading-4 text-muted-foreground/65", "{presentation.preview}" }
            }
            div { class: "mt-2 flex min-w-0 flex-nowrap items-center gap-1.5",
                AgentThumbnail {
                    src: presentation.agent.icon.clone(),
                    seed: presentation.agent.id.clone(),
                    background: presentation.agent.color.clone(),
                    alt: presentation.agent.name.clone(),
                    class: "size-4 shrink-0 text-[5px]".to_string(),
                }
                if presentation.runtime.visible {
                    MetadataPill { dot: presentation.runtime.dot.to_string(), label: runtime_label(&session.runtime) }
                }
                SessionContextView {
                    context: presentation.context.clone(),
                    loading: !repository_ready,
                }
            }
            div { class: "mt-2 flex h-7 items-center gap-1.5 text-[10px] text-muted-foreground/70",
                time { class: "tabular-nums", "{presentation.updated}" }
                div { class: "flex-1" }
                SessionActions {
                    resume,
                    editor_url: presentation.context.editor_url.clone(),
                    git_url: presentation.context.git_url.clone(),
                }
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
            if !context.project.is_empty() {
                SessionDetailField { label: translate("resume-column-project"),
                    SessionPropertyLink { url: context.project_url.clone(),
                        LineIconView { icon: LineIcon::Package, class: "size-3 shrink-0 text-muted-foreground".to_string() }
                        span { class: "truncate", "{context.project}" }
                    }
                }
            }
            if !context.branch.is_empty() {
                SessionDetailField { label: translate("resume-column-branch"),
                    SessionPropertyLink { url: context.git_url.clone(),
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
                SessionDetailField { label: translate("sessions-directory"),
                    SessionPropertyLink { url: context.editor_url.clone(),
                        span { class: "break-all font-mono text-[10px]", "{session.cwd}" }
                    }
                }
            }
        }
    }
}

#[component]
fn SessionPropertyLink(url: String, children: Element) -> Element {
    let empty = url.is_empty();
    rsx! {
        a {
            href: (!empty).then_some(url.clone()),
            class: if empty { "flex min-w-0 items-center gap-2 text-[10px] text-foreground/75" } else { "flex min-w-0 cursor-pointer items-center gap-2 text-[10px] text-foreground/75 transition-colors hover:text-primary hover:underline" },
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                if !empty { PageNavigation::click(url.clone(), &event); }
            },
            {children}
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
fn SessionActions(resume: ResumeTarget, editor_url: String, git_url: String) -> Element {
    rsx! {
        div { class: "flex shrink-0 justify-end gap-0.5",
            SessionActionButton {
                label: translate("sessions-open-chat"),
                visibility: "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100".to_string(),
                onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    resume.click(&event);
                },
                LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5".to_string() }
            }
            if !editor_url.is_empty() {
                SessionActionButton {
                    label: translate("editor-editor"),
                    visibility: "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100".to_string(),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        PageNavigation::click(editor_url.clone(), &event);
                    },
                    BuiltinIconView {
                        icon: vmux_api::BuiltinIcon::Code,
                        class: "size-3.5".to_string(),
                    }
                }
            }
            if !git_url.is_empty() {
                SessionActionButton {
                    label: translate("git-title"),
                    visibility: "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100".to_string(),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        PageNavigation::click(git_url.clone(), &event);
                    },
                    GitIconView { class: "size-3.5".to_string() }
                }
            }
        }
    }
}

#[component]
fn SessionRowMenu(session: SessionSummary, title: String, children: Element) -> Element {
    let mut confirm_open = use_signal(|| false);
    let archive = ArchiveTarget::from_session(&session);
    let cleanup = DeleteTarget::from_session(&session);
    let cleanup_title = title;
    let archived = session.archived_at > 0;
    let mutations = use_context::<SessionMutations>();
    let confirm = use_memo(move || Some(confirm_open()));
    let value = use_signal(String::new);
    rsx! {
        ContextMenu {
            attributes: vec![],
            ContextMenuTrigger { attributes: vec![], {children} }
            ContextMenuContent { attributes: vec![],
                if !archived {
                    ContextMenuItem {
                        index: 0usize,
                        value: Into::<ReadSignal<String>>::into(value),
                        on_select: move |_: String| mutations.archive(vec![archive.clone()]),
                        attributes: vec![],
                        LineIconView { icon: LineIcon::Archive, class: "mr-2 size-3.5" }
                        {translate("common-archive")}
                    }
                }
                ContextMenuItem {
                    index: usize::from(!archived),
                    value: Into::<ReadSignal<String>>::into(value),
                    on_select: move |_: String| confirm_open.set(true),
                    attributes: vec![],
                    LineIconView { icon: LineIcon::Trash, class: "mr-2 size-3.5" }
                    {translate("sessions-delete")}
                }
            }
        }
        AlertDialogRoot {
            open: Into::<ReadSignal<Option<bool>>>::into(confirm),
            on_open_change: Callback::new(move |next| confirm_open.set(next)),
            default_open: false,
            attributes: vec![],
            AlertDialogContent { attributes: vec![],
                AlertDialogTitle { attributes: vec![], {translate("sessions-delete")} }
                AlertDialogDescription { attributes: vec![],
                    {translate_with(
                        "sessions-delete-detail",
                        &[("name", TranslationValue::String(&cleanup_title))],
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
                            mutations.cleanup(cleanup.clone());
                        })),
                        {translate("sessions-delete")}
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
fn SessionArchiveAllMenu(
    targets: Vec<ArchiveTarget>,
    restorable: Vec<ArchiveTarget>,
    collapsed: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let mut open = use_signal(|| false);
    let label = translate("sessions-actions");
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
                    SessionGroupMenuItems {
                        targets, restorable, collapsed, on_toggle,
                        on_close: move |_| open.set(false),
                    }
                }
            }
        }
    }
}

#[component]
fn SessionGroupMenuItems(
    targets: Vec<ArchiveTarget>,
    restorable: Vec<ArchiveTarget>,
    collapsed: bool,
    on_toggle: EventHandler<()>,
    #[props(default)] context: bool,
    #[props(default)] on_close: Option<EventHandler<()>>,
) -> Element {
    let mutations = use_context::<SessionMutations>();
    let restore_index = usize::from(!targets.is_empty());
    let toggle_index = restore_index + usize::from(!restorable.is_empty());
    rsx! {
        if !targets.is_empty() {
            SessionGroupMenuItem { context, index: 0usize, icon: LineIcon::Archive,
                label: translate("sessions-archive-all"),
                on_select: move |_| { if let Some(close) = on_close { close.call(()); } mutations.archive(targets.clone()); },
            }
        }
        if !restorable.is_empty() {
            SessionGroupMenuItem { context, index: restore_index, icon: LineIcon::RotateCcw,
                label: translate("common-restore"),
                on_select: move |_| { if let Some(close) = on_close { close.call(()); } mutations.restore(restorable.clone()); },
            }
        }
        SessionGroupMenuItem { context, index: toggle_index,
            icon: LineIcon::ChevronDown,
            label: translate(if collapsed { "common-expand" } else { "common-collapse" }),
            on_select: move |_| { if let Some(close) = on_close { close.call(()); } on_toggle.call(()); },
        }
    }
}

#[component]
fn SessionGroupMenuItem(
    context: bool,
    index: usize,
    icon: LineIcon,
    label: String,
    on_select: EventHandler<()>,
) -> Element {
    let value = use_signal(String::new);
    if context {
        return rsx! { ContextMenuItem {
            index, value: Into::<ReadSignal<String>>::into(value), attributes: attributes!(div { aria_label: label.clone() }),
            on_select: move |_: String| on_select.call(()),
            LineIconView { icon, class: "mr-2 size-3.5" }
            "{label}"
        } };
    }
    rsx! { button {
        r#type: "button", role: "menuitem",
        class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-foreground/75 transition-colors hover:bg-foreground/[0.08] hover:text-foreground",
        onclick: move |_| on_select.call(()),
        LineIconView { icon, class: "size-3.5 shrink-0" }
        span { "{label}" }
    } }
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
    let project_seed = if context.project_url.is_empty() {
        context.project.clone()
    } else {
        context.project_url.clone()
    };
    rsx! {
        div { class: "flex min-w-0 flex-1 items-center gap-1.5",
            if loading {
                Skeleton { class: "h-[18px] w-16 rounded-md bg-foreground/[0.055]" }
                Skeleton { class: "h-[18px] w-12 rounded-md bg-foreground/[0.045]" }
            } else if !context.project.is_empty() {
                MetadataPill {
                    label: context.project.clone(),
                    url: context.project_url.clone(),
                    leading: rsx! {
                        IdentityOrb {
                            seed: project_seed,
                            alt: context.project,
                            class: "size-2.5".to_string(),
                        }
                    },
                }
            }
            if !loading && !context.branch.is_empty() {
                MetadataPill { label: context.branch, url: context.git_url, icon: Some(LineIcon::GitBranch), mono: true }
            }
        }
    }
}

#[component]
fn MetadataPill(
    label: String,
    #[props(default)] url: String,
    #[props(default)] title: String,
    #[props(default)] dot: String,
    #[props(default)] icon: Option<LineIcon>,
    #[props(default = rsx! {})] leading: Element,
    #[props(default)] mono: bool,
) -> Element {
    let label_class = if mono { "font-mono" } else { "" };
    let linked = !url.is_empty();
    let link_class = if linked {
        "cursor-pointer transition-colors hover:bg-foreground/[0.075] hover:text-foreground"
    } else {
        ""
    };
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    rsx! {
        a { class: "inline-flex h-[18px] max-w-full min-w-0 items-center gap-1 rounded-md bg-foreground/[0.045] px-1.5 text-[10px] text-muted-foreground {link_class}", title,
            href: linked.then_some(url.clone()),
            onclick: move |event: MouseEvent| {
                if linked {
                    event.prevent_default();
                    event.stop_propagation();
                    PageNavigation::click(url.clone(), &event);
                }
            },
            if !dot.is_empty() {
                span { class: "size-1.5 shrink-0 rounded-full {dot}" }
            }
            if let Some(icon) = icon {
                LineIconView { icon, class: "size-2.5 shrink-0 opacity-65".to_string() }
            }
            {leading}
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
    use dioxus::core::{AttributeValue, ElementId, Mutation};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedHtmlEventConverter, SerializedKeyboardData,
        SerializedMouseData,
    };
    use std::cell::RefCell;
    use vmux_api::BinEvent;
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct ManagerHost {
        listeners: RefCell<Vec<BytesListener>>,
        archived: RefCell<Vec<String>>,
    }

    impl PageHost for ManagerHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == crate::event::SessionsArchive::id() {
                let request =
                    rkyv::from_bytes::<crate::event::SessionsArchive, rkyv::rancor::Error>(bytes)
                        .unwrap();
                *self.archived.borrow_mut() = request.ids;
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == SessionCatalogUiState::id() {
                self.listeners.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    impl ManagerHost {
        fn load(&self) {
            let mut sessions = Vec::new();
            for index in 0..90 {
                sessions.push(SessionSummary {
                    id: format!("session-{index}"),
                    name: format!("Task {index}"),
                    stage: "active".into(),
                    runtime: "idle".into(),
                    last_activated_at: vmux_ecs::UnixMillis::now().0 - 4 * 24 * 60 * 60 * 1_000,
                    ..Default::default()
                });
            }
            let state = SessionCatalogUiState {
                loading: true,
                repositories: SessionRepositories {
                    ready: true,
                    ..Default::default()
                },
                catalog: vmux_session::CatalogSnapshot {
                    stages: vec![StageSummary {
                        id: "active".into(),
                        name: "Active".into(),
                        ..Default::default()
                    }],
                    sessions,
                },
            };
            let frame = vmux_api::UiStateFrame::<
                SessionCatalogUiState,
                crate::state::SessionCatalogUiStatePatch,
            >::Snapshot(state);
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in self.listeners.borrow_mut().iter_mut() {
                listener(&bytes);
            }
        }
    }

    struct ManagerFixture {
        dom: VirtualDom,
        host: Rc<ManagerHost>,
        collapse: ElementId,
        context: ElementId,
    }

    impl ManagerFixture {
        fn loaded() -> Self {
            dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let host = Rc::new(ManagerHost::default());
            let mut dom = VirtualDom::new(SessionsManager);
            dom.provide_root_context(HostBinding(host.clone()));
            dom.rebuild_in_place();
            dom.render_immediate_to_vec();
            assert!(dioxus_ssr::render(&dom).contains("aria-busy"));
            host.load();
            let mounted = dom.render_immediate_to_vec();
            dom.render_immediate_to_vec();
            let collapse_label = translate("common-collapse");
            let collapse = mounted
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &collapse_label => Some(*id),
                    _ => None,
                })
                .expect("the loaded group header must be collapsible");
            let section_label = translate_humanized("Stale");
            let context = mounted
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &section_label => Some(*id),
                    _ => None,
                })
                .expect("the group header must have a context menu");
            Self {
                dom,
                host,
                collapse,
                context,
            }
        }
    }

    #[test]
    fn closing_a_loaded_group_during_refresh_does_not_reintroduce_loading_placeholders() {
        let mut fixture = ManagerFixture::loaded();
        assert!(!dioxus_ssr::render(&fixture.dom).contains("aria-busy"));
        fixture.dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            fixture.collapse,
        );
        fixture.dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(&fixture.dom);
        assert!(!html.contains("aria-busy"));
        assert!(html.contains(&translate("common-expand")));
    }

    #[test]
    fn a_group_header_context_menu_archives_the_entire_group_beyond_the_visible_page() {
        let mut fixture = ManagerFixture::loaded();
        fixture.dom.runtime().handle_event(
            "contextmenu",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            fixture.context,
        );
        let opened = fixture.dom.render_immediate_to_vec();
        let archive_label = translate("sessions-archive-all");
        let archive = opened
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &archive_label => Some(*id),
                _ => None,
            })
            .expect("right clicking the section must offer bulk archive");
        let keyboard = SerializedKeyboardData::new(
            Key::Enter,
            Code::Enter,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        );
        fixture.dom.runtime().handle_event(
            "keydown",
            Event::new(EventData::Keyboard(keyboard).into_any(), true),
            archive,
        );
        let archived = fixture.host.archived.borrow();
        assert_eq!(archived.len(), 90);
        for index in 0..90 {
            assert!(archived.contains(&format!("session-{index}")));
        }
    }

    #[test]
    fn project_activity_filters_include_nested_work_and_exclude_stale_terminal_and_sibling_projects()
     {
        let now = vmux_ecs::UnixMillis::now().0;
        let stages = vec![
            StageSummary {
                id: "active".to_string(),
                ..Default::default()
            },
            StageSummary {
                id: "queued".to_string(),
                ..Default::default()
            },
            StageSummary {
                id: "done".to_string(),
                terminal: true,
                ..Default::default()
            },
        ];
        let mut sessions = Vec::new();
        for (id, stage, cwd, activated) in [
            ("active", "active", "/work/repo/src", now),
            ("queued", "queued", "/work/repo/.worktrees/task", now),
            (
                "stale",
                "active",
                "/work/repo",
                now - 4 * 24 * 60 * 60 * 1_000,
            ),
            ("done", "done", "/work/repo", now),
            ("sibling", "active", "/work/repo-other", now),
        ] {
            sessions.push(SessionSummary {
                id: id.to_string(),
                stage: stage.to_string(),
                cwd: cwd.to_string(),
                runtime: "idle".to_string(),
                last_activated_at: activated,
                ..Default::default()
            });
        }
        let groups = StageGroup::from_catalog(&stages, &sessions, "");
        let filters = [
            SessionFilter::Project("/work/repo".to_string()),
            SessionFilter::Current,
        ];
        let filtered = StageGroup::filtered(&groups, &filters, &SessionRepositories::default());
        let ids = filtered
            .iter()
            .flat_map(|group| group.sessions.iter().map(|session| session.id.as_str()))
            .collect::<HashSet<_>>();
        assert_eq!(ids, HashSet::from(["active", "queued"]));
    }

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
