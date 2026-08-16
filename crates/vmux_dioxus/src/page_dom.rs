//! The `VirtualDom`, driven by hand.

use dioxus_core::{Element, Event, VirtualDom};
use dioxus_html::HtmlEvent;
use dioxus_interpreter_js::MutationState;

use crate::event_request::EventOutcome;

/// A page whose components run here, rendering into a document owned by something else.
pub struct PageDom {
    dom: VirtualDom,
    mutations: MutationState,
    unflushed: bool,
}

impl PageDom {
    /// Mount `app` without rendering it. Call [`PageDom::rebuild`] for the first batch.
    pub fn mount(app: fn() -> Element) -> Self {
        Self::install_event_converter();

        Self {
            dom: VirtualDom::new(app),
            mutations: MutationState::default(),
            unflushed: false,
        }
    }

    /// Teach `dioxus_html` how to turn a serialized event back into typed data.
    ///
    /// The converter is a process-wide slot that starts empty, and every downcast in a handler
    /// unwraps it — so without this the first event panics inside `dioxus_html` rather than
    /// failing anywhere a caller could see.
    fn install_event_converter() {
        static ONCE: std::sync::Once = std::sync::Once::new();

        ONCE.call_once(|| {
            dioxus_html::set_event_converter(Box::new(dioxus_html::SerializedHtmlEventConverter));
        });
    }

    /// The first render, which always produces a batch: the document starts empty.
    pub fn rebuild(&mut self) -> Vec<u8> {
        self.dom.rebuild(&mut self.mutations);
        self.unflushed = true;
        self.mutations.export_memory()
    }

    /// Every render after the first.
    ///
    /// `None` means there is nothing to send — either nothing changed, or the last batch has not
    /// been acknowledged yet. A caller that gets `None` must ask again after [`PageDom::flushed`],
    /// or the page stops updating.
    ///
    /// Rendering is withheld while a batch is in flight because an effect may read the document,
    /// and the document does not yet reflect the render that scheduled the effect.
    pub fn render(&mut self) -> Option<Vec<u8>> {
        if self.unflushed || !self.has_work() {
            return None;
        }

        self.dom.render_immediate(&mut self.mutations);
        self.unflushed = true;

        Some(self.mutations.export_memory())
    }

    /// Whether a render would change anything, asked without blocking.
    ///
    /// An empty batch cannot answer this: `export_memory` always emits the channel's header, so a
    /// render with no work still yields bytes — 36 of them, whose contents move with channel state
    /// and so cannot be compared against a fixed baseline either.
    ///
    /// `wait_for_work` is `process_events` followed by a dirty-scope check, and awaits only once
    /// both say there is nothing to do. Polling it against a no-op waker runs exactly that check.
    /// Dropping the future afterwards discards nothing: the receiver it polls takes a message only
    /// when one is already waiting, and in that case the future resolved rather than pending.
    fn has_work(&mut self) -> bool {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let mut context = Context::from_waker(Waker::noop());

        matches!(
            std::pin::pin!(self.dom.wait_for_work()).poll(&mut context),
            Poll::Ready(())
        )
    }

    /// The document applied the batch it was last given.
    pub fn flushed(&mut self) {
        self.unflushed = false;
    }

    /// Whether a batch is still waiting to be applied.
    pub fn awaiting_flush(&self) -> bool {
        self.unflushed
    }

    /// Run one event through the page, and say whether the browser should still act on it.
    ///
    /// The answer is the caller's to return to a blocked page, so this must not defer any part of
    /// the work: the handlers run here, on this thread, before it returns.
    pub fn handle(&mut self, event: HtmlEvent) -> EventOutcome {
        let HtmlEvent {
            element,
            name,
            bubbles,
            data,
        } = event;

        let event = Event::new(data.into_any(), bubbles);
        self.dom
            .runtime()
            .handle_event(&name, event.clone(), element);

        EventOutcome::new(!event.default_action_enabled())
    }

    /// Wait until the page has work to do.
    pub async fn wait_for_work(&mut self) {
        self.dom.wait_for_work().await;
    }
}

#[cfg(test)]
// The fixtures below are components, and a component is PascalCase.
#[allow(non_snake_case)]
mod tests {
    use dioxus::prelude::*;
    use dioxus_core::ElementId;
    use dioxus_html::{EventData, HtmlEvent, SerializedMouseData};

    use super::*;

    fn Static() -> Element {
        rsx! { div { "hello" } }
    }

    #[test]
    fn a_first_render_describes_a_document_that_does_not_exist_yet() {
        let mut page = PageDom::mount(Static);

        assert!(
            !page.rebuild().is_empty(),
            "the document starts empty, so the first render has to say how to fill it"
        );
    }

    #[test]
    fn a_render_that_changes_nothing_sends_nothing() {
        let mut page = PageDom::mount(Static);
        page.rebuild();
        page.flushed();

        assert!(
            page.render().is_none(),
            "an unchanged page must not keep the document busy applying empty batches"
        );
    }

    #[test]
    fn no_further_batch_is_produced_until_the_last_one_is_acknowledged() {
        fn Counting() -> Element {
            let mut count = use_signal(|| 0);
            rsx! { button { onclick: move |_| count += 1, "{count}" } }
        }

        let mut page = PageDom::mount(Counting);
        page.rebuild();
        page.handle(click_on(ElementId(1)));

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
        fn Preventing() -> Element {
            rsx! { a { onclick: |event: Event<MouseData>| event.prevent_default(), "link" } }
        }

        let mut page = PageDom::mount(Preventing);
        page.rebuild();

        assert!(
            page.handle(click_on(ElementId(1))).prevent_default(),
            "a page that blocks navigation depends on this answer arriving before it returns"
        );
    }

    #[test]
    fn a_handler_that_does_not_prevent_the_default_lets_the_browser_act() {
        fn Plain() -> Element {
            rsx! { a { onclick: |_| {}, "link" } }
        }

        let mut page = PageDom::mount(Plain);
        page.rebuild();

        assert!(!page.handle(click_on(ElementId(1))).prevent_default());
    }

    #[test]
    fn an_event_for_an_element_that_has_no_handler_is_answered_rather_than_dropped() {
        let mut page = PageDom::mount(Static);
        page.rebuild();

        assert!(
            !page.handle(click_on(ElementId(9999))).prevent_default(),
            "the page blocks on the reply, so an unrecognised element still has to be answered"
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
