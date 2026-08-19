//! What differs between two views of the same page.
//!
//! A [`NativePage`](crate::NativePage) is one const per url, so anything per-view has nowhere to
//! live on it. A page served into a browser answered this with its own address —
//! `vmux://error/?title=…` — which only works where a page has a `location` to read.
//!
//! Here the host builds the `VirtualDom` itself, so it can put the difference in the root scope
//! before the first render. The page reads it with `try_consume_context` and has it immediately,
//! rather than mounting empty, asking, and rendering a second time when the answer arrives.

use dioxus_core::VirtualDom;

/// Erased as a closure rather than a value, because the concrete type is the host's and providing
/// it is generic — capturing it at the call site is what keeps this crate from having to name it.
type Provide = Box<dyn FnOnce(PageScope<'_>)>;

/// Per-view data for the host to put in a page's root scope.
#[derive(Default)]
pub struct Instance(Option<Provide>);

impl Instance {
    pub fn of(provide: impl FnOnce(PageScope<'_>) + 'static) -> Self {
        Self(Some(Box::new(provide)))
    }

    pub(crate) fn provide_to(self, dom: &VirtualDom) {
        let Some(provide) = self.0 else {
            return;
        };
        provide(PageScope(dom));
    }
}

/// A page's root scope while it is being built.
pub struct PageScope<'a>(&'a VirtualDom);

impl PageScope<'_> {
    pub fn provide<T: Clone + 'static>(&self, value: T) {
        self.0.provide_root_context(value);
    }
}

#[cfg(test)]
// The fixture below is a component, and a component is PascalCase.
#[allow(non_snake_case)]
mod tests {
    use dioxus::prelude::*;

    use super::*;
    use crate::PageDom;

    thread_local! {
        static SEEN: std::cell::RefCell<Option<String>> =
            const { std::cell::RefCell::new(None) };
    }

    #[component]
    fn Reader() -> Element {
        let seen = try_consume_context::<String>();
        SEEN.with(|cell| *cell.borrow_mut() = seen);
        rsx! { div {} }
    }

    /// The whole point of providing before the first render: a page whose views differ has its
    /// difference on the render that fills the document, rather than rendering once without it.
    ///
    /// Silent when broken — the page falls back to a default and shows a plausible empty state —
    /// so nothing else would catch it.
    #[test]
    fn a_page_reads_its_instance_on_the_render_that_builds_the_document() {
        SEEN.with(|cell| *cell.borrow_mut() = None);

        let mut page = PageDom::mount(
            Reader,
            Instance::of(|scope| scope.provide("the failure".to_string())),
        );
        page.rebuild();

        assert_eq!(
            SEEN.with(|cell| cell.borrow().clone()),
            Some("the failure".to_string()),
        );
    }
}
