//! How a page offers a way out of itself.
//!
//! The desktop surrounds every page with its own furniture — tabs, a stack, a launcher one
//! keystroke away — so a page rendered there never has to offer an exit. A phone surrounds it with
//! nothing: the page is the whole window, and anything the reader needs in order to leave has to
//! come from inside it.
//!
//! Rather than split the page or give it a prop it cannot be handed (a natively-hosted page is a
//! `fn() -> Element`), a host that surrounds a page with nothing says so through context. Every
//! other host provides nothing, `PageBack::of` answers `None`, and the affordance does not render
//! — so the desktop is untouched by the phone needing one.

use crate::components::icon::Icon;
use crate::util::merge_class;
use dioxus::prelude::*;

const BACK_BUTTON: &str = "-ml-1 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-muted-foreground active:bg-accent";

/// How a page returns to whatever it was opened from.
///
/// Provided by a host that surrounds the page with nothing of its own. Absent everywhere else.
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
    rsx! {
        button {
            class: merge_class(BACK_BUTTON, Some(&class)),
            r#type: "button",
            // Named for the app that first needed it and translated into all 116 locales there.
            // Minting a `common-` id for the same word would mean 116 untranslated strings.
            aria_label: crate::i18n::translate("mobile-chat-back"),
            onclick: move |_| back.go(),
            Icon { class: "h-5 w-5",
                path { d: "m15 18-6-6 6-6" }
            }
        }
    }
}
