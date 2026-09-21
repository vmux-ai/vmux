use dioxus_core::VirtualDom;

type Provide = Box<dyn FnOnce(PageScope<'_>)>;

#[derive(Default)]
pub struct Instance(Option<Provide>);

impl<F> From<F> for Instance
where
    F: for<'a> FnOnce(PageScope<'a>) + 'static,
{
    fn from(provide: F) -> Self {
        Self(Some(Box::new(provide)))
    }
}

impl Instance {
    pub(crate) fn provide_to(self, dom: &VirtualDom) {
        let Some(provide) = self.0 else {
            return;
        };
        provide(PageScope(dom));
    }
}

pub struct PageScope<'a>(&'a VirtualDom);

impl PageScope<'_> {
    pub fn provide<T: Clone + 'static>(&self, value: T) {
        self.0.provide_root_context(value);
    }
}

#[cfg(test)]
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

    #[test]
    fn a_page_reads_its_instance_on_the_render_that_builds_the_document() {
        SEEN.with(|cell| *cell.borrow_mut() = None);

        let mut page = PageDom::mount(
            Reader,
            Instance::from(|scope| scope.provide("the failure".to_string())),
        );
        page.rebuild();

        assert_eq!(
            SEEN.with(|cell| cell.borrow().clone()),
            Some("the failure".to_string()),
        );
    }
}
