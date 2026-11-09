use std::rc::Rc;

use dioxus_core::{Element, Event, VirtualDom};
use dioxus_html::{EventData, HtmlEvent, PlatformEventData, RenderedElementBacking};
use dioxus_interpreter_js::MutationState;

use crate::event_request::EventOutcome;
use crate::page_dom_converter::{LiveElements, MountedBacking};

use std::future::Future;
use std::task::{Context, Poll, Waker};

pub type PageComponent = fn() -> Element;

pub struct PageDom {
    dom: VirtualDom,
    component: PageComponent,
    contexts: crate::instance::Contexts,
    mutations: MutationState,
    unflushed: bool,
    rebuilt: bool,
    snapshots: vmux_ui::hooks::UiStateSnapshots,
    host: Option<vmux_ui::transport::HostBinding>,
}

impl PageDom {
    pub fn mount(app: PageComponent, instance: crate::Instance) -> Self {
        Self::install_event_converter();

        let dom = VirtualDom::new(app);
        let contexts = instance.provide_to(&dom);
        let snapshots = vmux_ui::hooks::UiStateSnapshots::default();
        dom.provide_root_context(snapshots.clone());
        let host = vmux_ui::transport::HostScope::binding();
        if let Some(host) = &host {
            dom.provide_root_context(host.clone());
        }

        Self {
            dom,
            component: app,
            contexts,
            mutations: MutationState::default(),
            unflushed: false,
            rebuilt: false,
            snapshots,
            host,
        }
    }

    fn install_event_converter() {
        static ONCE: std::sync::Once = std::sync::Once::new();

        ONCE.call_once(|| {
            dioxus_html::set_event_converter(Box::new(LiveElements::new()));
        });
    }

    pub fn rebuild(&mut self) -> Vec<u8> {
        self.rebuild_with_waker(Waker::noop())
    }

    pub fn rebuild_with_waker(&mut self, waker: &Waker) -> Vec<u8> {
        if self.rebuilt {
            self.snapshots.capture();
            let dom = VirtualDom::new(self.component);
            self.contexts.provide_to(&dom);
            dom.provide_root_context(self.snapshots.clone());
            if let Some(host) = &self.host {
                dom.provide_root_context(host.clone());
            }
            self.dom = dom;
        }
        self.rebuilt = true;
        self.mutations = MutationState::default();
        self.dom.rebuild(&mut self.mutations);
        self.snapshots.finish_restore();
        self.unflushed = true;
        let mutations = self.mutations.export_memory();
        self.arm_waker(waker);
        mutations
    }

    pub fn render(&mut self) -> Option<Vec<u8>> {
        self.render_with_waker(Waker::noop())
    }

    pub fn render_with_waker(&mut self, waker: &Waker) -> Option<Vec<u8>> {
        if self.unflushed || !self.has_work(waker) {
            return None;
        }

        self.dom.render_immediate(&mut self.mutations);
        self.unflushed = true;

        let mutations = self.mutations.export_memory();
        self.arm_waker(waker);
        Some(mutations)
    }

    pub fn update_instance(&mut self, instance: crate::Instance) {
        self.contexts = instance.provide_to(&self.dom);
        self.dom.mark_all_dirty();
    }

    fn has_work(&mut self, waker: &Waker) -> bool {
        let mut context = Context::from_waker(waker);

        matches!(
            std::pin::pin!(self.dom.wait_for_work()).poll(&mut context),
            Poll::Ready(())
        )
    }

    fn arm_waker(&mut self, waker: &Waker) {
        let mut context = Context::from_waker(waker);
        let _ = std::pin::pin!(self.dom.wait_for_work()).poll(&mut context);
    }

    pub fn flushed(&mut self) {
        self.unflushed = false;
    }

    pub fn awaiting_flush(&self) -> bool {
        self.unflushed
    }

    pub fn handle(
        &mut self,
        event: HtmlEvent,
        backing: impl RenderedElementBacking + Clone + 'static,
    ) -> EventOutcome {
        let HtmlEvent {
            element,
            name,
            bubbles,
            data,
        } = event;

        let data = match data {
            EventData::Mounted => Rc::new(PlatformEventData::new(Box::new(MountedBacking::from(
                backing,
            )))) as Rc<dyn std::any::Any>,
            data => data.into_any(),
        };
        let event = Event::new(data, bubbles);
        self.dom
            .runtime()
            .handle_event(&name, event.clone(), element);

        EventOutcome::new(!event.default_action_enabled())
    }

    pub async fn wait_for_work(&mut self) {
        self.dom.wait_for_work().await;
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use dioxus::prelude::*;
    use dioxus_core::ElementId;
    use dioxus_html::{EventData, HtmlEvent, SerializedMouseData};

    use super::*;

    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Default)]
    struct BindingHost(Cell<usize>);

    impl vmux_ui::transport::PageHost for BindingHost {
        fn send(&self, _: &str, _: &[u8]) -> Result<(), vmux_ui::hooks::EventListenerError> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }

        fn listen(
            &self,
            _: &str,
            _: vmux_ui::transport::BytesListener,
        ) -> Result<(), vmux_ui::hooks::EventListenerError> {
            Ok(())
        }
    }

    #[component]
    fn BoundPage() -> Element {
        consume_context::<vmux_ui::transport::HostBinding>()
            .0
            .send("binding", &[])
            .unwrap();
        rsx! {}
    }

    #[component]
    fn MountWithinAnotherPage() -> Element {
        let host = consume_context::<Rc<BindingHost>>();
        let _host = vmux_ui::transport::HostScope::enter(host);
        let mut page = PageDom::mount(BoundPage, crate::Instance::default());
        page.rebuild();
        rsx! {}
    }

    #[test]
    fn mounting_a_page_binds_its_host_instead_of_the_callers_context() {
        let outer = Rc::new(BindingHost::default());
        let inner = Rc::new(BindingHost::default());
        let mut dom = VirtualDom::new(MountWithinAnotherPage);
        dom.provide_root_context(vmux_ui::transport::HostBinding(outer.clone()));
        dom.provide_root_context(inner.clone());
        dom.rebuild_in_place();
        assert_eq!(outer.0.get(), 0);
        assert_eq!(inner.0.get(), 1);
    }

    #[cfg(ui)]
    #[derive(Clone, Default)]
    struct AddressClicks {
        url: Rc<Cell<usize>>,
        item: Rc<Cell<usize>>,
    }

    #[cfg(ui)]
    #[component]
    fn ClickableAddress() -> Element {
        let clicks = consume_context::<AddressClicks>();
        let url = clicks.url;
        let item = clicks.item;
        rsx! {
            vmux_ui::components::page_address::AddressSurface {
                url: "vmux://settings/".to_string(),
                onactivate: move |_| url.set(url.get() + 1),
                button { id: "address-navigation-item".to_string(), onclick: move |_| item.set(item.get() + 1), "Settings" }
            }
        }
    }

    #[test]
    #[cfg(ui)]
    fn address_background_and_navigation_buttons_have_independent_click_events() {
        let clicks = AddressClicks::default();
        let provided = clicks.clone();
        let mut page = PageDom::mount(
            ClickableAddress,
            crate::Instance::from(move |scope: crate::PageScope<'_>| scope.provide(provided)),
        );
        let initial = page.dom.rebuild_to_vec();
        let background = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                dioxus_core::Mutation::SetAttribute {
                    name: "title",
                    value: dioxus_core::AttributeValue::Text(value),
                    id,
                    ..
                } if value == "vmux://settings/" => Some(*id),
                _ => None,
            })
            .unwrap();
        let navigation = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                dioxus_core::Mutation::SetAttribute {
                    name: "id",
                    value: dioxus_core::AttributeValue::Text(value),
                    id,
                    ..
                } if value == "address-navigation-item" => Some(*id),
                _ => None,
            })
            .unwrap();
        page.handle(click_on(background), ());
        assert_eq!(clicks.url.get(), 1);
        assert_eq!(clicks.item.get(), 0);
        page.handle(click_on(navigation), ());
        assert_eq!(clicks.item.get(), 1);
        assert_eq!(clicks.url.get(), 1);
    }

    #[component]
    fn Static() -> Element {
        rsx! { div { "hello" } }
    }

    #[component]
    fn InstanceContext() -> Element {
        let value = consume_context::<String>();
        rsx! { div { "{value}" } }
    }

    #[derive(Clone, Default)]
    struct ActiveScopes(Rc<Cell<usize>>);

    impl ActiveScopes {
        fn enter(&self) {
            self.0.set(self.0.get() + 1);
        }

        fn leave(&self) {
            self.0.set(self.0.get() - 1);
        }
    }

    #[component]
    fn Tracked() -> Element {
        let scopes = consume_context::<ActiveScopes>();
        let mounted = scopes.clone();
        use_hook(move || mounted.enter());
        use_drop(move || scopes.leave());
        rsx! { div { "tracked" } }
    }

    #[test]
    fn full_rebuild_releases_the_previous_component_scopes() {
        let scopes = ActiveScopes::default();
        let provided = scopes.clone();
        let mut page = PageDom::mount(
            Tracked,
            crate::Instance::from(move |scope: crate::PageScope<'_>| {
                scope.provide(provided.clone());
            }),
        );
        page.rebuild();
        assert_eq!(scopes.0.get(), 1);
        page.flushed();
        page.rebuild();

        assert_eq!(scopes.0.get(), 1);
        drop(page);
        assert_eq!(scopes.0.get(), 0);
    }

    #[test]
    fn document_recovery_reapplies_the_latest_instance_context() {
        let mut page = PageDom::mount(
            InstanceContext,
            crate::Instance::from(|scope: crate::PageScope<'_>| scope.provide("first".to_string())),
        );
        page.rebuild();
        page.flushed();
        page.update_instance(crate::Instance::from(|scope: crate::PageScope<'_>| {
            scope.provide("updated".to_string());
        }));
        let recovered = page.rebuild();

        assert!(String::from_utf8_lossy(&recovered).contains("updated"));
        assert!(!String::from_utf8_lossy(&recovered).contains("first"));
    }

    #[test]
    fn a_first_render_describes_a_document_that_does_not_exist_yet() {
        let mut page = PageDom::mount(Static, crate::Instance::default());

        assert!(
            !page.rebuild().is_empty(),
            "the document starts empty, so the first render has to say how to fill it"
        );
    }

    #[test]
    fn a_render_that_changes_nothing_sends_nothing() {
        let mut page = PageDom::mount(Static, crate::Instance::default());
        page.rebuild();
        page.flushed();

        assert!(
            page.render().is_none(),
            "an unchanged page must not keep the document busy applying empty batches"
        );
    }

    #[test]
    fn no_further_batch_is_produced_until_the_last_one_is_acknowledged() {
        #[component]
        fn Counting() -> Element {
            let mut count = use_signal(|| 0);
            rsx! { button { onclick: move |_| count += 1, "{count}" } }
        }

        let mut page = PageDom::mount(Counting, crate::Instance::default());
        page.rebuild();
        page.handle(click_on(ElementId(1)), ());

        assert!(
            page.render().is_none(),
            "the first batch is still unacknowledged, so the effects of this render would read a stale document"
        );

        page.flushed();
        assert!(
            page.render().is_some(),
            "once acknowledged, the pending change has to reach the document"
        );
    }

    #[test]
    fn a_controlled_field_carries_its_new_value_to_the_document() {
        #[component]
        fn Field() -> Element {
            let mut text = use_signal(String::new);
            rsx! {
                button { onclick: move |_| text.set("recalled".to_string()) }
                textarea { value: "{text}" }
            }
        }

        let mut page = PageDom::mount(Field, crate::Instance::default());
        page.rebuild();
        page.flushed();
        page.handle(click_on(ElementId(1)), ());
        let batch = page
            .render()
            .expect("the press changed the field, so something must go out");

        assert!(
            String::from_utf8_lossy(&batch).contains("recalled"),
            "a field the reader cannot edit by hand is only ever set by its state, so a batch \
             that leaves the new text out leaves the field showing the old one for good"
        );
    }

    #[test]
    fn updating_an_instance_renders_again_without_rebuilding_the_dom() {
        let mut page = PageDom::mount(
            InstanceContext,
            crate::Instance::from(|scope: crate::PageScope<'_>| scope.provide("first".to_string())),
        );
        page.rebuild();
        page.flushed();
        page.update_instance(crate::Instance::from(|scope: crate::PageScope<'_>| {
            scope.provide("second".to_string())
        }));

        let batch = page.render().expect("the changed context dirties the page");
        assert!(String::from_utf8_lossy(&batch).contains("second"));
    }

    #[test]
    fn a_handler_that_prevents_the_default_is_reported_to_the_page() {
        #[component]
        fn Preventing() -> Element {
            rsx! { a { onclick: |event: Event<MouseData>| event.prevent_default(), "link" } }
        }

        let mut page = PageDom::mount(Preventing, crate::Instance::default());
        page.rebuild();

        assert!(
            page.handle(click_on(ElementId(1)), ()).prevent_default(),
            "a page that blocks navigation depends on this answer arriving before it returns"
        );
    }

    #[test]
    fn a_handler_that_does_not_prevent_the_default_lets_the_browser_act() {
        #[component]
        fn Plain() -> Element {
            rsx! { a { onclick: |_| {}, "link" } }
        }

        let mut page = PageDom::mount(Plain, crate::Instance::default());
        page.rebuild();

        assert!(!page.handle(click_on(ElementId(1)), ()).prevent_default());
    }

    #[test]
    fn an_event_for_an_element_that_has_no_handler_is_answered_rather_than_dropped() {
        let mut page = PageDom::mount(Static, crate::Instance::default());
        page.rebuild();

        assert!(
            !page.handle(click_on(ElementId(9999)), ()).prevent_default(),
            "the page blocks on the reply, so an unrecognised element still has to be answered"
        );
    }

    #[test]
    fn a_mounted_element_reaches_the_page_able_to_answer_for_itself() {
        #[derive(Clone, Default)]
        struct Focusable(Rc<Cell<bool>>);

        impl dioxus_html::RenderedElementBacking for Focusable {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn set_focus(
                &self,
                _: bool,
            ) -> std::pin::Pin<Box<dyn Future<Output = dioxus_html::MountedResult<()>>>>
            {
                self.0.set(true);
                Box::pin(std::future::ready(Ok(())))
            }
        }

        #[component]
        fn Mounting() -> Element {
            rsx! {
                div {
                    onmounted: move |event: Event<MountedData>| {
                        drop(event.data().set_focus(true));
                    },
                }
            }
        }

        let backing = Focusable::default();
        let mut page = PageDom::mount(Mounting, crate::Instance::default());
        page.rebuild();
        page.handle(
            HtmlEvent {
                element: ElementId(1),
                name: "mounted".to_string(),
                bubbles: false,
                data: EventData::Mounted,
            },
            backing.clone(),
        );

        assert!(
            backing.0.get(),
            "the page asked its own element to take focus; a backing that never arrives makes \
             that a silent no-op, and the page cannot be typed in"
        );
    }

    fn click_on(element: ElementId) -> HtmlEvent {
        HtmlEvent {
            element,
            name: "click".to_string(),
            bubbles: true,
            data: EventData::Mouse(SerializedMouseData::default()),
        }
    }
}
