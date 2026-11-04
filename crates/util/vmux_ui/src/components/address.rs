use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_api::address::{
    AddressCrumb, AddressIntent, AddressOption, PageAddress, PageAddressRequest,
};

use crate::components::avatar::IdentityOrb;
use crate::components::breadcrumb::{
    BreadcrumbDropdown, BreadcrumbOption, BreadcrumbOptionIcon, BreadcrumbSelector,
    BreadcrumbSeparator,
};
use crate::components::dropdown::DropdownOption;
use crate::components::manager::{ManagerAddButton, ManagerIconButton, WorkspacePage};
use crate::file_icon::TypeIcon;
use crate::hooks::send;
use crate::i18n::translate;
use crate::icon::{BuiltinIconView, LineIcon, LineIconView};

#[component]
pub fn AddressTrail(
    state: PageAddress,
    #[props(default)] onpopup: Option<EventHandler<bool>>,
    #[props(default)] dismiss_revision: u64,
) -> Element {
    let mut open = use_signal(|| None::<String>);
    let mut observed_dismiss = use_signal(|| dismiss_revision);
    let identity = (state.owner, state.revision, state.url.clone());
    let mut previous = use_signal(|| identity.clone());
    use_effect(move || {
        if *previous.peek() != identity || *observed_dismiss.peek() != dismiss_revision {
            previous.set(identity.clone());
            observed_dismiss.set(dismiss_revision);
            open.set(None);
        }
    });
    use_effect(move || {
        let active = open().is_some();
        if let Some(onpopup) = onpopup {
            onpopup.call(active);
        }
    });
    use_drop(move || {
        if let Some(onpopup) = onpopup {
            onpopup.call(false);
        }
    });
    let mut available = use_signal(|| 0.0);
    let mut widths = use_signal(HashMap::<String, f64>::new);
    let measurement_open = use_signal(|| None::<String>);
    let visible = PathVisibility::start(&state.paths, available(), &widths.read());
    let page = if state.page == "knowledge" {
        WorkspacePage::Knowledge
    } else {
        WorkspacePage::Editor
    };
    let mut contextual = Vec::new();
    for entry in &state.contextual {
        let Some(AddressIntent::Navigate { url }) = &entry.intent else {
            continue;
        };
        contextual.push(BreadcrumbOption {
            label: AddressLabel(entry).text(),
            value: url.clone(),
            identity_seed: String::new(),
            icon_url: String::new(),
            icon: entry.icon,
        });
    }
    let mut current = page.option();
    if state.page == "editor"
        && let Some(editor) = contextual.first()
    {
        current = editor.clone();
    }
    let requests = AddressSender(state.clone());
    let page_requests = requests.clone();
    let search_requests = requests.clone();
    rsx! {
        nav {
            class: "relative flex h-7 min-w-0 flex-1 items-center gap-1 text-xs",
            aria_label: translate("editor-breadcrumb"),
            onclick: move |event: MouseEvent| event.stop_propagation(),
            BreadcrumbSelector {
                label: current.label,
                value: current.value,
                options: WorkspacePage::options_with(contextual),
                leading: rsx! { BreadcrumbOptionIcon { option: page.option(), class: "size-4 shrink-0".to_string() } },
                on_open_change: move |active: bool| open.set(active.then(|| "page".to_string())),
                onselect: move |url: String| page_requests.send(AddressIntent::Navigate { url }),
            }
            for (index, crumb) in state.fixed.iter().enumerate() {
                span { key: "fixed-{index}", class: "flex shrink-0 items-center gap-1",
                    BreadcrumbSeparator {}
                    AddressMenu { id: format!("fixed-{index}"), crumb: crumb.clone(), requests: requests.clone(), open }
                }
            }
            if let Some(checkout) = &state.checkout {
                {
                    let intent = checkout.intent.clone();
                    let requests = requests.clone();
                    rsx! { ManagerAddButton {
                        label: translate("git-checkout"),
                        icon: LineIcon::GitBranch,
                        onclick: move |_| { if let Some(intent) = &intent { requests.send(intent.clone()); } },
                    } }
                }
            }
            BreadcrumbSeparator {}
            div {
                class: "flex min-w-0 flex-1 items-center overflow-x-auto [contain:inline-size] [scrollbar-width:none] [mask-image:linear-gradient(to_right,black_calc(100%_-_1rem),transparent)]",
                onresize: move |event: Event<ResizeData>| {
                    if let Ok(size) = event.get_border_box_size() && size.width.is_finite() && size.width > 0.0 && (*available.peek() - size.width).abs() > 0.5 { available.set(size.width); }
                },
                if visible > 0 {
                    AddressMenu {
                        id: "hidden".to_string(),
                        crumb: AddressCrumb { entry: AddressOption { label: "…".to_string(), label_key: String::new(), ..Default::default() }, options: state.paths[..visible].iter().map(|crumb| crumb.entry.clone()).collect(), ..Default::default() },
                        requests: requests.clone(), open,
                    }
                    BreadcrumbSeparator {}
                }
                for (index, crumb) in state.paths.iter().enumerate().skip(visible) {
                    {
                        rsx! {
                            span {
                                key: "{crumb.entry.path}", class: "flex shrink-0 items-center",
                                AddressMenu { id: format!("path-{}", crumb.entry.path), crumb: crumb.clone(), requests: requests.clone(), open, preserve_name: true }
                                if index + 1 < state.paths.len() { BreadcrumbSeparator {} }
                            }
                        }
                    }
                }
            }
            for (index, crumb) in state.symbols.iter().enumerate() {
                span { key: "symbol-{index}", class: "flex min-w-0 shrink items-center gap-1",
                    BreadcrumbSeparator {}
                    AddressMenu { id: format!("symbol-{index}"), crumb: crumb.clone(), requests: requests.clone(), open }
                }
            }
            ManagerIconButton {
                label: translate("editor-show-search"), active: state.search_active,
                onclick: move |_| search_requests.send(AddressIntent::Search),
                LineIconView { icon: LineIcon::Search, class: "size-3.5" }
            }
            div {
                class: "pointer-events-none invisible absolute left-0 top-0 flex w-max items-center",
                aria_hidden: "true",
                inert: true,
                for (index, crumb) in state.paths.iter().enumerate().skip(state.paths.len().saturating_sub(PathVisibility::MAX_PATHS)) {
                    {
                        let path = crumb.entry.path.clone();
                        rsx! {
                            span {
                                key: "{path}", class: "flex shrink-0 items-center",
                                "data-vmux-address-measure": path.clone(),
                                onresize: move |event: Event<ResizeData>| {
                                    if let Ok(size) = event.get_border_box_size() && size.width.is_finite() && size.width > 0.0 && widths.peek().get(&path).is_none_or(|width| (width - size.width).abs() > 0.5) { widths.write().insert(path.clone(), size.width); }
                                },
                                AddressMenu { id: format!("measure-{}", crumb.entry.path), crumb: crumb.clone(), requests: requests.clone(), open: measurement_open, preserve_name: true }
                                if index + 1 < state.paths.len() { BreadcrumbSeparator {} }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddressMenu(
    id: String,
    crumb: AddressCrumb,
    requests: AddressSender,
    mut open: Signal<Option<String>>,
    #[props(default)] preserve_name: bool,
) -> Element {
    let active = open.read().as_ref() == Some(&id);
    let open_id = id.clone();
    let on_open = crumb.on_open.clone();
    let open_requests = requests.clone();
    rsx! {
        BreadcrumbDropdown {
            label: AddressLabel(&crumb.entry).text(), title: crumb.entry.title.clone(), open: active, active: crumb.active, truncate: !preserve_name,
            on_open_change: move |active| {
                if active {
                    open.set(Some(open_id.clone()));
                    if let Some(intent) = &on_open { open_requests.send(intent.clone()); }
                } else if open.peek().as_ref() == Some(&open_id) { open.set(None); }
            },
            leading: rsx! { if !preserve_name || crumb.active { AddressIcon { entry: crumb.entry.clone() } } },
            if crumb.pending {
                div { class: "space-y-2 px-3 py-2", for index in 0..3 { div { key: "{index}", class: "h-3 w-32 animate-pulse rounded bg-foreground/10" } } }
            } else if crumb.options.is_empty() {
                div { class: "px-3 py-2 text-muted-foreground", {translate("editor-breadcrumb-empty")} }
            }
            for entry in crumb.options {
                {
                    let requests = requests.clone();
                    let intent = entry.intent.clone();
                    rsx! { DropdownOption {
                        key: "{entry.title}-{entry.label}",
                        label: AddressLabel(&entry).text(), selected: entry.intent == crumb.entry.intent,
                        onclick: move |_| { open.set(None); if let Some(intent) = &intent { requests.send(intent.clone()); } },
                        leading: rsx! { AddressIcon { entry } },
                    } }
                }
            }
        }
    }
}

#[component]
fn AddressIcon(entry: AddressOption) -> Element {
    if !entry.identity.is_empty() {
        return rsx! { IdentityOrb { seed: entry.identity, alt: entry.label, class: "size-4".to_string() } };
    }
    if !entry.path.is_empty() {
        return rsx! { TypeIcon { path: entry.path, is_dir: entry.is_dir, class: "size-3.5 shrink-0 opacity-80" } };
    }
    if let Some(icon) = entry.icon {
        return rsx! { BuiltinIconView { icon, class: "size-3.5 shrink-0".to_string() } };
    }
    rsx! {}
}

struct AddressLabel<'a>(&'a AddressOption);

impl AddressLabel<'_> {
    fn text(self) -> String {
        if self.0.label_key.is_empty() {
            self.0.label.clone()
        } else {
            translate(&self.0.label_key)
        }
    }
}

#[derive(Clone, PartialEq)]
struct AddressSender(PageAddress);

impl AddressSender {
    fn send(&self, intent: AddressIntent) {
        let _ = send(&PageAddressRequest {
            owner: self.0.owner,
            revision: self.0.revision,
            url: self.0.url.clone(),
            intent,
        });
    }
}

struct PathVisibility;

impl PathVisibility {
    const MAX_PATHS: usize = 4;

    fn start(paths: &[AddressCrumb], available: f64, widths: &HashMap<String, f64>) -> usize {
        let mut hidden = paths.len().saturating_sub(Self::MAX_PATHS);
        if available <= 0.0 {
            return hidden;
        }
        let mut total = if hidden > 0 { 28.0 } else { 0.0 };
        for crumb in &paths[hidden..] {
            let Some(width) = widths.get(&crumb.entry.path) else {
                return hidden;
            };
            total += width;
        }
        while hidden + 1 < paths.len() && total > available {
            if hidden == 0 {
                total += 28.0;
            }
            total -= widths[&paths[hidden].entry.path];
            hidden += 1;
        }
        hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, ElementId, Mutation, VirtualDom};
    use dioxus_html::{
        EventData, SerializedHtmlEventConverter, SerializedResizeData, geometry::PixelsSize,
    };

    #[component]
    fn SizingAddress() -> Element {
        rsx! { AddressTrail {
            state: PageAddress {
                paths: vec![
                    AddressCrumb { entry: AddressOption::file("talks".into(), "/repo/talks".into(), true), ..Default::default() },
                    AddressCrumb { entry: AddressOption::file("long-file-name.md".into(), "/repo/talks/long-file-name.md".into(), false), ..Default::default() },
                ],
                ..Default::default()
            },
        } }
    }

    struct AddressSizing {
        dom: VirtualDom,
        viewport: ElementId,
        measurements: HashMap<String, ElementId>,
    }

    impl AddressSizing {
        fn mount() -> Self {
            dioxus::html::set_event_converter(Box::new(SerializedHtmlEventConverter));
            let mut dom = VirtualDom::new(SizingAddress);
            let initial = dom.rebuild_to_vec();
            let mut measurements = HashMap::new();
            let mut listeners = Vec::new();
            for edit in initial.edits {
                match edit {
                    Mutation::SetAttribute {
                        name: "data-vmux-address-measure",
                        value: AttributeValue::Text(path),
                        id,
                        ..
                    } => {
                        measurements.insert(path, id);
                    }
                    Mutation::NewEventListener { name, id } if name == "resize" => {
                        listeners.push(id)
                    }
                    _ => {}
                }
            }
            let viewport = listeners
                .into_iter()
                .find(|id| !measurements.values().any(|measured| measured == id))
                .unwrap();
            dom.render_immediate_to_vec();
            Self {
                dom,
                viewport,
                measurements,
            }
        }

        fn resize(&mut self, element: ElementId, width: f64) {
            let size = PixelsSize::new(width, 28.0);
            let data = EventData::Resize(SerializedResizeData::new(size, size));
            self.dom
                .runtime()
                .handle_event("resize", Event::new(data.into_any(), false), element);
            self.dom.render_immediate_to_vec();
        }

        fn html(&self) -> String {
            dioxus_ssr::render(&self.dom)
        }
    }

    #[test]
    fn address_measurement_keeps_hidden_directories_measurable_and_ignores_detached_sizes() {
        let mut address = AddressSizing::mount();
        let directory = address.measurements["/repo/talks"];
        let file = address.measurements["/repo/talks/long-file-name.md"];
        address.resize(directory, 60.0);
        address.resize(file, 400.0);
        address.resize(address.viewport, 700.0);
        assert!(!address.html().contains(">…<"));
        address.resize(address.viewport, 430.0);
        assert!(address.html().contains(">…<"));
        let narrow = address.html();
        address.resize(address.viewport, 0.0);
        address.resize(file, 0.0);
        address.resize(address.viewport, 430.25);
        assert_eq!(address.html(), narrow);
        address.resize(directory, 10.0);
        assert!(!address.html().contains(">…<"));
        address.resize(directory, 60.0);
        assert!(address.html().contains(">…<"));
        address.resize(address.viewport, 700.0);
        assert!(!address.html().contains(">…<"));
    }

    #[test]
    fn narrow_addresses_hide_whole_directories_and_keep_the_filename() {
        let paths =
            ["src", "features", "editor", "very-long-file-name.rs"].map(|name| AddressCrumb {
                entry: AddressOption::file(
                    name.to_string(),
                    format!("/repo/{name}"),
                    !name.ends_with(".rs"),
                ),
                ..Default::default()
            });
        let mut widths = HashMap::new();
        for (crumb, width) in paths.iter().zip([35.0, 60.0, 50.0, 160.0]) {
            widths.insert(crumb.entry.path.clone(), width);
        }
        assert_eq!(PathVisibility::start(&paths, 320.0, &widths), 0);
        assert_eq!(PathVisibility::start(&paths, 240.0, &widths), 2);
        let start = PathVisibility::start(&paths, 120.0, &widths);
        assert_eq!(
            paths[start..]
                .iter()
                .map(|crumb| crumb.entry.label.as_str())
                .collect::<Vec<_>>(),
            ["very-long-file-name.rs"]
        );
    }
}
