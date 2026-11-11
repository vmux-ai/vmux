use dioxus::prelude::*;
use vmux_ui::i18n::translate;

use super::state::Chat;

#[component]
pub(super) fn Authentication(chat: Chat) -> Element {
    rsx! {
        super::setup::Turn {
            p { class: "text-sm leading-relaxed", {translate("onboarding-auth-question")} }
            if chat.authentication_pending() {
                p { class: "text-sm leading-relaxed", {translate("onboarding-auth-pending")} }
            }
            p { class: "text-sm leading-relaxed", {translate("onboarding-auth-hint")} }
        }
    }
}
