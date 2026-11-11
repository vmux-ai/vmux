use dioxus::prelude::*;
use vmux_ui::i18n::translate;

use super::state::Chat;

#[component]
pub(super) fn Authentication(chat: Chat) -> Element {
    if chat.authentication_pending() {
        return rsx! { super::setup::ProgressTurn { phase: super::setup::Phase::Authentication } };
    }
    rsx! {
        super::setup::Turn {
            p { class: "text-sm leading-relaxed", {translate("onboarding-auth-question")} }
            if chat.setup_prompt().is_some() {
                p { class: "text-sm leading-relaxed", {translate("onboarding-auth-hint")} }
            }
        }
    }
}
