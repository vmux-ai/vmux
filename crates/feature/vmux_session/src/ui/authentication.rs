use dioxus::prelude::*;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use super::state::Chat;
use crate::onboarding::ChatAuthenticate;

#[component]
pub(super) fn Authentication(chat: Chat) -> Element {
    let methods = chat.authentication_methods();
    let pending = chat.authentication_pending();
    let key_state =
        vmux_ui::hooks::UiStateBinding::<crate::onboarding::KeyEntryUiState>::use_context()
            .state
            .read()
            .clone();
    rsx! {
        section { class: "flex flex-col items-start gap-4 py-3",
            span { class: "text-xs font-medium text-muted-foreground", "vmux" }
            p { class: "text-sm text-muted-foreground", {translate("onboarding-auth-question")} }
            if pending {
                p { class: "text-sm text-muted-foreground", {translate("onboarding-auth-pending")} }
                vmux_ui::components::skeleton::Skeleton { class: "h-10 w-48" }
            } else {
                div { class: "flex flex-wrap gap-2",
                    for method in methods {
                        button {
                            key: "{method.id}",
                            r#type: "button",
                            class: "rounded-xl bg-muted/40 px-4 py-3 text-left text-sm hover:bg-muted/60",
                            title: method.description,
                            onclick: move |_| { let _ = send(&ChatAuthenticate { method_id: method.id.clone() }); },
                            "{method.name}"
                        }
                    }
                }
            }
            p { class: "text-xs text-muted-foreground", {translate("onboarding-auth-hint")} }
            if !key_state.key_variable.is_empty() {
                vmux_ui::components::api_key::ApiKeyEntry {
                    pending: key_state.pending,
                    failed: key_state.failed,
                    on_submit: move |key| { let _ = send(&crate::onboarding::ChatSaveKey { key }); },
                }
            }
            button {
                r#type: "button",
                class: "self-start rounded-lg px-3 py-2 text-xs text-muted-foreground hover:bg-muted/40",
                onclick: move |_| { let _ = send(&crate::onboarding::ChatCancelSetup); },
                {translate("common-cancel")}
            }
            button {
                r#type: "button",
                class: "self-start rounded-lg px-3 py-2 text-xs text-muted-foreground hover:bg-muted/40",
                onclick: move |_| vmux_ui::navigation::PageNavigation::replace("vmux://sessions/inbox?setup=1"),
                {translate("agent-error-open-agents")}
            }
        }
    }
}
