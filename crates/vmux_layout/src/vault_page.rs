#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::tools::{TOOLS_SNAPSHOT_EVENT, ToolsSnapshot};
use vmux_core::vault::{
    VAULT_ACTION_RESULT_EVENT, VaultAction, VaultActionRequest, VaultActionResult,
    VaultRefreshRequest, VaultSnapshot,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerList, ManagerPage, ManagerSpinner,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let mut snapshot = use_signal(ToolsSnapshot::default);
    let mut loaded = use_signal(|| false);
    let mut pending = use_signal(|| None::<VaultAction>);
    let mut notice = use_signal(|| None::<VaultActionResult>);
    let repository = use_signal(|| "vmux-vault".to_string());
    let private = use_signal(|| true);

    let _snapshot_listener =
        use_bin_event_listener::<ToolsSnapshot, _>(TOOLS_SNAPSHOT_EVENT, move |event| {
            snapshot.set(event);
            loaded.set(true);
        });
    let _action_listener =
        use_bin_event_listener::<VaultActionResult, _>(VAULT_ACTION_RESULT_EVENT, move |result| {
            pending.set(None);
            notice.set(Some(result));
            request_snapshot();
        });

    use_effect(move || {
        locale();
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(&translate("vault-title"));
        }
        request_snapshot();
    });

    let current = snapshot();
    rsx! {
        ManagerPage {
            header { class: "shrink-0 border-b border-foreground/[0.07] px-5 py-3",
                div { class: "flex items-center gap-3",
                    h1 { class: "text-base font-semibold tracking-tight", {translate("vault-title")} }
                    div { class: "flex-1" }
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        onclick: move |_| {
                            loaded.set(false);
                            request_snapshot();
                        },
                        {translate("common-refresh")}
                    }
                }
            }
            ManagerList {
                if !loaded() {
                    ManagerSpinner { detail: translate("common-loading") }
                } else {
                    VaultPanel {
                        vault: current.vault.clone(),
                        repository,
                        private,
                        pending,
                    }
                }
                if let Some(result) = notice() {
                    div {
                        class: if result.success {
                            "rounded-xl bg-emerald-400/10 px-4 py-3 text-xs text-emerald-700 ring-1 ring-inset ring-emerald-400/20 dark:text-emerald-300"
                        } else {
                            "rounded-xl bg-ansi-1/10 px-4 py-3 text-xs text-ansi-1 ring-1 ring-inset ring-ansi-1/20"
                        },
                        if result.success {
                            {action_result_message(result.action)}
                        } else {
                            "{result.message}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn VaultPanel(
    vault: VaultSnapshot,
    repository: Signal<String>,
    private: Signal<bool>,
    pending: Signal<Option<VaultAction>>,
) -> Element {
    let is_connected = vault.initialized && !vault.remote.is_empty();
    let status = if vault.dirty > 0 {
        translate_with(
            "vault-change-count",
            &[("count", TranslationValue::Number(vault.dirty as i64))],
        )
    } else {
        translate("vault-clean")
    };
    rsx! {
        div { class: "rounded-2xl bg-foreground/[0.035] p-5 ring-1 ring-inset ring-foreground/10",
            div { class: "flex items-start gap-4",
                div { class: "grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-violet-500/10 text-violet-700 ring-1 ring-inset ring-violet-500/20 dark:text-violet-300",
                    svg { class: "h-5 w-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M12 3 4.5 6v5.5c0 4.7 3.2 8.1 7.5 9.5 4.3-1.4 7.5-4.8 7.5-9.5V6Z" }
                        path { d: "m9 12 2 2 4-4" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "font-medium text-foreground/95", {translate("vault-title")} }
                    if is_connected {
                        div { class: "truncate text-xs text-muted-foreground/70", "{vault.remote}" }
                        div { class: "mt-1 flex gap-2 text-[10px] text-muted-foreground/60",
                            if !vault.branch.is_empty() {
                                span { "{vault.branch}" }
                            }
                            span { "{status}" }
                            if vault.ahead > 0 {
                                span { "↑{vault.ahead}" }
                            }
                            if vault.behind > 0 {
                                span { "↓{vault.behind}" }
                            }
                        }
                    } else {
                        div { class: "text-xs text-muted-foreground/70", {translate("vault-description")} }
                        div { class: "mt-1 truncate text-[10px] text-muted-foreground/55", "{vault.root}" }
                    }
                }
                if is_connected {
                    ManagerButton {
                        variant: ManagerButtonVariant::Primary,
                        disabled: pending().is_some(),
                        onclick: move |_| send_action(
                            pending,
                            VaultAction::Sync,
                            String::new(),
                            true,
                        ),
                        {translate("vault-sync")}
                    }
                }
            }
            if !is_connected {
                div { class: "mt-5 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]",
                    input {
                        class: "min-w-0 rounded-xl bg-background/55 px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-primary/40",
                        value: repository(),
                        placeholder: "vmux-vault",
                        oninput: move |event| repository.set(event.value()),
                    }
                    div { class: "flex gap-2",
                        ManagerButton {
                            variant: ManagerButtonVariant::Primary,
                            disabled: pending().is_some(),
                            onclick: move |_| send_action(
                                pending,
                                VaultAction::Create,
                                repository(),
                                private(),
                            ),
                            {translate("vault-create")}
                        }
                        ManagerButton {
                            variant: ManagerButtonVariant::Secondary,
                            disabled: pending().is_some(),
                            onclick: move |_| send_action(
                                pending,
                                VaultAction::Connect,
                                repository(),
                                private(),
                            ),
                            {translate("vault-connect")}
                        }
                    }
                }
                div { class: "mt-3 flex flex-wrap items-center gap-3",
                    label { class: "flex cursor-pointer items-center gap-2 text-xs text-muted-foreground",
                        input {
                            r#type: "checkbox",
                            checked: private(),
                            onchange: move |event| private.set(event.checked()),
                        }
                        {translate("vault-private")}
                    }
                    if !private() {
                        span { class: "text-[10px] text-amber-600 dark:text-amber-300", {translate("vault-public-warning")} }
                    }
                }
                if !vault.repositories.is_empty() {
                    select {
                        class: "mt-3 w-full rounded-xl bg-background/55 px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/40",
                        value: "",
                        onchange: move |event| {
                            if !event.value().is_empty() {
                                repository.set(event.value());
                            }
                        },
                        option { value: "", {translate("vault-choose-repository")} }
                        for candidate in vault.repositories.iter() {
                            option { value: "{candidate.url}",
                                "{candidate.name}"
                                if candidate.empty {
                                    " · "
                                    {translate("vault-empty")}
                                }
                            }
                        }
                    }
                }
                if !vault.error.is_empty() {
                    div { class: "mt-3 text-[10px] text-amber-600 dark:text-amber-300", "{vault.error}" }
                }
            }
        }
    }
}

fn request_snapshot() {
    let _ = try_cef_bin_emit_rkyv(&VaultRefreshRequest);
}

fn send_action(
    mut pending: Signal<Option<VaultAction>>,
    action: VaultAction,
    repository: String,
    private: bool,
) {
    pending.set(Some(action));
    let _ = try_cef_bin_emit_rkyv(&VaultActionRequest {
        action,
        repository,
        private,
    });
}

fn action_result_message(action: VaultAction) -> String {
    translate(match action {
        VaultAction::Create => "vault-result-created",
        VaultAction::Connect => "vault-result-connected",
        VaultAction::Sync => "vault-result-synced",
    })
}
