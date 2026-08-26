use std::rc::Rc;

use dioxus_core::{Element, Event, VirtualDom};
use dioxus_html::{EventData, HtmlEvent, PlatformEventData, RenderedElementBacking};
use dioxus_interpreter_js::MutationState;

use crate::event_request::EventOutcome;

mod converter;

pub type PageComponent = fn() -> Element;

pub struct PageDom {
    dom: VirtualDom,
    mutations: MutationState,
    unflushed: bool,
}

impl PageDom {
    pub fn mount(app: PageComponent, instance: crate::Instance) -> Self {
        Self::install_event_converter();

        let dom = VirtualDom::new(app);
        instance.provide_to(&dom);

        Self {
            dom,
            mutations: MutationState::default(),
            unflushed: false,
        }
    }

    fn install_event_converter() {
        static ONCE: std::sync::Once = std::sync::Once::new();

        ONCE.call_once(|| {
            dioxus_html::set_event_converter(Box::new(converter::LiveElements::new()));
        });
    }

    pub fn rebuild(&mut self) -> Vec<u8> {
        self.dom.rebuild(&mut self.mutations);
        self.unflushed = true;
        self.mutations.export_memory()
    }

    pub fn render(&mut self) -> Option<Vec<u8>> {
        if self.unflushed || !self.has_work() {
            return None;
        }

        self.dom.render_immediate(&mut self.mutations);
        self.unflushed = true;

        Some(self.mutations.export_memory())
    }

    fn has_work(&mut self) -> bool {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let mut context = Context::from_waker(Waker::noop());

        matches!(
            std::pin::pin!(self.dom.wait_for_work()).poll(&mut context),
            Poll::Ready(())
        )
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
            EventData::Mounted => Rc::new(PlatformEventData::new(Box::new(
                converter::MountedBacking::of(backing),
            ))) as Rc<dyn std::any::Any>,
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

    #[component]
    fn Static() -> Element {
        rsx! { div { "hello" } }
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
        use std::cell::Cell;
        use std::rc::Rc;

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
