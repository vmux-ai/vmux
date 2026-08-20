//! What a page has to draw for itself when its host draws no chrome around it.
//!
//! The desktop wraps every page in a shell — tabs, a stack, a way back to the launcher — so a page
//! rendered there never has to offer one. A phone has no shell: the page is the whole window, and
//! anything the reader needs in order to leave has to come from inside it.
//!
//! Rather than split the page or give it a prop it cannot be handed (a natively-hosted page is a
//! `fn() -> Element`), the host that lacks chrome says so through context. A host with chrome
//! provides nothing, `PageBack::of()` answers `None`, and the affordance does not render — so the
//! desktop is untouched by the phone needing one.

use dioxus::prelude::*;

/// How a page returns to whatever it was opened from.
///
/// Provided by a host that draws no chrome of its own. Absent everywhere else.
#[derive(Clone, Copy, PartialEq)]
pub struct PageBack(EventHandler<()>);

impl PageBack {
    pub fn new(on_back: EventHandler<()>) -> Self {
        Self(on_back)
    }

    /// The way out of this page, if the host offers one.
    pub fn of() -> Option<Self> {
        try_consume_context::<Self>()
    }

    pub fn go(&self) {
        self.0.call(());
    }
}

/// The chevron a page shows when it is the whole window.
///
/// Renders nothing unless a [`PageBack`] is in context, so a page can place it unconditionally.
#[component]
pub fn BackButton(#[props(default)] class: String) -> Element {
    let Some(back) = PageBack::of() else {
        return rsx! {};
    };
    let class = if class.is_empty() {
        "-ml-1 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-muted-foreground active:bg-accent".to_string()
    } else {
        class
    };
    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            // Named for the app that first needed it and translated into all 116 locales there.
            // Minting a `common-` id for the same word would mean 116 untranslated strings.
            aria_label: crate::i18n::translate("mobile-chat-back"),
            onclick: move |_| back.go(),
            svg {
                class: "h-5 w-5",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "m15 18-6-6 6-6" }
            }
        }
    }
}
