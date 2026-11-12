use std::{collections::HashMap, rc::Rc};

use dioxus::prelude::*;
use vmux_api::address::{
    AddressCrumb, AddressIntent, AddressOption, PageAddress, PageAddressRequest,
};

use crate::components::avatar::IdentityOrb;
use crate::components::breadcrumb::{BreadcrumbDropdown, BreadcrumbSeparator};
use crate::components::dropdown::DropdownOption;
use crate::components::manager::{ManagerAddButton, ManagerIconButton};
use crate::file_icon::TypeIcon;
use crate::hooks::send;
use crate::i18n::translate;
use crate::icon::{BuiltinIconView, LineIcon, LineIconView};

#[component]
pub fn AddressTrail(
    state: PageAddress,
    #[props(default)] onpopup: Option<EventHandler<bool>>,
    #[props(default)] dismiss_revision: u64,
    #[props(default)] loading: bool,
) -> Element {
    let mut open = use_signal(|| None::<String>);
    let mut observed_dismiss = use_signal(|| dismiss_revision);
    let identity = (state.owner, state.document_url().to_string());
    let mut previous = use_signal(|| identity.clone());
    use_effect(use_reactive!(|(identity, dismiss_revision)| {
        if *previous.peek() != identity || *observed_dismiss.peek() != dismiss_revision {
            previous.set(identity.clone());
            observed_dismiss.set(dismiss_revision);
            open.set(None);
        }
    }));
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
    let requests = AddressSender::new(&state);
    let current_is_parent = matches!(&state.current.intent, Some(AddressIntent::Navigate { url }) if url.trim_end_matches('/') != state.document_url().trim_end_matches('/'));
    rsx! {
        nav {
            class: "relative flex h-7 min-w-0 flex-1 items-center gap-1 text-xs",
            aria_label: translate("editor-breadcrumb"),
            onclick: move |event: MouseEvent| event.stop_propagation(),
            AddressMenu {
                id: "page".to_string(),
                crumb: AddressCrumb {
                    entry: state.current.clone(), options: state.views.clone(),
                    ..Default::default()
                },
                requests: requests.clone(), open,
                loading,
                navigate_on_click: current_is_parent,
            }
            for (index, crumb) in state.fixed.iter().enumerate() {
                span { key: "fixed-{index}", class: "flex min-w-0 shrink items-center gap-1",
                    BreadcrumbSeparator {}
                    AddressMenu { id: format!("fixed-{index}"), crumb: crumb.clone(), requests: requests.clone(), open }
                }
            }
            if !state.paths.is_empty() { BreadcrumbSeparator {} }
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
pub fn AddressControls(state: PageAddress) -> Element {
    let requests = AddressSender::new(&state);
    let search = requests.clone();
    rsx! {
        if let Some(checkout) = &state.checkout {
            {
                let intent = checkout.intent.clone();
                rsx! { ManagerAddButton {
                    label: translate("git-checkout"), icon: LineIcon::GitBranch,
                    onclick: move |_| { if let Some(intent) = &intent { requests.send(intent.clone()); } },
                } }
            }
        }
        if state.search_available { ManagerIconButton {
            label: translate("editor-show-search"), active: state.search_active,
            onclick: move |_| search.send(AddressIntent::Search),
            LineIconView { icon: LineIcon::Search, class: "size-3.5" }
        } }
    }
}

#[component]
fn AddressMenu(
    id: String,
    crumb: AddressCrumb,
    requests: AddressSender,
    mut open: Signal<Option<String>>,
    #[props(default)] preserve_name: bool,
    #[props(default)] loading: bool,
    #[props(default)] navigate_on_click: bool,
) -> Element {
    if crumb.options.is_empty()
        && crumb.tabs.is_empty()
        && crumb.on_open.is_none()
        && crumb.on_more.is_none()
        && crumb.entry.intent.is_none()
    {
        return rsx! { span { class: "flex h-7 min-w-0 items-center gap-1", title: crumb.entry.title.clone(),
            AddressIcon { entry: crumb.entry.clone(), loading }
            span { class: "truncate", {AddressLabel(&crumb.entry).text()} }
        } };
    }
    let active = open.read().as_ref() == Some(&id);
    let open_mode = crate::components::dropdown::DropdownOpenMode::Hover;
    let open_id = id.clone();
    let on_open = crumb.on_open.clone();
    let open_requests = requests.clone();
    let more_requests = requests.clone();
    let more = crumb.on_more.clone();
    let on_primary = if navigate_on_click {
        let intent = crumb.entry.intent.clone();
        let navigation = requests.clone();
        intent.map(|intent| {
            EventHandler::new(move |()| {
                open.set(None);
                navigation.send(intent.clone());
            })
        })
    } else {
        None
    };
    let terminal_directory = matches!(
        &crumb.on_open,
        Some(AddressIntent::TerminalDirectories { .. })
    );
    rsx! {
        BreadcrumbDropdown {
            open_mode,
            on_primary,
            label: AddressLabel(&crumb.entry).text(), title: crumb.entry.title.clone(), open: active, active: crumb.active, truncate: !preserve_name,
            on_open_change: move |active| {
                if active {
                    open.set(Some(open_id.clone()));
                    if let Some(intent) = &on_open { open_requests.send(intent.clone()); }
                } else if open.peek().as_ref() == Some(&open_id) { open.set(None); }
            },
            leading: rsx! { if !preserve_name || crumb.active { AddressIcon { entry: crumb.entry.clone(), loading } } },
            if !crumb.tabs.is_empty() {
                div { class: "flex gap-1 p-1",
                    for tab in crumb.tabs {
                        {
                            let requests = requests.clone();
                            let intent = tab.intent.clone();
                            let selected = intent == crumb.selected_tab;
                            rsx! { button {
                                r#type: "button", class: if selected { "rounded-md bg-foreground/10 px-2 py-1 text-xs text-foreground" } else { "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-foreground/5 hover:text-foreground" },
                                onclick: move |_| { if let Some(intent) = &intent { requests.send(intent.clone()); } },
                                {AddressLabel(&tab).text()}
                            } }
                        }
                    }
                }
            }
            div { class: "max-h-72 overflow-y-auto",
                onscroll: move |event: Event<ScrollData>| {
                    if !crumb.pending && crate::scroll::ScrollPreload::LIST.reached(&event)
                        && let Some(intent) = &more
                    {
                        more_requests.send(intent.clone());
                    }
                },
            if crumb.pending {
                div { class: "space-y-2 px-3 py-2", for index in 0..3 { div { key: "{index}", class: "h-3 w-32 animate-pulse rounded bg-foreground/10" } } }
            } else if crumb.options.is_empty() {
                div { class: "px-3 py-2 text-muted-foreground", {translate("editor-breadcrumb-empty")} }
            }
            for entry in crumb.options {
                {
                    let requests = requests.clone();
                    let intent = entry.intent.clone();
                    let title = if terminal_directory && intent.is_none() {
                        translate("terminal-directory-input-pending")
                    } else {
                        entry.title.clone()
                    };
                    rsx! { DropdownOption {
                        title,
                        key: "{entry.identity}-{entry.path}-{entry.intent:?}",
                        label: AddressLabel(&entry).text(), selected: intent.is_some() && entry.intent == crumb.entry.intent,
                        disabled: intent.is_none(),
                        onclick: move |_| { open.set(None); if let Some(intent) = &intent { requests.send(intent.clone()); } },
                        leading: rsx! { AddressIcon { entry } },
                    } }
                }
            }
            }
        }
    }
}

#[component]
fn AddressIcon(entry: AddressOption, #[props(default)] loading: bool) -> Element {
    if loading {
        return rsx! { crate::components::progress::LoadingIcon {} };
    }
    if !entry.identity.is_empty() {
        return rsx! { IdentityOrb { seed: entry.identity, alt: entry.label, class: "size-4".to_string() } };
    }
    if !entry.icon_url.is_empty() {
        return rsx! { img { src: entry.icon_url, alt: entry.label, class: "size-4 shrink-0 object-contain" } };
    }
    if !entry.path.is_empty() {
        return rsx! { TypeIcon { path: entry.path, is_dir: entry.is_dir, class: "size-4 shrink-0 opacity-80" } };
    }
    if let Some(icon) = entry.icon {
        return rsx! { BuiltinIconView { icon, class: "size-4 shrink-0".to_string() } };
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
struct AddressSender {
    owner: u64,
    revision: u64,
    url: Rc<str>,
}

impl AddressSender {
    fn new(address: &PageAddress) -> Self {
        Self {
            owner: address.owner,
            revision: address.revision,
            url: Rc::from(address.url.as_str()),
        }
    }

    fn send(&self, intent: AddressIntent) {
        let _ = send(&PageAddressRequest {
            owner: self.owner,
            revision: self.revision,
            url: self.url.to_string(),
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
    use crate::transport::HostBinding;
    use crate::transport::{
        BytesListener, HostPayload, HostScope, PageHost, event_listener::EventListenerError,
    };
    use dioxus::core::{AttributeValue, ElementId, Mutation, VirtualDom};
    use dioxus_html::{
        EventData, SerializedHtmlEventConverter, SerializedMouseData, SerializedResizeData,
        geometry::PixelsSize,
    };
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;

    #[derive(Default)]
    struct TrailProbe {
        requests: RefCell<Vec<PageAddressRequest>>,
        popup: RefCell<Vec<bool>>,
    }

    impl PageHost for TrailProbe {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == PageAddressRequest::id()
                && let Some(request) = HostPayload::new(bytes).decode::<PageAddressRequest>()
            {
                self.requests.borrow_mut().push(request);
            }
            Ok(())
        }

        fn listen(&self, _id: &str, _listener: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[component]
    fn RoutedTrail() -> Element {
        let probe = use_context::<Rc<TrailProbe>>();
        let mut dismissed = use_signal(|| 0u64);
        rsx! { AddressTrail {
            state: PageAddress {
                owner: 17, revision: 9, url: "vmux://sessions/one".into(),
                current: AddressOption { label: "Sessions".into(), ..Default::default() },
                views: vec![AddressOption {
                    label: "Projects".into(),
                    intent: Some(AddressIntent::Navigate { url: "vmux://projects/".into() }),
                    ..Default::default()
                }],
                checkout: Some(AddressOption { label: "Checkout".into(), ..Default::default() }),
                ..Default::default()
            },
            dismiss_revision: dismissed(),
            onpopup: move |open| probe.popup.borrow_mut().push(open),
        }
        button { id: format!("dismiss-{}", "address"), onclick: move |_| dismissed += 1, "Dismiss" }
        }
    }

    #[component]
    fn ParentTrail() -> Element {
        rsx! { AddressTrail {
            state: PageAddress {
                owner: 17, revision: 9, url: "vmux://projects/one".into(),
                current: AddressOption { label: "Projects".into(), intent: Some(AddressIntent::Navigate { url: "vmux://projects/".into() }), ..Default::default() },
                views: vec![AddressOption { label: "All projects".into(), intent: Some(AddressIntent::Navigate { url: "vmux://projects/".into() }), ..Default::default() }],
                ..Default::default()
            }
        } }
    }

    #[test]
    fn parent_breadcrumb_click_navigates_even_when_its_hover_menu_is_open() {
        dioxus::html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let probe = Rc::new(TrailProbe::default());
        let mut dom = VirtualDom::new(ParentTrail);
        dom.provide_root_context(HostBinding(probe.clone()));
        let initial = dom.rebuild_to_vec();
        let trigger = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-expanded",
                    id,
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "mouseenter",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                false,
            ),
            trigger,
        );
        dom.render_immediate_to_vec();
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            trigger,
        );
        dom.render_immediate_to_vec();
        assert!(
            matches!(probe.requests.borrow().as_slice(), [PageAddressRequest { owner: 17, revision: 9, url, intent: AddressIntent::Navigate { url: target } }] if url == "vmux://projects/one" && target == "vmux://projects/")
        );
    }

    #[test]
    fn declared_routes_render_navigation_and_popup_capture_without_page_operations() {
        dioxus::html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let probe = Rc::new(TrailProbe::default());
        let _host = HostScope::enter(probe.clone());
        let mut dom = VirtualDom::new(RoutedTrail);
        dom.provide_root_context(probe.clone());
        let initial = dom.rebuild_to_vec();
        let trigger = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-expanded",
                    id,
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("breadcrumb trigger missing: {:?}", initial.edits));
        let dismiss = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "id",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == "dismiss-address" => Some(*id),
                _ => None,
            })
            .unwrap();
        assert!(dioxus_ssr::render(&dom).contains("Sessions"));
        assert!(!dioxus_ssr::render(&dom).contains(&translate("git-checkout")));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            trigger,
        );
        let opened = dom.render_immediate_to_vec();
        let option = opened
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "data-vmux-dropdown-selected",
                    id,
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.render_immediate_to_vec();
        assert!(probe.popup.borrow().contains(&true));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            option,
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            probe.requests.borrow().as_slice(),
            [PageAddressRequest {
                owner: 17,
                revision: 9,
                url: "vmux://sessions/one".into(),
                intent: AddressIntent::Navigate {
                    url: "vmux://projects/".into()
                },
            }]
        );
        dom.render_immediate_to_vec();
        assert_eq!(probe.popup.borrow().last(), Some(&false));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            trigger,
        );
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        assert_eq!(probe.popup.borrow().last(), Some(&true));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            dismiss,
        );
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        assert_eq!(probe.popup.borrow().last(), Some(&false));
    }

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
