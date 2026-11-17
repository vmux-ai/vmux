use dioxus::prelude::*;
use vmux_ui::i18n::translate;

use super::state::Chat;

#[component]
pub(super) fn Authentication(chat: Chat) -> Element {
    if chat.authentication_pending() {
        return rsx! { super::setup::Turn { agent_name: chat.header_name(),
            p { class: "text-sm leading-relaxed", {vmux_ui::i18n::translate_with("onboarding-login-finish-agent", &[("agent", vmux_ui::i18n::TranslationValue::String(&chat.header_name()))])} }
        } };
    }
    rsx! {
        super::setup::Turn { agent_name: chat.header_name(),
            p { class: "text-sm leading-relaxed", {translate(if chat.key_input_requested() { "onboarding-key-question" } else { "onboarding-auth-question" })} }
            if chat.setup_prompt().is_some() {
                p { class: "text-sm leading-relaxed", {translate("onboarding-auth-hint")} }
            }
        }
    }
}
