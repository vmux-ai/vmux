//! Drive a page the way a person would, with no window and no browser.
//!
//! A [`PageProbe`] mounts a page's components, renders them into a [`ShadowTree`], and then trades
//! in selectors: find something, click it, read what changed. It is the same `VirtualDom` and the
//! same mutations the webview would be given, so a behaviour that holds here holds there.
//!
//! What it cannot answer is anything the browser decides: layout, styling, hit-testing, input
//! method. A probe says the page reacted; only a screenshot says it looked right.

use std::fmt;

use dioxus_core::ElementId;
use dioxus_html::{EventData, HtmlEvent, SerializedMouseData};

use crate::event_request::EventOutcome;
use crate::page_dom::{PageComponent, PageDom};
use crate::selector::{Selector, SelectorError};
use crate::shadow_tree::ShadowTree;

/// A mounted page, queried and driven by selector.
pub struct PageProbe {
    page: PageDom<ShadowTree>,
}

/// Why a probe could not do what it was asked.
#[derive(Clone, Debug, PartialEq)]
pub enum ProbeError {
    /// The selector was not a selector.
    BadSelector(SelectorError),
    /// Nothing in the document matched.
    NoSuchElement { selector: String, outline: String },
    /// Something matched, but dioxus never gave it an id, so no event can name it.
    NotAddressable { selector: String },
    /// The element is there and addressable, but nothing is listening for this event.
    NoListener { selector: String, event: String },
    /// Renders kept scheduling more renders.
    DidNotSettle,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSelector(error) => write!(f, "{error}"),
            Self::NoSuchElement { selector, outline } => {
                write!(
                    f,
                    "nothing matched `{selector}`. The document is:\n{outline}"
                )
            }
            Self::NotAddressable { selector } => write!(
                f,
                "`{selector}` matched an element dioxus never assigned an id, so no event can \
                 reach it — give it a listener or a dynamic attribute"
            ),
            Self::NoListener { selector, event } => write!(
                f,
                "`{selector}` is not listening for `{event}`, so dispatching one would assert \
                 nothing"
            ),
            Self::DidNotSettle => f.write_str("the page kept re-rendering and never went quiet"),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<SelectorError> for ProbeError {
    fn from(error: SelectorError) -> Self {
        Self::BadSelector(error)
    }
}

/// A render that schedules a render is normal; sixty-four in a row is a loop.
const SETTLE_LIMIT: usize = 64;

impl PageProbe {
    /// Mount `app` and render it, leaving the document ready to query.
    pub fn mount(app: PageComponent, instance: crate::Instance) -> Self {
        let mut page = PageDom::with_sink(app, instance);
        page.diff_rebuild();
        let mut probe = Self { page };
        let _ = probe.settle();

        probe
    }

    /// Click the first element `selector` names, then let the page finish reacting.
    pub fn click(&mut self, selector: &str) -> Result<EventOutcome, ProbeError> {
        self.dispatch(
            selector,
            "click",
            EventData::Mouse(SerializedMouseData::default()),
        )
    }

    /// Send `event` to the first element `selector` names, then let the page finish reacting.
    ///
    /// The escape hatch behind [`PageProbe::click`]: anything `dioxus_html` can serialize can be
    /// delivered here, addressed by selector rather than by a raw [`ElementId`].
    pub fn dispatch(
        &mut self,
        selector: &str,
        event: &str,
        data: EventData,
    ) -> Result<EventOutcome, ProbeError> {
        let element = self.find(selector)?;
        if !self.tree().has_listener(element, event) {
            return Err(ProbeError::NoListener {
                selector: selector.to_string(),
                event: event.to_string(),
            });
        }

        let outcome = self.page.handle(
            HtmlEvent {
                element,
                name: event.to_string(),
                bubbles: true,
                data,
            },
            (),
        );
        self.settle()?;

        Ok(outcome)
    }

    /// The id of the first element `selector` names.
    pub fn find(&self, selector: &str) -> Result<ElementId, ProbeError> {
        let parsed: Selector = selector.parse()?;
        let Some(element) = self.tree().find(&parsed) else {
            return Err(match self.tree().exists(&parsed) {
                true => ProbeError::NotAddressable {
                    selector: selector.to_string(),
                },
                false => ProbeError::NoSuchElement {
                    selector: selector.to_string(),
                    outline: self.outline(),
                },
            });
        };

        Ok(element)
    }

    /// Every character of text under the first element `selector` names.
    pub fn text(&self, selector: &str) -> Result<String, ProbeError> {
        let parsed: Selector = selector.parse()?;
        let Some(text) = self.tree().text(&parsed) else {
            return Err(ProbeError::NoSuchElement {
                selector: selector.to_string(),
                outline: self.outline(),
            });
        };

        Ok(text)
    }

    /// One attribute of the first element `selector` names.
    pub fn attribute(&self, selector: &str, name: &str) -> Result<Option<String>, ProbeError> {
        let parsed: Selector = selector.parse()?;

        Ok(self.tree().attribute(&parsed, name))
    }

    /// How many elements `selector` names, which is `0` when it names none.
    pub fn count(&self, selector: &str) -> Result<usize, ProbeError> {
        let parsed: Selector = selector.parse()?;

        Ok(self.tree().count(&parsed))
    }

    /// The document as indented text, which is what a failing assertion should print.
    pub fn outline(&self) -> String {
        self.tree().outline()
    }

    /// The document itself, for a question the methods above do not cover.
    pub fn tree(&self) -> &ShadowTree {
        self.page.sink()
    }

    /// Render until nothing more is scheduled.
    ///
    /// A page acknowledges each batch before the next is produced, so a probe has to play the
    /// document's part: flush, render, repeat. Effects run between those renders, and an effect
    /// that sets a signal is why one render is not enough.
    fn settle(&mut self) -> Result<(), ProbeError> {
        for _ in 0..SETTLE_LIMIT {
            self.page.flushed();
            if !self.page.diff_render() {
                return Ok(());
            }
        }

        Err(ProbeError::DidNotSettle)
    }
}

#[cfg(test)]
// The fixtures below are components, and a component is PascalCase.
#[allow(non_snake_case)]
mod tests {
    use dioxus::prelude::*;

    use super::*;

    impl PageProbe {
        fn of(app: PageComponent) -> Self {
            Self::mount(app, crate::Instance::default())
        }
    }

    #[component]
    fn Counter() -> Element {
        let mut count = use_signal(|| 0);

        rsx! {
            button { "data-testid": "bump", onclick: move |_| count += 1, "count: {count}" }
        }
    }

    /// The whole harness in one assertion: a click reaches a handler, the signal it sets schedules
    /// a render, and the render reaches the document a query reads.
    #[test]
    fn a_click_runs_the_handler_and_the_new_text_is_readable() {
        let mut page = PageProbe::of(Counter);
        assert_eq!(page.text("[data-testid=bump]").unwrap(), "count: 0");

        page.click("[data-testid=bump]").unwrap();

        assert_eq!(page.text("[data-testid=bump]").unwrap(), "count: 1");
    }

    #[component]
    fn Rows() -> Element {
        let mut rows = use_signal(|| vec!["alpha", "beta", "gamma"]);

        rsx! {
            button { "data-testid": "drop", onclick: move |_| { rows.write().remove(1); }, "drop" }
            ul {
                for row in rows() {
                    li { class: "row", "{row}" }
                }
            }
        }
    }

    /// A list shrinking exercises the mutations a counter never reaches — the placeholder that
    /// holds a list's position, and the removal of a node from the middle of it.
    #[test]
    fn removing_a_list_item_removes_it_from_the_document() {
        let mut page = PageProbe::of(Rows);
        assert_eq!(page.count(".row").unwrap(), 3);
        assert_eq!(page.text("ul").unwrap(), "alphabetagamma");

        page.click("[data-testid=drop]").unwrap();

        assert_eq!(page.count(".row").unwrap(), 2);
        assert_eq!(
            page.text("ul").unwrap(),
            "alphagamma",
            "the middle row is the one that went, and order survived the removal"
        );
    }

    #[component]
    fn Growing() -> Element {
        let mut rows = use_signal(|| vec!["only"]);

        rsx! {
            button { "data-testid": "add", onclick: move |_| rows.write().insert(0, "first"), "add" }
            ul {
                for row in rows() {
                    li { class: "row", "{row}" }
                }
            }
        }
    }

    /// Inserting at the head is the case that tells an append-only tree from a real one.
    #[test]
    fn an_item_inserted_at_the_head_lands_before_the_one_already_there() {
        let mut page = PageProbe::of(Growing);

        page.click("[data-testid=add]").unwrap();

        assert_eq!(page.text("ul").unwrap(), "firstonly");
    }

    #[component]
    fn Toggling() -> Element {
        let mut open = use_signal(|| false);

        rsx! {
            button { "data-testid": "toggle", onclick: move |_| open.toggle(), "toggle" }
            if open() {
                p { "data-testid": "panel", "shown" }
            }
        }
    }

    #[test]
    fn a_conditional_branch_appears_and_disappears() {
        let mut page = PageProbe::of(Toggling);
        assert_eq!(page.count("[data-testid=panel]").unwrap(), 0);

        page.click("[data-testid=toggle]").unwrap();
        assert_eq!(page.text("[data-testid=panel]").unwrap(), "shown");

        page.click("[data-testid=toggle]").unwrap();
        assert_eq!(
            page.count("[data-testid=panel]").unwrap(),
            0,
            "the branch has to leave the document, not merely stop being rendered into"
        );
    }

    #[component]
    fn Inert() -> Element {
        rsx! { div { "data-testid": "inert", "nothing happens here" } }
    }

    /// The trap this harness exists to avoid: dispatching at an element that listens for nothing
    /// does nothing, and a test that then asserts on the unchanged document passes for the wrong
    /// reason.
    #[test]
    fn clicking_something_that_listens_for_nothing_is_refused() {
        let mut page = PageProbe::of(Inert);

        assert_eq!(
            page.click("[data-testid=inert]"),
            Err(ProbeError::NoListener {
                selector: "[data-testid=inert]".to_string(),
                event: "click".to_string(),
            })
        );
    }

    #[test]
    fn a_selector_that_matches_nothing_reports_the_document_it_searched() {
        let page = PageProbe::of(Counter);

        let Err(ProbeError::NoSuchElement { outline, .. }) = page.text("[data-testid=absent]")
        else {
            panic!("a missing element must not read as an empty one");
        };
        assert!(
            outline.contains("data-testid"),
            "the failure has to show what was there instead, got:\n{outline}"
        );
    }

    #[component]
    fn Nested() -> Element {
        rsx! {
            div { "data-testid": "outer",
                span { "data-testid": "inner", "text" }
            }
        }
    }

    /// A template's root is addressed when it is loaded, but a static node inside it is not —
    /// dioxus never needs to name it again. Saying so beats reporting it as absent when it is
    /// plainly in the outline.
    #[test]
    fn an_element_with_no_id_is_reported_as_unaddressable_rather_than_missing() {
        let page = PageProbe::of(Nested);

        assert_eq!(
            page.find("[data-testid=inner]"),
            Err(ProbeError::NotAddressable {
                selector: "[data-testid=inner]".to_string(),
            })
        );
        assert_eq!(
            page.text("[data-testid=inner]").unwrap(),
            "text",
            "it is still readable — only unclickable"
        );
    }

    #[component]
    fn Renaming() -> Element {
        let mut label = use_signal(|| "before".to_string());

        rsx! {
            button {
                "data-testid": "rename",
                "data-state": "{label}",
                onclick: move |_| label.set("after".to_string()),
                "go"
            }
        }
    }

    #[test]
    fn a_changed_attribute_is_visible_to_a_query() {
        let mut page = PageProbe::of(Renaming);
        assert_eq!(
            page.attribute("[data-testid=rename]", "data-state")
                .unwrap(),
            Some("before".to_string())
        );

        page.click("[data-testid=rename]").unwrap();

        assert_eq!(
            page.attribute("[data-testid=rename]", "data-state")
                .unwrap(),
            Some("after".to_string())
        );
    }

    #[component]
    fn Blocking() -> Element {
        rsx! { a { onclick: |event: Event<MouseData>| event.prevent_default(), "link" } }
    }

    /// The answer the page blocks on. A probe returns it so a test can assert navigation was
    /// stopped, which is not visible in the document at all.
    #[test]
    fn a_handler_preventing_the_default_says_so_through_the_probe() {
        let mut page = PageProbe::of(Blocking);

        assert!(page.click("a").unwrap().prevent_default());
    }
}
