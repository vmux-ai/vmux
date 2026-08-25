use crate::components::button::IconButton;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub struct PageBack(EventHandler<()>);

impl PageBack {
    pub fn new(on_back: EventHandler<()>) -> Self {
        Self(on_back)
    }

    pub fn of() -> Option<Self> {
        try_consume_context::<Self>()
    }

    pub fn go(&self) {
        self.0.call(());
    }
}

#[component]
pub fn BackButton() -> Element {
    let Some(back) = PageBack::of() else {
        return rsx! {};
    };
    rsx! {
        IconButton {
            label: crate::i18n::translate("mobile-chat-back"),
            paths: vec!["m15 18-6-6 6-6".to_string()],
            onclick: move |_| back.go(),
        }
    }
}
