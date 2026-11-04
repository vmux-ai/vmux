use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use crate::components::breadcrumb::{
    BreadcrumbOption, BreadcrumbOptionIcon, BreadcrumbSelector, Breadcrumbs,
};
use crate::hooks::UiStateSnapshots;
use crate::navigation::PageNavigation;
use crate::transport::event_listener::EventListenerError;
use crate::transport::{BytesListener, HostBinding, PageHost, TextOffsetAnswer};

#[component]
pub fn PageAddress(
    current: BreadcrumbOption,
    options: Vec<BreadcrumbOption>,
    children: Element,
    #[props(default = rsx! {})] actions: Element,
    #[props(default = rsx! {})] view_controls: Element,
    #[props(default)] context: PageAddressContext,
) -> Element {
    let label = current.label.clone();
    let leading = current.clone();
    rsx! {
        PageAddressContent { context,
        div { class: "flex h-8 min-w-0 flex-1 items-center gap-2", onclick: move |event: MouseEvent| event.stop_propagation(),
            div { class: "flex min-w-0 flex-1 items-center gap-2",
            Breadcrumbs { class: "min-w-0 flex-1 overflow-hidden pr-4 [mask-image:linear-gradient(to_right,black_calc(100%_-_1rem),transparent)]".to_string(),
                BreadcrumbSelector {
                    label,
                    value: current.value.clone(),
                    options,
                    leading: rsx! {
                        BreadcrumbOptionIcon {
                            option: leading,
                            class: "size-4 shrink-0".to_string(),
                        }
                    },
                    onselect: PageNavigation::replace,
                }
                {children}
            }
            {view_controls}
            }
            div { class: "flex shrink-0 items-center gap-1", {actions} }
        }
        }
    }
}

#[component]
pub fn PageAddressContent(
    children: Element,
    #[props(default)] context: PageAddressContext,
) -> Element {
    let slot = use_hook(AddressRegistry::allocate);
    let host = use_hook(HostBinding::current);
    let snapshots = try_consume_context::<UiStateSnapshots>().unwrap_or_default();
    use_effect(use_reactive!(|children, context| {
        if let Some(host) = &host {
            AddressRegistry::publish(
                slot,
                AddressEntry {
                    content: children.clone(),
                    context: context.clone(),
                    host: host.clone(),
                    snapshots: snapshots.clone(),
                },
            );
            host.0.publish_address(slot);
        }
    }));
    use_drop(move || AddressRegistry::remove(slot));
    rsx! {}
}

#[component]
pub fn PageAddressOutlet(slot: u64, fallback: Element) -> Element {
    let tick = use_signal(|| 0u64);
    let watcher = use_hook(move || AddressRegistry::watch(tick));
    use_drop(move || AddressRegistry::unwatch(watcher));
    let _ = tick();
    let Some(entry) = AddressRegistry::entry(slot) else {
        return fallback;
    };
    let Some(presentation) = HostBinding::current() else {
        return fallback;
    };
    rsx! { AddressContent { key: "{slot}", entry, presentation } }
}

#[component]
pub fn AddressSurface(url: String, onactivate: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div { class: "relative flex h-8 min-w-0 flex-1 items-center",
            button {
                r#type: "button",
                "data-vmux-url-edit": "",
                class: "absolute inset-0 appearance-none rounded-md border-0 bg-transparent outline-none cursor-text focus-visible:bg-foreground/[0.04]",
                title: url.clone(),
                aria_label: url,
                onclick: move |_| onactivate.call(()),
            }
            div {
                class: "pointer-events-none relative flex h-full min-w-0 flex-1 items-center px-1 [&_button]:pointer-events-auto [&_a]:pointer-events-auto [&_input]:pointer-events-auto [&_textarea]:pointer-events-auto [&_select]:pointer-events-auto [&_[role=button]]:pointer-events-auto [&_[role=tab]]:pointer-events-auto [&_[popover]]:pointer-events-auto [&_[contenteditable]]:pointer-events-auto [&_[data-vmux-address-item]]:pointer-events-auto [&_[data-vmux-dropdown-root]]:pointer-events-auto",
                {children}
            }
        }
    }
}

#[component]
fn AddressContent(entry: Rc<AddressEntry>, presentation: HostBinding) -> Element {
    let source = entry.host.0.clone();
    let presentation = presentation.0;
    use_context_provider(move || {
        HostBinding(Rc::new(AddressHost {
            source,
            presentation,
        }))
    });
    use_context_provider(|| entry.snapshots.clone());
    entry.context.provide();
    entry.content.clone()
}

#[derive(Clone, Default)]
pub struct PageAddressContext(Vec<Rc<dyn Fn()>>);

impl PageAddressContext {
    pub fn with<T: Clone + 'static>(mut self, value: T) -> Self {
        self.0.push(Rc::new(move || {
            provide_context(value.clone());
        }));
        self
    }

    fn provide(&self) {
        for provide in &self.0 {
            provide();
        }
    }
}

impl PartialEq for PageAddressContext {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        for (left, right) in self.0.iter().zip(&other.0) {
            if !Rc::ptr_eq(left, right) {
                return false;
            }
        }
        true
    }
}

struct AddressEntry {
    content: Element,
    context: PageAddressContext,
    host: HostBinding,
    snapshots: UiStateSnapshots,
}

impl PartialEq for AddressEntry {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content && self.context == other.context
    }
}

#[derive(Default)]
struct AddressRegistry {
    entries: HashMap<u64, Rc<AddressEntry>>,
    watchers: HashMap<u64, Signal<u64>>,
}

thread_local! {
    static ADDRESSES: RefCell<AddressRegistry> = RefCell::new(AddressRegistry::default());
}

impl AddressRegistry {
    fn allocate() -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    fn publish(slot: u64, entry: AddressEntry) {
        ADDRESSES.with(|registry| {
            let mut registry = registry.borrow_mut();
            if registry
                .entries
                .get(&slot)
                .is_some_and(|current| **current == entry)
            {
                return;
            }
            registry.entries.insert(slot, Rc::new(entry));
            registry.changed();
        });
    }

    fn remove(slot: u64) {
        ADDRESSES.with(|registry| {
            let mut registry = registry.borrow_mut();
            if registry.entries.remove(&slot).is_some() {
                registry.changed();
            }
        });
    }

    fn entry(slot: u64) -> Option<Rc<AddressEntry>> {
        ADDRESSES.with(|registry| registry.borrow().entries.get(&slot).cloned())
    }

    fn watch(tick: Signal<u64>) -> u64 {
        let id = Self::allocate();
        ADDRESSES.with(|registry| registry.borrow_mut().watchers.insert(id, tick));
        id
    }

    fn unwatch(id: u64) {
        ADDRESSES.with(|registry| registry.borrow_mut().watchers.remove(&id));
    }

    fn changed(&mut self) {
        for tick in self.watchers.values_mut() {
            if let Ok(mut value) = tick.try_write() {
                *value = value.wrapping_add(1);
            }
        }
    }
}

struct AddressHost {
    source: Rc<dyn PageHost>,
    presentation: Rc<dyn PageHost>,
}

impl PageHost for AddressHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        self.source.send(id, bytes)
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        self.source.listen(id, on_bytes)
    }

    fn focus_element(&self, id: &str) {
        self.presentation.focus_element(id);
    }

    fn show_element_popover(&self, id: &str) {
        self.presentation.show_element_popover(id);
    }

    fn show_dropdown_popover(&self, id: &str) {
        self.presentation.show_dropdown_popover(id);
    }

    fn scroll_element_into_view(&self, id: &str) {
        self.presentation.scroll_element_into_view(id);
    }

    fn scroll_element_into_view_vertical(&self, id: &str) {
        self.presentation.scroll_element_into_view_vertical(id);
    }

    fn reveal_first_rendered(&self, ids: &[&str], centered: bool) {
        self.presentation.reveal_first_rendered(ids, centered);
    }

    fn scroll_element_to(&self, id: &str, top: f64) {
        self.presentation.scroll_element_to(id, top);
    }

    fn scroll_element_to_smooth(&self, id: &str, top: f64) {
        self.presentation.scroll_element_to_smooth(id, top);
    }

    fn section_at_top(&self, container: &str) -> TextOffsetAnswer {
        self.presentation.section_at_top(container)
    }

    fn scroll_section(&self, container: &str, id: &str, behavior: dioxus::html::ScrollBehavior) {
        self.presentation.scroll_section(container, id, behavior);
    }

    fn animate_height(&self, id: &str, duration: u32) {
        self.presentation.animate_height(id, duration);
    }

    fn animate_layout(&self, id: &str, duration: u32) {
        self.presentation.animate_layout(id, duration);
    }

    fn preserve_scroll(&self) {
        self.presentation.preserve_scroll();
    }

    fn text_offset_at(&self, id: &str, x: f64, y: f64) -> TextOffsetAnswer {
        self.presentation.text_offset_at(id, x, y)
    }

    fn select_element_text(&self, id: &str) {
        self.presentation.select_element_text(id);
    }

    fn toggle_media(&self, id: &str) {
        self.presentation.toggle_media(id);
    }

    fn offer_element_text(&self, id: &str) {
        self.presentation.offer_element_text(id);
    }

    fn place_caret(&self, id: &str, byte: usize) {
        self.presentation.place_caret(id, byte);
    }

    fn caret_to_end(&self, id: &str) {
        self.presentation.caret_to_end(id);
    }

    fn run_script(&self, script: String) {
        self.presentation.run_script(script);
    }

    fn event_field_selection(&self, id: &str) -> (usize, usize) {
        self.presentation.event_field_selection(id)
    }

    fn event_document_has_selection(&self) -> bool {
        self.presentation.event_document_has_selection()
    }

    fn write_to_clipboard(&self, text: &str) {
        self.presentation.write_to_clipboard(text);
    }

    fn resolves_keys(&self) -> bool {
        self.presentation.resolves_keys()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use crate::transport::{Host, HostScope};

    use super::*;

    #[derive(Default)]
    struct Probe {
        slot: Cell<u64>,
        sends: Cell<usize>,
        focuses: Cell<usize>,
    }

    impl PageHost for Probe {
        fn publish_address(&self, slot: u64) {
            self.slot.set(slot);
        }

        fn send(&self, _: &str, _: &[u8]) -> Result<(), EventListenerError> {
            self.sends.set(self.sends.get() + 1);
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn focus_element(&self, _: &str) {
            self.focuses.set(self.focuses.get() + 1);
        }
    }

    #[derive(Clone)]
    struct Controls(Rc<RefCell<Option<EventHandler<()>>>>);

    #[derive(Clone, PartialEq)]
    struct Location(String);

    #[component]
    fn Source() -> Element {
        let mut selected = use_signal(|| false);
        let controls = use_context::<Controls>();
        rsx! {
            p { if selected() { "chosen" } else { "body" } }
            PageAddressContent {
                context: PageAddressContext::default().with(Location("location".into())),
                TestControl { selected, controls, onselect: move |_| {
                    Host::emit("parent", &[]).unwrap();
                    selected.set(true);
                } }
            }
        }
    }

    #[component]
    fn StandardSource() -> Element {
        rsx! {
            p { "body" }
            PageAddress {
                current: BreadcrumbOption {
                    label: "Settings".into(),
                    value: "vmux://settings/".into(),
                    identity_seed: String::new(),
                    icon_url: String::new(),
                    icon: None,
                },
                options: Vec::new(),
            }
        }
    }

    #[component]
    fn PlainSource() -> Element {
        rsx! { p { "plain page" } }
    }

    #[component]
    fn TestControl(
        selected: Signal<bool>,
        controls: Controls,
        onselect: EventHandler<()>,
    ) -> Element {
        let location = use_context::<Location>();
        let click = EventHandler::new(move |_| {
            Host::emit("control", &[]).unwrap();
            Host::focus_element("field");
            onselect.call(());
        });
        *controls.0.borrow_mut() = Some(click);
        rsx! { button { onclick: move |_| click.call(()), "{location.0}", if selected() { "selected" } else { "select" } } }
    }

    impl PartialEq for Controls {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }

    #[component]
    fn Destination(slot: u64) -> Element {
        rsx! { PageAddressOutlet { slot, fallback: rsx! { p { "url" } } } }
    }

    type TestPage = fn() -> Element;

    struct AddressFixture {
        source: Option<VirtualDom>,
        destination: VirtualDom,
        source_host: Rc<Probe>,
        destination_host: Rc<Probe>,
        controls: Controls,
    }

    impl AddressFixture {
        fn mount() -> Self {
            Self::for_page(Source)
        }

        fn for_page(page: TestPage) -> Self {
            let source_host = Rc::new(Probe::default());
            let destination_host = Rc::new(Probe::default());
            let controls = Controls(Rc::default());
            let mut source = VirtualDom::new(page);
            source.provide_root_context(HostBinding(source_host.clone()));
            source.provide_root_context(controls.clone());
            source.rebuild_in_place();
            source.render_immediate(&mut dioxus::core::NoOpMutations);
            let mut destination = VirtualDom::new_with_props(
                Destination,
                DestinationProps {
                    slot: source_host.slot.get(),
                },
            );
            destination.provide_root_context(HostBinding(destination_host.clone()));
            destination.rebuild_in_place();
            Self {
                source: Some(source),
                destination,
                source_host,
                destination_host,
                controls,
            }
        }

        fn click(&mut self) {
            let _host = HostScope::enter(self.destination_host.clone());
            self.controls.0.borrow().as_ref().unwrap().call(());
            self.source
                .as_mut()
                .unwrap()
                .render_immediate(&mut dioxus::core::NoOpMutations);
            self.destination
                .render_immediate(&mut dioxus::core::NoOpMutations);
        }

        fn source_html(&self) -> String {
            dioxus_ssr::render(self.source.as_ref().unwrap())
        }

        fn destination_html(&self) -> String {
            dioxus_ssr::render(&self.destination)
        }
    }

    #[test]
    fn pages_opt_into_the_standard_address_api_without_changing_plain_pages() {
        let opted_in = AddressFixture::for_page(StandardSource);
        assert!(!opted_in.source_html().contains("Settings"));
        assert!(opted_in.destination_html().contains("Settings"));
        let plain = AddressFixture::for_page(PlainSource);
        assert!(plain.source_html().contains("plain page"));
        assert!(plain.destination_html().contains("url"));
        assert!(!plain.destination_html().contains("Settings"));
    }

    #[test]
    fn address_ui_moves_out_of_the_page_and_keeps_its_owner_and_context() {
        let mut fixture = AddressFixture::mount();
        assert!(fixture.source_html().contains("body"));
        assert!(!fixture.source_html().contains("button"));
        assert!(fixture.destination_html().contains("location"));
        fixture.click();
        assert_eq!(fixture.source_host.sends.get(), 2);
        assert_eq!(fixture.destination_host.sends.get(), 0);
        assert_eq!(fixture.source_host.focuses.get(), 0);
        assert_eq!(fixture.destination_host.focuses.get(), 1);
        assert!(fixture.source_html().contains("chosen"));
        assert!(fixture.destination_html().contains("selected"));
    }

    #[test]
    fn identical_pages_keep_separate_address_controls_and_state() {
        let mut first = AddressFixture::mount();
        let second = AddressFixture::mount();
        first.click();
        assert!(first.source_html().contains("chosen"));
        assert!(second.source_html().contains("body"));
        assert_eq!(second.source_host.sends.get(), 0);
        assert_ne!(first.source_host.slot.get(), second.source_host.slot.get());
    }

    #[test]
    fn closing_a_page_removes_its_address_ui_from_the_window() {
        let mut fixture = AddressFixture::mount();
        fixture.source.take();
        fixture
            .destination
            .render_immediate(&mut dioxus::core::NoOpMutations);
        assert!(!fixture.destination_html().contains("button"));
        assert!(fixture.destination_html().contains("url"));
    }
}
