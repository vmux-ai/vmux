#![allow(non_snake_case)]

use std::collections::BTreeMap;

use dioxus::prelude::*;
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::input::Input;
use vmux_ui::components::manager::ManagerIconButton;
use vmux_ui::components::page::{PageShell, PageToolbarRow};
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::{
    ContainerCommandRequest, ContainerEntry, ContainerInspectRequest, ContainerLogsRequest,
    ContainerOperation, ContainerPort, ContainerProjectOpenRequest, ContainerSettingsRequest,
    ContainerStartRequest, ContainerStopRequest, ContainerUiState,
};

#[vmux_page::page(component = Page, states = ["PageAddressUiState", "ThemeUiState", "ContainerUiState"])]
pub(crate) struct ContainerPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ContainerUiState>().state;
    let mut search = use_signal(String::new);
    let mut selected = use_signal(String::new);
    let mut settings_open = use_signal(|| false);
    let mut deleting = use_signal(Vec::<String>::new);
    let mut panel = use_signal(|| "info".to_string());
    let mut inspected = use_signal(|| None::<(String, bool)>);
    use_effect(move || {
        let snapshot = state();
        let selection = Catalog(&snapshot.containers).selection(&selected());
        let id = if selection.starts_with("project:") {
            String::new()
        } else {
            selection
        };
        let request = (id, panel() == "logs");
        if inspected.peek().as_ref() == Some(&request) {
            return;
        }
        inspected.set(Some(request.clone()));
        let _ = send(&ContainerInspectRequest {
            id: request.0,
            logs: request.1,
        });
    });
    let snapshot = state();
    let catalog = Catalog(&snapshot.containers);
    let selection = catalog.selection(&selected());
    let groups = catalog.groups(&search());
    let running = snapshot
        .containers
        .iter()
        .filter(|entry| entry.running())
        .count();
    let initialized = !snapshot.data_directory.is_empty();
    let phase_label = if snapshot.stopping {
        translate("container-phase-stopping")
    } else if snapshot.ready {
        translate("container-docker-running")
    } else if snapshot.starting || (snapshot.running && snapshot.phase != "stopping") {
        translate("container-docker-starting")
    } else if snapshot.phase == "stopping" {
        translate("container-phase-stopping")
    } else {
        translate("container-docker-stopped")
    };
    rsx! {
        PageShell {
            PageToolbarRow { class: "gap-2 px-3",
                LineIconView { icon: LineIcon::Package, class: "size-3.5" }
                span { class: "font-medium text-foreground", {translate("container-title")} }
                if snapshot.ready && snapshot.inventory_ready {
                    span { class: "text-[10px] text-muted-foreground/70", "{running} ", {translate("container-running")} }
                }
                div { class: "flex-1" }
                StateDot { state: if snapshot.ready { "running" } else if snapshot.busy { "restarting" } else { "exited" } }
                span { class: "mr-1 text-[11px]", "{phase_label}" }
                Button {
                    size: ButtonSize::Sm, variant: ButtonVariant::Ghost,
                    aria_label: translate(if snapshot.running { "container-stop" } else { "container-start" }),
                    disabled: !snapshot.supported || snapshot.busy || !initialized,
                    onclick: move |_| {
                        if state().running { let _ = send(&ContainerStopRequest); }
                        else { let _ = send(&ContainerStartRequest); }
                    },
                    {translate(if snapshot.running { "container-stop" } else { "container-start" })}
                }
                Button {
                    size: ButtonSize::Sm, disabled: !snapshot.supported || snapshot.busy || !initialized,
                    onclick: move |_| { let _ = send(&ContainerProjectOpenRequest); },
                    LineIconView { icon: LineIcon::Plus, class: "mr-1 size-3" }
                    {translate(if snapshot.project_starting { "container-project-starting" } else { "container-open-project" })}
                }
                ManagerIconButton {
                    label: translate("container-settings"),
                    onclick: move |_| settings_open.set(true),
                    LineIconView { icon: LineIcon::Wrench, class: "size-3.5" }
                }
            }
            if !snapshot.error.is_empty() {
                div { class: "mx-3 mb-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive",
                    ErrorText { message: snapshot.error.clone() }
                }
            }
            div { class: "flex min-h-0 flex-1 overflow-hidden",
                aside { class: "flex w-72 max-w-[40%] shrink-0 flex-col bg-foreground/[0.018]",
                    div { class: "px-3 pb-2 pt-3",
                        Input {
                            r#type: "search", class: "w-full text-xs", value: search(),
                            placeholder: translate("container-search"),
                            oninput: move |event: FormEvent| search.set(event.value()),
                        }
                    }
                    div { class: "min-h-0 flex-1 space-y-2 overflow-auto px-2 pb-3",
                        if initialized && snapshot.ready && snapshot.inventory_ready {
                            for group in groups {
                                ContainerGroup {
                                    key: "{group.name}", group, selected: selection.clone(), busy: snapshot.busy,
                                    onselect: move |id| selected.set(id),
                                }
                            }
                            if snapshot.containers.is_empty() {
                                p { class: "px-2 py-3 text-xs text-muted-foreground/65", {translate("container-empty-title")} }
                            } else if !search().is_empty() && Catalog(&snapshot.containers).groups(&search()).is_empty() {
                                p { class: "px-2 py-3 text-xs text-muted-foreground/65", {translate("container-no-results")} }
                                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| search.set(String::new()), {translate("container-clear-search")} }
                            }
                        } else if !initialized || snapshot.ready {
                            ListSkeleton {}
                        }
                    }
                }
                section { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                    if !snapshot.supported && initialized {
                        Empty { title: translate("container-unsupported"), description: String::new() }
                    } else if !initialized || (snapshot.ready && !snapshot.inventory_ready && snapshot.inventory_error.is_empty()) {
                        div { class: "space-y-4 p-6",
                            div { class: "h-5 w-40 animate-pulse rounded bg-muted/40" }
                            div { class: "h-24 animate-pulse rounded bg-muted/25" }
                        }
                    } else if !snapshot.ready {
                        Empty { title: translate(if snapshot.starting { "container-docker-starting" } else { "container-off-title" }), description: translate(if snapshot.starting { "container-starting-description" } else { "container-off-description" }),
                            Button { size: ButtonSize::Sm, disabled: snapshot.busy, onclick: move |_| { let _ = send(&ContainerProjectOpenRequest); }, {translate("container-open-project")} }
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, disabled: snapshot.busy, onclick: move |_| { let _ = send(&ContainerStartRequest); }, {translate("container-start")} }
                        }
                    } else if !snapshot.inventory_error.is_empty() {
                        Empty { title: translate("container-inventory-error"), description: snapshot.inventory_error.clone() }
                    } else if snapshot.containers.is_empty() {
                        Empty { title: translate(if snapshot.project_starting { "container-project-starting" } else { "container-empty-title" }), description: translate(if snapshot.project_starting { "container-project-starting-description" } else { "container-empty-description" }),
                            Button { size: ButtonSize::Sm, disabled: snapshot.busy, onclick: move |_| { let _ = send(&ContainerProjectOpenRequest); }, {translate("container-open-project")} }
                        }
                    } else {
                        Details {
                            selection, entries: snapshot.containers.clone(), busy: snapshot.busy,
                            panel: panel(), onpanel: move |value| panel.set(value),
                            onselect: move |id| selected.set(id), ondelete: move |ids| deleting.set(ids),
                        }
                    }
                }
            }
            if settings_open() { Preferences { onclose: move |_| settings_open.set(false) } }
            if !deleting().is_empty() {
                DeleteDialog { ids: deleting(), onclose: move |_| deleting.set(Vec::new()) }
            }
        }
    }
}

#[component]
fn ContainerGroup(
    group: Group,
    selected: String,
    busy: bool,
    onselect: EventHandler<String>,
) -> Element {
    let mut expanded = use_signal(|| true);
    let group_id = format!("project:{}", group.name);
    let running = group.entries.iter().filter(|entry| entry.running()).count();
    let count = group.entries.len();
    rsx! {
        section { class: "space-y-0.5",
            if !group.name.is_empty() {
                div { class: if selected == group_id { "flex items-center rounded-md bg-primary/10 text-foreground" } else { "flex items-center rounded-md text-muted-foreground hover:bg-foreground/[0.04]" },
                    button { r#type: "button", class: "flex size-7 items-center justify-center", aria_label: group.name.clone(), aria_expanded: expanded(), onclick: move |_| expanded.set(!expanded()),
                        LineIconView { icon: LineIcon::ChevronDown, class: if expanded() { "size-3" } else { "size-3 -rotate-90" } }
                    }
                    button { r#type: "button", class: "flex min-w-0 flex-1 items-center gap-2 py-2 pr-2 text-left text-xs", onclick: move |_| onselect.call(group_id.clone()),
                        LineIconView { icon: LineIcon::Package, class: "size-3.5 shrink-0 text-primary/70" }
                        span { class: "min-w-0 flex-1 truncate font-medium", "{group.name}" }
                        span { class: "text-[10px] tabular-nums text-muted-foreground/60", "{running}/{count}" }
                    }
                }
            } else {
                h2 { class: "px-2 py-1 text-[10px] text-muted-foreground/60", {translate("container-standalone")} }
            }
            if expanded() {
                div { class: if group.name.is_empty() { "space-y-0.5" } else { "space-y-0.5 pl-4" },
                    for entry in group.entries {
                        ContainerRow { key: "{entry.id}", selected: selected == entry.id, entry, busy, onselect }
                    }
                }
            }
        }
    }
}

#[component]
fn ContainerRow(
    entry: ContainerEntry,
    selected: bool,
    busy: bool,
    onselect: EventHandler<String>,
) -> Element {
    let row_id = entry.id.clone();
    let keyboard_id = entry.id.clone();
    let operation_id = entry.id.clone();
    let running = entry.running();
    let title = if entry.service.is_empty() {
        entry.name.clone()
    } else {
        entry.service.clone()
    };
    rsx! {
        div {
            class: if selected { "group flex items-center gap-2 rounded-md bg-primary/10 px-2 py-2" } else { "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-2 hover:bg-foreground/[0.04]" },
            role: "button", tabindex: "0", aria_selected: selected,
            onclick: move |_| onselect.call(row_id.clone()),
            onkeydown: move |event| { if event.key() == Key::Enter || event.key() == Key::Character(" ".into()) { event.prevent_default(); onselect.call(keyboard_id.clone()); } },
            StateDot { state: entry.state.clone() }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-xs font-medium text-foreground/90", "{title}" }
                div { class: "mt-0.5 truncate text-[10px] text-muted-foreground/60", "{entry.image}" }
            }
            div { class: "opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100",
                ManagerIconButton {
                    label: translate(if running { "container-stop-container" } else { "container-start-container" }), disabled: busy,
                    onclick: move |event: MouseEvent| { event.stop_propagation(); ContainerCommandRequest::submit(vec![operation_id.clone()], if running { ContainerOperation::Stop } else { ContainerOperation::Start }); },
                    RunIcon { running }
                }
            }
        }
    }
}

#[component]
fn Details(
    selection: String,
    entries: Vec<ContainerEntry>,
    busy: bool,
    panel: String,
    onpanel: EventHandler<String>,
    onselect: EventHandler<String>,
    ondelete: EventHandler<Vec<String>>,
) -> Element {
    let state = use_ui_state::<ContainerUiState>().state;
    let snapshot = state();
    let group = selection.strip_prefix("project:").map(str::to_string);
    let mut members = Vec::new();
    for entry in entries {
        if group.as_ref().is_some_and(|group| entry.project == *group) || entry.id == selection {
            members.push(entry);
        }
    }
    if members.is_empty() {
        return rsx! { Empty { title: translate("container-select"), description: translate("container-select-description") } };
    }
    let title = match &group {
        Some(group) => group.clone(),
        None => members[0].name.clone(),
    };
    let subtitle = if group.is_some() {
        translate("container-project")
    } else {
        members[0].image.clone()
    };
    let mut start_ids = Vec::new();
    let mut stop_ids = Vec::new();
    let mut all_ids = Vec::new();
    for member in &members {
        all_ids.push(member.id.clone());
        if member.running() {
            stop_ids.push(member.id.clone());
        } else {
            start_ids.push(member.id.clone());
        }
    }
    let restart_ids = all_ids.clone();
    let delete_ids = all_ids;
    let stopped = stop_ids.is_empty();
    let empty_start = start_ids.is_empty();
    let tabs = vec![
        SegmentedControlItem {
            id: "info".into(),
            label: translate("container-info"),
        },
        SegmentedControlItem {
            id: "logs".into(),
            label: translate("container-service-logs"),
        },
    ];
    rsx! {
        div { class: "shrink-0 px-5 pb-3 pt-5",
            div { class: "flex items-start gap-3",
                LineIconView { icon: LineIcon::Package, class: "mt-1 size-5 text-primary/70" }
                div { class: "min-w-0 flex-1",
                    h1 { class: "truncate text-base font-semibold", "{title}" }
                    p { class: "mt-1 truncate text-xs text-muted-foreground/65", "{subtitle}" }
                }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, disabled: busy || empty_start, onclick: move |_| ContainerCommandRequest::submit(start_ids.clone(), ContainerOperation::Start), {translate("container-start-container")} }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, disabled: busy || stopped, onclick: move |_| ContainerCommandRequest::submit(stop_ids.clone(), ContainerOperation::Stop), {translate("container-stop-container")} }
                ManagerIconButton { label: translate("container-restart"), disabled: busy, onclick: move |_| ContainerCommandRequest::submit(restart_ids.clone(), ContainerOperation::Restart), LineIconView { icon: LineIcon::RotateCcw, class: "size-3.5" } }
                ManagerIconButton { label: translate("container-delete"), disabled: busy || !stopped, onclick: move |_| ondelete.call(delete_ids.clone()), LineIconView { icon: LineIcon::Trash, class: "size-3.5" } }
            }
            div { class: "mt-4",
                SegmentedControl { active: panel.clone(), items: tabs, onselect: move |value| onpanel.call(value) }
            }
        }
        if panel == "logs" && group.is_none() {
            div { class: "flex min-h-0 flex-1 flex-col px-5 pb-5",
                div { class: "mb-2 flex shrink-0 items-center gap-2 text-[10px] text-muted-foreground/65",
                    StateDot { state: "running" }
                    span { {translate("container-live")} }
                    div { class: "flex-1" }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| { let _ = send(&ContainerInspectRequest { id: selection.clone(), logs: true }); }, {translate("container-refresh-logs")} }
                }
                if snapshot.log_id != members[0].id || !snapshot.container_logs_ready {
                    div { class: "flex-1 space-y-2 rounded-md bg-foreground/[0.025] p-3",
                        div { class: "h-3 w-3/4 animate-pulse rounded bg-muted/40" }
                        div { class: "h-3 w-1/2 animate-pulse rounded bg-muted/30" }
                    }
                } else if !snapshot.container_log_error.is_empty() {
                    p { class: "text-xs text-destructive", "{snapshot.container_log_error}" }
                } else if snapshot.container_logs.is_empty() {
                    p { class: "rounded-md bg-foreground/[0.025] p-4 text-xs text-muted-foreground/60", {translate("container-log-empty")} }
                } else {
                    pre { class: "min-h-0 flex-1 overflow-auto rounded-md bg-foreground/[0.025] p-3 font-mono text-[11px] leading-relaxed text-foreground/80", "{snapshot.container_logs}" }
                }
            }
        } else if group.is_some() {
            div { class: "min-h-0 flex-1 overflow-auto px-5 pb-5",
                if panel == "logs" { p { class: "mb-3 text-xs text-muted-foreground/65", {translate("container-group-logs")} } }
                div { class: "space-y-1 rounded-lg bg-foreground/[0.025] p-2",
                    for member in members {
                        button { key: "{member.id}", r#type: "button", class: "flex w-full items-center gap-3 rounded-md px-2 py-3 text-left hover:bg-foreground/[0.04]", onclick: move |_| onselect.call(member.id.clone()),
                            StateDot { state: member.state.clone() }
                            span { class: "min-w-0 flex-1 truncate text-xs", "{member.service}" }
                            span { class: "truncate text-[10px] text-muted-foreground/60", "{member.image}" }
                        }
                    }
                }
            }
        } else {
            div { class: "min-h-0 flex-1 overflow-auto px-5 pb-5",
                dl { class: "space-y-5",
                    Info { label: translate("container-status"),
                        StateLabel { state: members[0].state.clone() }
                        span { class: "ml-2 text-[11px] text-muted-foreground/55", "{members[0].status}" }
                    }
                    Info { label: translate("container-image"), code { class: "break-all text-xs", "{members[0].image}" } }
                    if !members[0].project.is_empty() {
                        Info { label: translate("container-project"), span { "{members[0].project}" } }
                        Info { label: translate("container-service"), span { "{members[0].service}" } }
                    }
                    Info { label: translate("container-ports"), PortLinks { ports: members[0].ports.clone() } }
                }
            }
        }
    }
}

#[component]
fn Preferences(onclose: EventHandler<()>) -> Element {
    let state = use_ui_state::<ContainerUiState>().state;
    let initial = state.peek();
    let mut cpus = use_signal(|| initial.cpus.to_string());
    let mut memory = use_signal(|| initial.memory_mib.to_string());
    let mut shared = use_signal(|| initial.shared_directory.clone());
    let mut submitted = use_signal(|| None::<u64>);
    use_effect(move || {
        if let Some(revision) = submitted()
            && state().configuration_revision >= revision
        {
            onclose.call(());
        }
    });
    let snapshot = state();
    let locked = snapshot.running || snapshot.busy;
    let valid = cpus().parse::<u8>().is_ok_and(|value| value > 0)
        && memory().parse::<u32>().is_ok_and(|value| value >= 512)
        && !shared().trim().is_empty();
    rsx! {
        DialogRoot { open: true, on_open_change: move |next: bool| { if !next { onclose.call(()); } }, attributes: vec![],
            DialogContent { class: "max-w-[480px] p-5", attributes: vec![],
                DialogTitle { attributes: vec![], {translate("container-settings")} }
                if snapshot.running { p { class: "text-xs text-muted-foreground/70", {translate("container-settings-running")} } }
                div { class: "grid grid-cols-2 gap-4",
                    label { class: "space-y-1.5", span { class: "text-xs", {translate("container-cpus")} }
                        Input { r#type: "number", class: "w-full", min: "1", value: cpus(), disabled: locked, oninput: move |event: FormEvent| cpus.set(event.value()) }
                    }
                    label { class: "space-y-1.5", span { class: "text-xs", {translate("container-memory")} }
                        Input { r#type: "number", class: "w-full", min: "512", step: "256", value: memory(), disabled: locked, oninput: move |event: FormEvent| memory.set(event.value()) }
                    }
                }
                label { class: "space-y-1.5", span { class: "text-xs", {translate("container-shared-directory")} }
                    Input { class: "w-full text-xs", value: shared(), disabled: locked, oninput: move |event: FormEvent| shared.set(event.value()) }
                }
                if !snapshot.error.is_empty() { p { class: "text-xs text-destructive", ErrorText { message: snapshot.error.clone() } } }
                details { class: "text-xs text-muted-foreground/70",
                    summary { class: "cursor-pointer py-2", {translate("container-connection")} }
                    code { class: "block break-all rounded-md bg-foreground/[0.04] p-2 text-[10px]", "unix://{snapshot.socket}" }
                    div { class: "mt-2 flex justify-end", Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| { let _ = send(&ContainerLogsRequest); }, {translate("container-refresh-logs")} } }
                    if !snapshot.logs.is_empty() { pre { class: "mt-2 max-h-40 overflow-auto whitespace-pre-wrap text-[10px]", "{snapshot.logs}" } }
                }
                div { class: "flex justify-end gap-2",
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), {translate("container-cancel")} }
                    Button { size: ButtonSize::Sm, disabled: locked || !valid,
                        onclick: move |_| {
                            submitted.set(Some(state().configuration_revision + 1));
                            let _ = send(&ContainerSettingsRequest { shared_directory: shared(), cpus: cpus().parse().unwrap_or(0), memory_mib: memory().parse().unwrap_or(0) });
                        },
                        {translate("container-save")}
                    }
                }
            }
        }
    }
}

#[component]
fn DeleteDialog(ids: Vec<String>, onclose: EventHandler<()>) -> Element {
    rsx! {
        DialogRoot { open: true, on_open_change: move |next: bool| { if !next { onclose.call(()); } }, attributes: vec![],
            DialogContent { class: "max-w-[420px] p-5", attributes: vec![],
                DialogTitle { attributes: vec![], {translate("container-delete-title")} }
                DialogDescription { attributes: vec![], {translate("container-delete-description")} }
                div { class: "flex justify-end gap-2",
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), {translate("container-cancel")} }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Destructive, onclick: move |_| { ContainerCommandRequest::submit(ids.clone(), ContainerOperation::Remove); onclose.call(()); }, {translate("container-delete")} }
                }
            }
        }
    }
}

#[component]
fn PortLinks(ports: Vec<ContainerPort>) -> Element {
    if ports.is_empty() {
        return rsx! { span { class: "text-xs text-muted-foreground/55", {translate("container-no-ports")} } };
    }
    rsx! {
        div { class: "flex flex-wrap gap-2",
            for port in ports {
                if port.protocol == "tcp" {
                    a { href: "http://localhost:{port.host}", class: "inline-flex items-center gap-1.5 rounded-md bg-foreground/[0.045] px-2 py-1 text-xs text-primary hover:bg-primary/10",
                        onclick: move |event| { event.prevent_default(); let _ = send(&OpenRequest { value: format!("http://localhost:{}", port.host), open: Some(OpenTarget::InNewTab) }); },
                        "localhost:{port.host}", LineIconView { icon: LineIcon::ExternalLink, class: "size-3" }
                    }
                } else { code { class: "text-xs", "{port.host} → {port.guest}/{port.protocol}" } }
            }
        }
    }
}

#[component]
fn Info(label: String, children: Element) -> Element {
    rsx! { div { dt { class: "mb-1.5 text-[10px] text-muted-foreground/60", "{label}" } dd { class: "text-xs text-foreground/85", {children} } } }
}

#[component]
fn Empty(title: String, description: String, #[props(default)] children: Element) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 items-center justify-center px-6 py-10",
            div { class: "max-w-sm text-center",
                LineIconView { icon: LineIcon::Package, class: "mx-auto mb-4 size-8 text-muted-foreground/35" }
                h2 { class: "text-sm font-medium text-foreground/85", "{title}" }
                if !description.is_empty() { p { class: "mt-2 text-xs leading-relaxed text-muted-foreground/65", "{description}" } }
                div { class: "mt-5 flex justify-center gap-2", {children} }
            }
        }
    }
}

#[component]
fn ListSkeleton() -> Element {
    rsx! { div { class: "space-y-3 p-2",
        for index in 0..4 { div { key: "{index}", class: "space-y-2", div { class: "h-3 w-3/4 animate-pulse rounded bg-muted/40" } div { class: "h-2 w-1/2 animate-pulse rounded bg-muted/25" } } }
    } }
}

#[component]
fn StateDot(state: String) -> Element {
    let class = match state.as_str() {
        "running" => "bg-success",
        "paused" => "bg-amber-400",
        "restarting" => "animate-pulse bg-primary",
        "dead" => "bg-destructive",
        _ => "bg-muted-foreground/35",
    };
    rsx! { span { class: "size-1.5 shrink-0 rounded-full {class}", aria_hidden: true } }
}

#[component]
fn StateLabel(state: String) -> Element {
    let key = match state.as_str() {
        "running" | "paused" | "exited" | "created" | "restarting" | "dead" | "removing" => {
            format!("container-state-{state}")
        }
        _ => "container-state-unknown".to_string(),
    };
    rsx! { span { class: "inline-flex items-center gap-2", StateDot { state } {translate(&key)} } }
}

#[component]
fn RunIcon(running: bool) -> Element {
    rsx! { svg { class: "size-3", view_box: "0 0 16 16", fill: "currentColor", "aria-hidden": "true",
        if running { rect { x: "4", y: "4", width: "8", height: "8", rx: "1" } }
        else { path { d: "M5 3v10l8-5z" } }
    } }
}

#[component]
fn ErrorText(message: String) -> Element {
    let text = match message.as_str() {
        "container-project-invalid"
        | "container-project-not-shared"
        | "container-config-running" => translate(&message),
        _ => message,
    };
    rsx! { span { "{text}" } }
}

#[derive(Clone, PartialEq)]
struct Group {
    name: String,
    entries: Vec<ContainerEntry>,
}

struct Catalog<'a>(&'a [ContainerEntry]);

impl Catalog<'_> {
    fn groups(&self, search: &str) -> Vec<Group> {
        let mut grouped = BTreeMap::<String, Vec<ContainerEntry>>::new();
        for entry in self.0 {
            if entry.matches(search) {
                grouped
                    .entry(entry.project.clone())
                    .or_default()
                    .push(entry.clone());
            }
        }
        let standalone = grouped.remove("");
        let mut groups = Vec::new();
        for (name, entries) in grouped {
            groups.push(Group { name, entries });
        }
        if let Some(entries) = standalone {
            groups.push(Group {
                name: String::new(),
                entries,
            });
        }
        groups
    }

    fn selection(&self, selected: &str) -> String {
        if let Some(project) = selected.strip_prefix("project:") {
            if self.0.iter().any(|entry| entry.project == project) {
                return selected.to_string();
            }
        } else if self.0.iter().any(|entry| entry.id == selected) {
            return selected.to_string();
        }
        match self.0.first() {
            Some(entry) if !entry.project.is_empty() => format!("project:{}", entry.project),
            Some(entry) => entry.id.clone(),
            None => String::new(),
        }
    }
}

impl ContainerCommandRequest {
    fn submit(ids: Vec<String>, operation: ContainerOperation) {
        let _ = send(&Self { ids, operation });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, ElementId, Mutation, VirtualDom};
    use dioxus::html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::rc::Rc;
    use std::task::{Context, Waker};
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, HostScope, PageHost};

    #[derive(Default)]
    struct Host {
        listeners: RefCell<Vec<(String, BytesListener)>>,
        starts: Cell<usize>,
        inspections: RefCell<Vec<ContainerInspectRequest>>,
    }

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == ContainerStartRequest::id() {
                let _ =
                    rkyv::from_bytes::<ContainerStartRequest, rkyv::rancor::Error>(bytes).unwrap();
                self.starts.set(self.starts.get() + 1);
            } else if id == ContainerInspectRequest::id() {
                self.inspections.borrow_mut().push(
                    rkyv::from_bytes::<ContainerInspectRequest, rkyv::rancor::Error>(bytes)
                        .unwrap(),
                );
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            self.listeners.borrow_mut().push((id.to_string(), listener));
            Ok(())
        }
    }

    struct Preview {
        host: Rc<Host>,
        dom: VirtualDom,
        start: ElementId,
        _scope: HostScope,
    }

    impl Preview {
        fn new() -> Self {
            dioxus::html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let host = Rc::new(Host::default());
            let scope = HostScope::enter(host.clone());
            let mut dom = VirtualDom::new(Page);
            dom.provide_root_context(HostBinding(host.clone()));
            let edits = dom.rebuild_to_vec();
            let label = translate("container-start");
            let start = edits
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &label => Some(*id),
                    _ => None,
                })
                .expect("start Docker button");
            let mut preview = Self {
                host,
                dom,
                start,
                _scope: scope,
            };
            preview.render();
            preview
        }

        fn publish(&mut self, state: ContainerUiState) -> String {
            let frame = UiStateFrame::<ContainerUiState, ContainerUiState>::Snapshot(state);
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            let mut found = false;
            for (id, listener) in self.host.listeners.borrow_mut().iter_mut() {
                if id == ContainerUiState::id() {
                    listener(&bytes);
                    found = true;
                }
            }
            assert!(found, "the page must subscribe to its snapshot");
            self.render()
        }

        fn render(&mut self) -> String {
            for _ in 0..8 {
                let mut context = Context::from_waker(Waker::noop());
                let _ = std::pin::pin!(self.dom.wait_for_work()).poll(&mut context);
                self.dom.render_immediate_to_vec();
            }
            dioxus_ssr::render(&self.dom)
        }

        fn click_start(&mut self) {
            self.dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                self.start,
            );
            self.render();
        }
    }

    #[test]
    fn stopped_dashboard_starts_docker_without_presenting_resource_fields() {
        let mut preview = Preview::new();
        let html = preview.publish(ContainerUiState {
            supported: true,
            data_directory: "/fixtures/data".into(),
            shared_directory: "/fixtures/projects".into(),
            cpus: 2,
            memory_mib: 2048,
            ..Default::default()
        });
        assert_eq!(html.matches("type=\"search\"").count(), 1);
        assert_eq!(html.matches("<input").count(), 1);
        assert!(html.contains(&translate("container-open-project")));
        assert!(!html.contains(&translate("container-cpus")));
        preview.click_start();
        assert_eq!(preview.host.starts.get(), 1);
    }

    #[test]
    fn inventory_updates_preserve_selection_without_reissuing_log_selection() {
        let mut preview = Preview::new();
        let mut snapshot = ContainerUiState {
            supported: true,
            running: true,
            ready: true,
            inventory_ready: true,
            data_directory: "/fixtures/data".into(),
            containers: vec![ContainerEntry {
                id: "eeeeeeeeeeee".into(),
                name: "web".into(),
                image: "nginx:alpine".into(),
                state: "running".into(),
                status: "Up 1 second".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let html = preview.publish(snapshot.clone());
        assert!(html.contains("nginx:alpine"));
        let requests = preview.host.inspections.borrow().len();
        snapshot.containers[0].status = "Up 2 seconds".into();
        snapshot.container_logs_ready = true;
        snapshot.log_id = "eeeeeeeeeeee".into();
        snapshot.container_logs = "new log output".into();
        let html = preview.publish(snapshot);
        assert!(html.contains("Up 2 seconds"));
        assert_eq!(preview.host.inspections.borrow().len(), requests);
        assert_eq!(
            preview.host.inspections.borrow().last().unwrap().id,
            "eeeeeeeeeeee"
        );
    }
}
