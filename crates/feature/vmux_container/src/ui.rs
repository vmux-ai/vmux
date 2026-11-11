#![allow(non_snake_case)]

use std::collections::BTreeMap;

use dioxus::prelude::*;
use vmux_api::BuiltinIcon;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::input::Input;
use vmux_ui::components::manager::{ManagerIconButton, WorkspaceLayout};
use vmux_ui::components::motion::{Collapsible, ContentTransition};
use vmux_ui::components::page::PageShell;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::components::progress::LoadingIcon;
use vmux_ui::components::sheet::SheetHeaderChevron;
use vmux_ui::components::tree_row::TreeRowAccent;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::{
    ContainerCommandRequest, ContainerEntry, ContainerInspectRequest, ContainerLogsRequest,
    ContainerOperation, ContainerPort, ContainerSettingsRequest, ContainerStartRequest,
    ContainerStopRequest, ContainerUiState,
};

#[vmux_page::page(component = Page, states = ["PageAddressUiState", "ThemeUiState", "ContainerUiState"])]
pub(crate) struct ContainerPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<ContainerUiState>().state;
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
    rsx! {
        PageShell {
            WorkspaceLayout {
                resizable: true,
                sidebar: rsx! {
                    ContainerSheet { selected: selection.clone(), onselect: move |id| selected.set(id) }
                },
                toolbar: rsx! {
                    Header {
                        selection: selection.clone(), panel: panel(),
                        onpanel: move |value| panel.set(value),
                        ondelete: move |ids| deleting.set(ids),
                        onsettings: move |_| settings_open.set(true),
                    }
                },
                if !snapshot.error.is_empty() {
                    div { class: "mx-3 mb-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive",
                        ErrorText { message: snapshot.error.clone() }
                    }
                }
                div { class: "flex min-h-0 flex-1 flex-col overflow-hidden",
                    ContentTransition { key: "{selection}:{panel}", class: "flex min-h-0 flex-1 flex-col overflow-hidden",
                        if snapshot.ready && !selection.is_empty() {
                            Details {
                                selection, entries: snapshot.containers.clone(),
                                panel: panel(), onselect: move |id| selected.set(id),
                            }
                        } else {
                            Empty { title: translate("container-no-selection"), description: String::new() }
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
fn ContainerSheet(selected: String, onselect: EventHandler<String>) -> Element {
    let state = use_ui_state::<ContainerUiState>().state;
    let snapshot = state();
    let initialized = !snapshot.data_directory.is_empty();
    let starting = snapshot.starting
        || (snapshot.running
            && !snapshot.ready
            && !snapshot.stopping
            && snapshot.phase != "stopping");
    let stopping = snapshot.stopping || snapshot.phase == "stopping";
    let running = snapshot
        .containers
        .iter()
        .filter(|entry| entry.running())
        .count();
    let service = if stopping {
        "container-service-stopping"
    } else if starting {
        "container-service-starting"
    } else {
        "container-service-not-running"
    };
    let stage = if !initialized {
        "initial"
    } else if !snapshot.ready {
        service
    } else if !snapshot.inventory_ready {
        "inventory"
    } else {
        "ready"
    };
    rsx! {
        div { class: "flex h-full min-h-0 flex-col overflow-hidden bg-background font-sans text-xs text-foreground",
            div { class: "flex h-8 shrink-0 items-center gap-2 px-2.5 text-[11px] font-medium text-foreground/70",
                LineIconView { icon: LineIcon::Package, class: "size-3.5 shrink-0" }
                span { class: "min-w-0 flex-1 truncate", {translate("container-title")} }
                if snapshot.ready && snapshot.inventory_ready {
                    span { class: "text-[10px] tabular-nums text-muted-foreground/60", "{running}", " ", {translate("container-running")} }
                }
            }
            ContentTransition { key: "{stage}", class: "flex min-h-0 flex-1 flex-col",
                if !initialized {
                    ListSkeleton {}
                } else if !snapshot.supported {
                    Empty { title: translate("container-unsupported"), description: String::new() }
                } else if !snapshot.ready {
                    Empty { title: translate(service), description: String::new(), pending: starting || stopping,
                        if !starting && !stopping {
                            Button {
                                size: ButtonSize::Sm, disabled: snapshot.busy,
                                aria_label: translate("container-start-container"),
                                onclick: move |_| { let _ = send(&ContainerStartRequest); },
                                {translate("container-start-container")}
                            }
                        }
                    }
                } else if !snapshot.inventory_error.is_empty() {
                    Empty { title: translate("container-inventory-error"), description: snapshot.inventory_error.clone() }
                } else if !snapshot.inventory_ready {
                    ListSkeleton {}
                } else if snapshot.containers.is_empty() {
                    Empty { title: translate("container-empty-title"), description: String::new() }
                } else {
                    div { class: "min-h-0 flex-1 space-y-1 overflow-auto px-1 pb-3",
                        for group in Catalog(&snapshot.containers).groups() {
                            ContainerGroup { key: "{group.name}", group, selected: selected.clone(), busy: snapshot.busy, onselect }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Header(
    selection: String,
    panel: String,
    onpanel: EventHandler<String>,
    ondelete: EventHandler<Vec<String>>,
    onsettings: EventHandler<()>,
) -> Element {
    let state = use_ui_state::<ContainerUiState>().state;
    let snapshot = state();
    let mut members = Vec::new();
    let project = selection.strip_prefix("project:");
    for entry in &snapshot.containers {
        if project.is_some_and(|project| entry.project == project) || entry.id == selection {
            members.push(entry.clone());
        }
    }
    let mut address = PageAddress {
        current: AddressOption {
            label_key: "container-title".into(),
            icon: Some(BuiltinIcon::Container),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut controls = Vec::new();
    if let Some(first) = members.first() {
        address.fixed.push(AddressCrumb {
            entry: AddressOption {
                label: project.unwrap_or(&first.name).to_string(),
                icon: Some(if project.is_some() {
                    BuiltinIcon::Layers
                } else {
                    BuiltinIcon::Container
                }),
                title: first.image.clone(),
                ..Default::default()
            },
            ..Default::default()
        });
        let mut views = Vec::new();
        for (id, label) in [
            ("info", "container-info"),
            ("logs", "container-service-logs"),
        ] {
            views.push(AddressOption {
                label_key: label.into(),
                identity: id.into(),
                intent: Some(AddressIntent::Section { id: id.into() }),
                ..Default::default()
            });
        }
        address.fixed.push(AddressCrumb {
            entry: AddressOption {
                label_key: if panel == "logs" {
                    "container-service-logs"
                } else {
                    "container-info"
                }
                .into(),
                ..Default::default()
            },
            options: views,
            ..Default::default()
        });
        let mut start_ids = Vec::new();
        let mut stop_ids = Vec::new();
        let mut all_ids = Vec::new();
        for member in members {
            all_ids.push(member.id.clone());
            if member.running() {
                stop_ids.push(member.id);
            } else {
                start_ids.push(member.id);
            }
        }
        if !start_ids.is_empty() {
            controls.push(
                ToolbarControl::button(translate("container-start-container"), move |_| {
                    ContainerCommandRequest::submit(start_ids.clone(), ContainerOperation::Start)
                })
                .text()
                .disabled(snapshot.busy),
            );
        }
        if !stop_ids.is_empty() {
            controls.push(
                ToolbarControl::button(translate("container-stop-container"), move |_| {
                    ContainerCommandRequest::submit(stop_ids.clone(), ContainerOperation::Stop)
                })
                .text()
                .disabled(snapshot.busy),
            );
            controls.push(
                ToolbarControl::button(translate("container-restart"), move |_| {
                    ContainerCommandRequest::submit(all_ids.clone(), ContainerOperation::Restart)
                })
                .icon(LineIcon::RotateCcw)
                .disabled(snapshot.busy),
            );
        } else {
            controls.push(
                ToolbarControl::button(translate("container-delete"), move |_| {
                    ondelete.call(all_ids.clone())
                })
                .icon(LineIcon::Trash)
                .disabled(snapshot.busy),
            );
        }
    }
    controls.push(
        ToolbarControl::button(translate("container-settings"), move |_| {
            onsettings.call(())
        })
        .icon(LineIcon::Wrench)
        .disabled(snapshot.data_directory.is_empty()),
    );
    rsx! {
        PageBreadcrumbs { state: address,
            onintent: move |intent| { if let AddressIntent::Section { id } = intent && matches!(id.as_str(), "info" | "logs") { onpanel.call(id); } }
        }
        PageToolbar { controls }
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
    let accent = TreeRowAccent::resolve(selected == group_id, false).classes();
    rsx! {
        section { class: "space-y-0.5",
            if !group.name.is_empty() {
                div { class: "group flex h-8 items-center rounded-md px-1 {accent}",
                    button { r#type: "button", class: "flex min-w-0 flex-1 items-center gap-2 px-1 text-left text-[11px]", onclick: move |_| onselect.call(group_id.clone()),
                        LineIconView { icon: LineIcon::Package, class: "size-3.5 shrink-0 text-primary/70" }
                        span { class: "min-w-0 flex-1 truncate font-medium", "{group.name}" }
                        span { class: "text-[10px] tabular-nums text-muted-foreground/60", "{running}/{count}" }
                    }
                    SheetHeaderChevron { expanded: expanded(), label: group.name.clone(), subtle: true, onclick: move |_| expanded.set(!expanded()) }
                }
            } else {
                h2 { class: "flex h-7 items-center px-2 text-[10px] font-medium text-muted-foreground/65", {translate("container-standalone")} }
            }
            Collapsible { expanded: expanded(),
                div { class: if group.name.is_empty() { "space-y-0.5" } else { "space-y-0.5 pl-3" }, aria_hidden: !expanded(), inert: (!expanded()).then_some(""),
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
    let accent = TreeRowAccent::resolve(selected, false).classes();
    let title = if entry.service.is_empty() {
        entry.name.clone()
    } else {
        entry.service.clone()
    };
    rsx! {
        div {
            class: "group relative flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 outline-none transition-colors duration-100 {accent}",
            role: "button", tabindex: "0", aria_selected: selected, aria_label: entry.name.clone(),
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
    panel: String,
    onselect: EventHandler<String>,
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
        return rsx! { Empty { title: translate("container-no-selection"), description: String::new() } };
    }
    rsx! {
        if panel == "logs" && group.is_none() {
            div { class: "flex min-h-0 flex-1 flex-col px-4 py-3",
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
            div { class: "min-h-0 flex-1 overflow-auto px-4 py-3",
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
            div { class: "min-h-0 flex-1 overflow-auto px-4 py-3",
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
                if snapshot.running {
                    div { class: "flex items-center gap-3",
                        p { class: "min-w-0 flex-1 text-xs text-muted-foreground/70", {translate("container-settings-running")} }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, disabled: snapshot.busy, onclick: move |_| { let _ = send(&ContainerStopRequest); }, {translate("container-stop")} }
                    }
                }
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
fn Empty(
    title: String,
    description: String,
    #[props(default)] pending: bool,
    #[props(default)] children: Element,
) -> Element {
    rsx! {
        div { class: "flex min-h-0 flex-1 items-center justify-center px-4 py-8", aria_busy: pending,
            div { class: "max-w-sm text-center",
                if pending { LoadingIcon { class: "mx-auto mb-4 size-6 text-primary/65" } }
                else { LineIconView { icon: LineIcon::Package, class: "mx-auto mb-4 size-6 text-muted-foreground/35" } }
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
        "container-config-running" => translate(&message),
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
    fn groups(&self) -> Vec<Group> {
        let mut grouped = BTreeMap::<String, Vec<ContainerEntry>>::new();
        for entry in self.0 {
            grouped
                .entry(entry.project.clone())
                .or_default()
                .push(entry.clone());
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
        String::new()
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
    use std::collections::HashMap;
    use std::future::Future;
    use std::rc::Rc;
    use std::task::{Context, Waker};
    use vmux_api::address::PageAddressPublish;
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, HostScope, PageHost};

    #[derive(Default)]
    struct Host {
        listeners: RefCell<Vec<(String, BytesListener)>>,
        starts: Cell<usize>,
        addresses: RefCell<Vec<PageAddress>>,
        inspections: RefCell<Vec<ContainerInspectRequest>>,
    }

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == PageAddressPublish::id() {
                let request =
                    rkyv::from_bytes::<PageAddressPublish, rkyv::rancor::Error>(bytes).unwrap();
                self.addresses.borrow_mut().push(request.address);
            } else if id == ContainerStartRequest::id() {
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
        controls: HashMap<String, ElementId>,
        sidebar_blocked: bool,
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
            let mut preview = Self {
                host,
                dom,
                controls: HashMap::new(),
                sidebar_blocked: false,
                _scope: scope,
            };
            preview.apply(edits.edits);
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

        fn apply(&mut self, edits: Vec<Mutation>) {
            for edit in edits {
                if let Mutation::SetAttribute {
                    name: "inert",
                    value,
                    ..
                } = &edit
                {
                    self.sidebar_blocked = !matches!(value, AttributeValue::None);
                }
                if let Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(label),
                    id,
                    ..
                } = edit
                {
                    self.controls.insert(label, id);
                }
            }
        }

        fn render(&mut self) -> String {
            for _ in 0..8 {
                let mut context = Context::from_waker(Waker::noop());
                let _ = std::pin::pin!(self.dom.wait_for_work()).poll(&mut context);
                let edits = self.dom.render_immediate_to_vec();
                self.apply(edits.edits);
            }
            dioxus_ssr::render(&self.dom)
        }

        fn click(&mut self, label: &str) {
            let target = *self.controls.get(label).expect("visible control");
            self.dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                target,
            );
            self.render();
        }
    }

    #[test]
    fn stopped_service_has_one_start_control_and_no_selection_or_setup_fields() {
        let mut preview = Preview::new();
        let html = preview.publish(ContainerUiState {
            supported: true,
            data_directory: "/fixtures/data".into(),
            shared_directory: "/fixtures/projects".into(),
            cpus: 2,
            memory_mib: 2048,
            ..Default::default()
        });
        assert_eq!(html.matches("<input").count(), 0);
        assert!(html.contains(&translate("container-service-not-running")));
        assert!(html.contains(&translate("container-no-selection")));
        let start = translate("container-start-container");
        assert_eq!(html.matches(&format!("aria-label=\"{start}\"")).count(), 1);
        assert!(!html.contains(&translate("container-cpus")));
        preview.click(&start);
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
        assert!(html.contains(&translate("container-no-selection")));
        preview.click("web");
        assert!(preview.render().contains("Up 1 second"));
        let addresses = preview.host.addresses.borrow();
        let address = addresses.last().unwrap();
        assert_eq!(address.fixed[0].entry.label, "web");
        assert_eq!(address.fixed[1].entry.label_key, "container-info");
        assert_eq!(
            address.fixed[1]
                .options
                .iter()
                .map(|option| option.identity.as_str())
                .collect::<Vec<_>>(),
            ["info", "logs"]
        );
        drop(addresses);
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

    #[test]
    fn side_sheet_can_close_while_starting_and_busy_service_has_no_duplicate_start() {
        let mut preview = Preview::new();
        let html = preview.publish(ContainerUiState {
            supported: true,
            running: true,
            starting: true,
            busy: true,
            data_directory: "/fixtures/data".into(),
            ..Default::default()
        });
        assert!(html.contains("aria-busy=true"));
        assert!(!html.contains(&format!(
            "aria-label=\"{}\"",
            translate("container-start-container")
        )));
        let toggle = translate("schema-side-sheet");
        preview.click(&toggle);
        assert!(preview.sidebar_blocked);
        preview.click(&toggle);
        assert!(!preview.sidebar_blocked);
    }
}
