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
    let controls = Controls::use_state(state);
    use_context_provider(|| controls);
    use_effect(move || controls.reconcile());
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
    let snapshot = controls.snapshot();
    let catalog = Catalog(&snapshot.containers);
    let available = snapshot.available();
    let selection = if available {
        catalog.selection(&selected())
    } else {
        String::new()
    };
    let transition = if available {
        format!("{selection}:{}", panel())
    } else {
        format!("service:{}", snapshot.phase)
    };
    rsx! {
        PageShell {
            WorkspaceLayout {
                resizable: true,
                show_sidebar: available,
                sidebar: rsx! {
                    ContainerSheet { selected: selection.clone(), onselect: move |id| selected.set(id), onsettings: move |_| settings_open.set(true) }
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
                    ContentTransition { key: "{transition}", class: "flex min-h-0 flex-1 flex-col overflow-hidden",
                        if !available {
                            Service {}
                        } else if !selection.is_empty() {
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
fn Service() -> Element {
    let controls = Controls::current();
    let snapshot = controls.snapshot();
    if snapshot.data_directory.is_empty() {
        return rsx! { Empty { title: translate("container-engine"), description: String::new(), pending: true } };
    }
    if !snapshot.supported {
        return rsx! { Empty { title: translate("container-unsupported"), description: String::new() } };
    }
    let starting = snapshot.starting
        || (snapshot.running
            && !snapshot.ready
            && !snapshot.stopping
            && snapshot.phase != "stopping");
    let stopping = snapshot.stopping || snapshot.phase == "stopping";
    let service = if stopping {
        "container-service-stopping"
    } else if starting {
        "container-docker-starting"
    } else {
        "container-docker-stopped"
    };
    rsx! {
        div { class: "flex min-h-0 flex-1 items-center justify-center", aria_busy: starting || stopping,
            div { class: "flex flex-col items-center gap-6",
                VmIllustration {}
                if starting || stopping {
                    div { class: "flex h-8 items-center gap-2 text-xs text-muted-foreground",
                        LoadingIcon { class: "size-4" }
                        span { {translate(service)} }
                    }
                } else {
                Button {
                    size: ButtonSize::Sm, disabled: snapshot.busy,
                    title: translate("container-start"),
                    aria_label: translate("container-start"),
                    onclick: move |_| controls.request(Intent::Start),
                    {translate("container-start-container")}
                }
                }
            }
        }
    }
}

#[component]
fn ContainerSheet(
    selected: String,
    onselect: EventHandler<String>,
    onsettings: EventHandler<()>,
) -> Element {
    let controls = Controls::current();
    let snapshot = controls.snapshot();
    let running = snapshot
        .containers
        .iter()
        .filter(|entry| entry.running())
        .count();
    let stage = if snapshot.inventory_ready {
        "ready"
    } else {
        "inventory"
    };
    rsx! {
        div { class: "flex h-full min-h-0 flex-col overflow-hidden bg-background font-sans text-xs text-foreground",
            div { class: "flex h-8 shrink-0 items-center gap-2 px-2.5 text-[11px] font-medium text-foreground/70",
                LineIconView { icon: LineIcon::Package, class: "size-3.5 shrink-0" }
                span { class: "min-w-0 flex-1 truncate", {translate("container-engine")} }
                if snapshot.inventory_ready {
                    span { class: "text-[10px] tabular-nums text-muted-foreground/60", "{running}", " ", {translate("container-running")} }
                }
                EngineToolbar { compact: true, onsettings }
            }
            ContentTransition { key: "{stage}", class: "flex min-h-0 flex-1 flex-col",
                if !snapshot.inventory_error.is_empty() {
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
    let controls = Controls::current();
    let snapshot = controls.snapshot();
    let mut members = Vec::new();
    let project = selection.strip_prefix("project:");
    for entry in &snapshot.containers {
        if project.is_some_and(|project| entry.project == project) || entry.id == selection {
            members.push(entry.clone());
        }
    }
    let mut address = PageAddress {
        current: AddressOption {
            label_key: "container-engine".into(),
            icon: Some(BuiltinIcon::Container),
            ..Default::default()
        },
        ..Default::default()
    };
    address.fixed.push(AddressCrumb {
        entry: AddressOption {
            label_key: "container-title".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut toolbar = Vec::new();
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
            toolbar.push(
                ToolbarControl::button(translate("container-start-container"), move |_| {
                    controls.containers(start_ids.clone(), ContainerOperation::Start)
                })
                .icon(LineIcon::Play)
                .disabled(snapshot.busy),
            );
        }
        if !stop_ids.is_empty() {
            toolbar.push(
                ToolbarControl::button(translate("container-stop-container"), move |_| {
                    controls.containers(stop_ids.clone(), ContainerOperation::Stop)
                })
                .icon(LineIcon::Pause)
                .disabled(snapshot.busy),
            );
            toolbar.push(
                ToolbarControl::button(translate("container-restart"), move |_| {
                    controls.containers(all_ids.clone(), ContainerOperation::Restart)
                })
                .icon(LineIcon::RotateCcw)
                .disabled(snapshot.busy),
            );
        } else {
            toolbar.push(
                ToolbarControl::button(translate("container-delete"), move |_| {
                    ondelete.call(all_ids.clone())
                })
                .icon(LineIcon::Trash)
                .disabled(snapshot.busy),
            );
        }
    }
    rsx! {
        PageBreadcrumbs { state: address,
            onintent: move |intent| { if let AddressIntent::Section { id } = intent && matches!(id.as_str(), "info" | "logs") { onpanel.call(id); } }
        }
        if snapshot.available() {
            PageToolbar { controls: toolbar }
        } else {
            EngineToolbar { onsettings }
        }
    }
}

#[component]
fn EngineToolbar(#[props(default)] compact: bool, onsettings: EventHandler<()>) -> Element {
    let controls = Controls::current();
    let snapshot = controls.snapshot();
    rsx! {
        div { class: if compact { "flex shrink-0 items-center gap-1" } else { "flex h-8 items-center justify-end gap-1 px-3" },
            if snapshot.running {
                ManagerIconButton {
                    label: translate("container-stop"), disabled: snapshot.busy,
                    onclick: move |_| controls.request(Intent::Stop),
                    LineIconView { icon: LineIcon::Pause, class: "size-3.5" }
                }
            }
            ManagerIconButton {
                label: translate("container-settings"), disabled: snapshot.data_directory.is_empty(),
                onclick: move |_| onsettings.call(()),
                LineIconView { icon: LineIcon::Wrench, class: "size-3.5" }
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
    let controls = Controls::current();
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
                    onclick: move |event: MouseEvent| { event.stop_propagation(); controls.containers(vec![operation_id.clone()], if running { ContainerOperation::Stop } else { ContainerOperation::Start }); },
                    LineIconView { icon: if running { LineIcon::Pause } else { LineIcon::Play }, class: "size-3" }
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
    let controls = Controls::current();
    let snapshot = controls.snapshot();
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
    let controls = Controls::current();
    let state = controls.state;
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
    let snapshot = controls.snapshot();
    let locked = snapshot.running || snapshot.busy;
    let valid = cpus().parse::<u8>().is_ok_and(|value| value > 0)
        && memory().parse::<u32>().is_ok_and(|value| value >= 512)
        && !shared().trim().is_empty();
    rsx! {
        DialogRoot { open: true, on_open_change: move |next: bool| { if !next { onclose.call(()); } }, attributes: vec![],
            DialogContent { class: "max-w-[480px] p-5", attributes: vec![],
                DialogTitle { attributes: vec![], {translate("container-settings")} }
                if snapshot.running {
                    p { class: "text-xs text-muted-foreground/70", {translate("container-settings-running")} }
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
                    if locked {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), {translate("common-close")} }
                    } else {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), {translate("container-cancel")} }
                        Button { size: ButtonSize::Sm, disabled: !valid,
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
}

#[component]
fn DeleteDialog(ids: Vec<String>, onclose: EventHandler<()>) -> Element {
    let controls = Controls::current();
    rsx! {
        DialogRoot { open: true, on_open_change: move |next: bool| { if !next { onclose.call(()); } }, attributes: vec![],
            DialogContent { class: "max-w-[420px] p-5", attributes: vec![],
                DialogTitle { attributes: vec![], {translate("container-delete-title")} }
                DialogDescription { attributes: vec![], {translate("container-delete-description")} }
                div { class: "flex justify-end gap-2",
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), {translate("container-cancel")} }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Destructive, onclick: move |_| { controls.containers(ids.clone(), ContainerOperation::Remove); onclose.call(()); }, {translate("container-delete")} }
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
        "starting" | "stopping" | "restarting" => "animate-pulse bg-primary",
        "dead" => "bg-destructive",
        _ => "bg-muted-foreground/35",
    };
    rsx! { span { class: "size-1.5 shrink-0 rounded-full {class}", aria_hidden: true } }
}

#[component]
fn StateLabel(state: String) -> Element {
    let key = match state.as_str() {
        "starting" => "container-phase-created".to_string(),
        "stopping" => "container-phase-stopping".to_string(),
        "running" | "paused" | "exited" | "created" | "restarting" | "dead" | "removing" => {
            format!("container-state-{state}")
        }
        _ => "container-state-unknown".to_string(),
    };
    rsx! { span { class: "inline-flex items-center gap-2", StateDot { state } {translate(&key)} } }
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

impl ContainerUiState {
    fn available(&self) -> bool {
        (self.ready || (self.running && self.phase == "ready"))
            && !self.stopping
            && self.phase != "stopping"
    }
}

#[derive(Clone, Copy)]
struct Controls {
    state: Signal<ContainerUiState>,
    pending: Signal<Option<Pending>>,
    error: Signal<String>,
}

impl Controls {
    fn use_state(state: Signal<ContainerUiState>) -> Self {
        Self {
            state,
            pending: use_signal(|| None),
            error: use_signal(String::new),
        }
    }

    fn current() -> Self {
        use_context()
    }

    fn snapshot(self) -> ContainerUiState {
        let mut snapshot = (self.state)();
        if let Some(pending) = (self.pending)()
            && !pending.finished(&snapshot)
        {
            pending.intent.preview(&mut snapshot);
        }
        if !(self.error)().is_empty() {
            snapshot.error = (self.error)();
        }
        snapshot
    }

    fn reconcile(mut self) {
        let snapshot = (self.state)();
        let done = self
            .pending
            .peek()
            .as_ref()
            .is_some_and(|pending| pending.finished(&snapshot));
        if done {
            self.pending.set(None);
        }
    }

    fn request(mut self, intent: Intent) {
        if self.pending.peek().is_some() || self.state.peek().busy {
            return;
        }
        self.error.set(String::new());
        self.pending.set(Some(Pending {
            revision: self.state.peek().control_revision,
            intent: intent.clone(),
        }));
        if let Err(error) = intent.send() {
            self.pending.set(None);
            self.error.set(error.to_string());
        }
    }

    fn containers(self, ids: Vec<String>, operation: ContainerOperation) {
        if ids.is_empty() {
            return;
        }
        self.request(Intent::Containers { ids, operation });
    }
}

#[derive(Clone)]
struct Pending {
    revision: u64,
    intent: Intent,
}

impl Pending {
    fn finished(&self, snapshot: &ContainerUiState) -> bool {
        if snapshot.control_revision == self.revision {
            return false;
        }
        if !snapshot.control_error.is_empty() {
            return true;
        }
        match &self.intent {
            Intent::Start => snapshot.ready,
            Intent::Stop => !snapshot.running,
            Intent::Containers { ids, operation } => {
                if !snapshot.available() || !snapshot.inventory_error.is_empty() {
                    return true;
                }
                for id in ids {
                    let entry = snapshot.containers.iter().find(|entry| entry.id == *id);
                    let matches = match (operation, entry) {
                        (_, None) => true,
                        (ContainerOperation::Start | ContainerOperation::Restart, Some(entry)) => {
                            entry.running()
                        }
                        (ContainerOperation::Stop, Some(entry)) => !entry.running(),
                        _ => false,
                    };
                    if !matches {
                        return false;
                    }
                }
                true
            }
        }
    }
}

#[derive(Clone)]
enum Intent {
    Start,
    Stop,
    Containers {
        ids: Vec<String>,
        operation: ContainerOperation,
    },
}

impl Intent {
    fn send(&self) -> Result<(), vmux_ui::hooks::EventListenerError> {
        match self {
            Self::Start => send(&ContainerStartRequest),
            Self::Stop => send(&ContainerStopRequest),
            Self::Containers { ids, operation } => send(&ContainerCommandRequest {
                ids: ids.clone(),
                operation: *operation,
            }),
        }
    }

    fn preview(&self, snapshot: &mut ContainerUiState) {
        snapshot.busy = true;
        if snapshot.error == snapshot.control_error {
            snapshot.error.clear();
        }
        match self {
            Self::Start => snapshot.starting = true,
            Self::Stop => snapshot.stopping = true,
            Self::Containers { ids, operation } => {
                if *operation == ContainerOperation::Remove {
                    snapshot.containers.retain(|entry| !ids.contains(&entry.id));
                    return;
                }
                for entry in &mut snapshot.containers {
                    if !ids.contains(&entry.id) {
                        continue;
                    }
                    entry.state = match operation {
                        ContainerOperation::Start => "starting",
                        ContainerOperation::Stop => "stopping",
                        ContainerOperation::Restart => "restarting",
                        ContainerOperation::Remove => unreachable!(),
                    }
                    .into();
                    entry.status.clear();
                }
            }
        }
    }
}

#[component]
fn VmIllustration() -> Element {
    rsx! {
        svg { class: "h-28 w-40 text-muted-foreground/50", view_box: "0 0 160 112", fill: "none",
            role: "img", "aria-label": translate("container-description"),
            rect { x: "15", y: "8", width: "130", height: "82", rx: "9", fill: "currentColor", fill_opacity: "0.035", stroke: "currentColor", stroke_width: "2" }
            path { d: "M62 90v14m36-14v14m-48 0h60", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round" }
            g { class: "text-primary/80", stroke: "currentColor", stroke_width: "1.7", stroke_linejoin: "round",
                path { d: "m80 24 24 14v28L80 80 56 66V38Z", fill: "currentColor", fill_opacity: "0.07" }
                path { d: "m56 38 24 14 24-14M80 52v28M68 31l24 14" }
            }
        }
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
        stops: Cell<usize>,
        addresses: RefCell<Vec<PageAddress>>,
        inspections: RefCell<Vec<ContainerInspectRequest>>,
        commands: RefCell<Vec<ContainerCommandRequest>>,
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
            } else if id == ContainerStopRequest::id() {
                let _ =
                    rkyv::from_bytes::<ContainerStopRequest, rkyv::rancor::Error>(bytes).unwrap();
                self.stops.set(self.stops.get() + 1);
            } else if id == ContainerCommandRequest::id() {
                self.commands.borrow_mut().push(
                    rkyv::from_bytes::<ContainerCommandRequest, rkyv::rancor::Error>(bytes)
                        .unwrap(),
                );
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
    fn stopped_service_has_one_start_control_without_inactive_workspace_controls() {
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
        assert!(!html.contains(&translate("container-docker-stopped")));
        assert!(!html.contains(&translate("container-no-selection")));
        assert!(!html.contains(&format!(
            "aria-label=\"{}\"",
            translate("schema-side-sheet")
        )));
        let start = translate("container-start");
        assert_eq!(html.matches(&format!("aria-label=\"{start}\"")).count(), 1);
        assert!(!html.contains(&translate("container-cpus")));
        preview.click(&start);
        assert_eq!(preview.host.starts.get(), 1);
        let html = preview.render();
        assert!(html.contains("aria-busy=true"));
        assert!(html.contains(&translate("container-docker-starting")));
        assert!(!html.contains(&format!("aria-label=\"{start}\"")));
        let mut stale = ContainerUiState {
            supported: true,
            data_directory: "/fixtures/data".into(),
            ..Default::default()
        };
        let html = preview.publish(stale.clone());
        assert!(html.contains(&translate("container-docker-starting")));
        stale.control_revision = 1;
        stale.control_error = "fixture start refused".into();
        stale.error = stale.control_error.clone();
        let html = preview.publish(stale);
        assert!(html.contains("fixture start refused"));
        assert!(html.contains(&format!("aria-label=\"{start}\"")));
        assert!(!html.contains(&translate("container-docker-starting")));
    }

    #[test]
    fn service_shutdown_is_a_page_control_and_running_settings_are_read_only() {
        let mut preview = Preview::new();
        let mut snapshot = ContainerUiState {
            supported: true,
            running: true,
            ready: true,
            inventory_ready: true,
            data_directory: "/fixtures/data".into(),
            shared_directory: "/fixtures/projects".into(),
            cpus: 2,
            memory_mib: 2048,
            ..Default::default()
        };
        preview.publish(snapshot.clone());
        preview.click(&translate("container-stop"));
        assert_eq!(preview.host.stops.get(), 1);
        preview.click(&translate("container-settings"));
        let html = preview.render();
        assert!(html.contains(&translate("container-settings-running")));
        assert!(html.contains(&translate("common-close")));
        assert!(!html.contains(&translate("container-save")));
        assert!(!html.contains(&translate("container-cancel")));
        snapshot.running = false;
        snapshot.ready = false;
        snapshot.control_revision = 1;
        let html = preview.publish(snapshot.clone());
        assert!(html.contains(&translate("container-save")));
        assert!(html.contains(&translate("container-cancel")));
        assert_eq!(preview.host.stops.get(), 1);
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
        assert_eq!(address.current.label_key, "container-engine");
        assert_eq!(address.fixed[0].entry.label_key, "container-title");
        assert_eq!(address.fixed[1].entry.label, "web");
        assert_eq!(address.fixed[2].entry.label_key, "container-info");
        assert_eq!(
            address.fixed[2]
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
        let html = preview.publish(snapshot.clone());
        assert!(html.contains("Up 2 seconds"));
        assert_eq!(preview.host.inspections.borrow().len(), requests);
        assert_eq!(
            preview.host.inspections.borrow().last().unwrap().id,
            "eeeeeeeeeeee"
        );
        let vm_stop = format!("aria-label=\"{}\"", translate("container-stop"));
        let (sheet, details) = html.split_once("</aside>").expect("the side sheet");
        assert!(sheet.contains(&vm_stop));
        assert!(!details.contains(&vm_stop));
        assert!(details.contains(&format!(
            "aria-label=\"{}\"",
            translate("container-stop-container")
        )));
        preview.click(&translate("container-stop-container"));
        let commands = preview.host.commands.borrow();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].ids, ["eeeeeeeeeeee"]);
        assert_eq!(commands[0].operation, ContainerOperation::Stop);
        drop(commands);
        assert_eq!(preview.host.stops.get(), 0);
        let html = preview.render();
        assert!(html.contains(&translate("container-phase-stopping")));
        snapshot.control_revision = 1;
        snapshot.containers[0].state = "exited".into();
        let html = preview.publish(snapshot.clone());
        assert!(!html.contains(&translate("container-phase-stopping")));
        preview.click(&translate("container-stop"));
        assert_eq!(preview.host.stops.get(), 1);
        assert_eq!(preview.host.commands.borrow().len(), 1);
        assert!(
            preview
                .render()
                .contains(&translate("container-service-stopping"))
        );
        snapshot.control_revision = 2;
        snapshot.control_error = "fixture stop refused".into();
        snapshot.error = snapshot.control_error.clone();
        let html = preview.publish(snapshot);
        assert!(html.contains("fixture stop refused"));
        assert!(!html.contains(&translate("container-service-stopping")));
    }

    #[test]
    fn startup_and_shutdown_replace_the_workspace_without_losing_state() {
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
        assert!(!html.contains(&format!("aria-label=\"{}\"", translate("container-start"))));
        let toggle = translate("schema-side-sheet");
        assert!(!html.contains(&format!("aria-label=\"{toggle}\"")));
        let mut ready = ContainerUiState {
            supported: true,
            running: true,
            ready: true,
            inventory_ready: true,
            data_directory: "/fixtures/data".into(),
            ..Default::default()
        };
        let html = preview.publish(ready.clone());
        assert!(html.contains(&format!("aria-label=\"{toggle}\"")));
        assert!(html.contains(&translate("container-no-selection")));
        ready.phase = "ready".into();
        ready.ready = false;
        let html = preview.publish(ready.clone());
        assert!(html.contains(&format!("aria-label=\"{toggle}\"")));
        assert!(html.contains(&translate("container-no-selection")));
        assert!(!html.contains(&format!("aria-label=\"{}\"", translate("container-start"))));
        ready.ready = true;
        preview.click(&toggle);
        assert!(preview.sidebar_blocked);
        preview.click(&toggle);
        assert!(!preview.sidebar_blocked);
        ready.stopping = true;
        let html = preview.publish(ready.clone());
        assert!(html.contains(&translate("container-service-stopping")));
        assert!(html.contains("aria-busy=true"));
        assert!(!html.contains(&translate("container-no-selection")));
        assert!(!html.contains(&format!("aria-label=\"{toggle}\"")));
        ready.stopping = false;
        let html = preview.publish(ready);
        assert!(html.contains(&translate("container-no-selection")));
        assert_eq!(
            preview
                .host
                .listeners
                .borrow()
                .iter()
                .filter(|(id, _)| id == ContainerUiState::id())
                .count(),
            1
        );
    }
}
