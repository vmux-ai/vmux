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
    let selected_repository = use_signal(String::new);
    let private = use_signal(|| true);
    let preferred_provider = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| search.strip_prefix("?provider=").map(str::to_string))
        .unwrap_or_default();

    let _snapshot_listener =
        use_bin_event_listener::<ToolsSnapshot, _>(TOOLS_SNAPSHOT_EVENT, move |event| {
            if pending() == Some(VaultAction::ConnectGithub)
                && (!event.vault.github_owner.is_empty() || !event.vault.error.is_empty())
            {
                pending.set(None);
            }
            snapshot.set(event);
            loaded.set(true);
        });
    let _action_listener =
        use_bin_event_listener::<VaultActionResult, _>(VAULT_ACTION_RESULT_EVENT, move |result| {
            if result.action == VaultAction::ConnectGithub && result.success {
                let mut current = snapshot();
                current.vault.github_owner = result.message.clone();
                current.vault.error.clear();
                snapshot.set(current);
                notice.set(None);
            } else {
                pending.set(None);
                notice.set(Some(result));
            }
        });

    use_effect(move || {
        locale();
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(&translate("vault-title"));
        }
        request_snapshot(false);
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
                            request_snapshot(false);
                        },
                        {translate("common-refresh")}
                    }
                }
            }
            ManagerList {
                if !loaded() {
                    ManagerSpinner { detail: translate("common-loading") }
                } else {
                    if let Some(result) = notice().filter(|result| result.success || !result.message.is_empty()) {
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
                    VaultPanel {
                        vault: current.vault.clone(),
                        repository,
                        selected_repository,
                        private,
                        pending,
                        preferred_provider,
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
    selected_repository: Signal<String>,
    private: Signal<bool>,
    pending: Signal<Option<VaultAction>>,
    preferred_provider: String,
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
    let github_card_class = if preferred_provider == "github" {
        "rounded-xl bg-background/35 p-4 ring-2 ring-inset ring-primary/35"
    } else {
        "rounded-xl bg-background/35 p-4 ring-1 ring-inset ring-foreground/10"
    };
    let cloud_card_class = if preferred_provider == "cloud_folder" {
        "rounded-xl bg-background/35 p-4 ring-2 ring-inset ring-primary/35"
    } else {
        "rounded-xl bg-background/35 p-4 ring-1 ring-inset ring-foreground/10"
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
                div { class: "mt-5 grid gap-3 lg:grid-cols-2",
                    div { class: "{github_card_class}",
                        div { class: "flex items-start gap-3",
                            svg { class: "mt-0.5 h-5 w-5 shrink-0 text-foreground/75", view_box: "0 0 24 24", fill: "currentColor",
                                path { d: "M12 .7a11.3 11.3 0 0 0-3.57 22.02c.57.1.78-.25.78-.55v-2.16c-3.18.69-3.85-1.35-3.85-1.35-.52-1.32-1.27-1.67-1.27-1.67-1.04-.71.08-.7.08-.7 1.15.08 1.75 1.18 1.75 1.18 1.02 1.75 2.68 1.24 3.33.95.1-.74.4-1.24.73-1.53-2.54-.29-5.21-1.27-5.21-5.65 0-1.25.45-2.27 1.18-3.07-.12-.29-.51-1.45.11-3.03 0 0 .96-.31 3.11 1.17A10.8 10.8 0 0 1 12 5.93c.96 0 1.92.13 2.82.38 2.15-1.48 3.11-1.17 3.11-1.17.62 1.58.23 2.74.11 3.03.73.8 1.18 1.82 1.18 3.07 0 4.39-2.68 5.35-5.23 5.64.41.36.78 1.06.78 2.14v3.15c0 .3.21.66.79.55A11.3 11.3 0 0 0 12 .7Z" }
                            }
                            div { class: "min-w-0 flex-1",
                                div { class: "text-sm font-medium text-foreground", {translate("vault-github")} }
                                div { class: "mt-0.5 text-xs text-muted-foreground/70", {translate("vault-github-description")} }
                            }
                            if vault.github_owner.is_empty() {
                                ManagerButton {
                                    variant: ManagerButtonVariant::Primary,
                                    disabled: pending().is_some(),
                                    onclick: move |_| send_action(
                                        pending,
                                        VaultAction::ConnectGithub,
                                        String::new(),
                                        true,
                                    ),
                                    if pending() == Some(VaultAction::ConnectGithub) {
                                        span { class: "flex items-center gap-2",
                                            span { class: "h-3 w-3 animate-spin rounded-full border-2 border-current/25 border-t-current" }
                                            {translate("common-loading")}
                                        }
                                    } else {
                                        {translate("vault-connect-github")}
                                    }
                                }
                            }
                        }
                        if !vault.github_owner.is_empty() {
                            div { class: "mt-3 flex items-center gap-2 text-[10px] text-emerald-700 dark:text-emerald-300",
                                span {
                                    {translate_with(
                                        "vault-connected-as",
                                        &[("name", TranslationValue::String(&vault.github_owner))],
                                    )}
                                }
                                if pending() == Some(VaultAction::ConnectGithub) {
                                    span { class: "h-3 w-3 animate-spin rounded-full border-2 border-current/25 border-t-current" }
                                }
                            }
                            div { class: "mt-3 flex gap-2",
                                select {
                                    class: "min-w-0 flex-1 rounded-xl bg-background/55 px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 focus:ring-primary/40",
                                    value: selected_repository(),
                                    onchange: move |event| selected_repository.set(event.value()),
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
                                ManagerButton {
                                    variant: ManagerButtonVariant::Secondary,
                                    disabled: pending().is_some() || selected_repository().is_empty(),
                                    onclick: move |_| send_action(
                                        pending,
                                        VaultAction::Connect,
                                        selected_repository(),
                                        true,
                                    ),
                                    {translate("vault-use-repository")}
                                }
                            }
                            div { class: "mt-3 flex gap-2",
                                input {
                                    class: "min-w-0 flex-1 rounded-xl bg-background/55 px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-primary/40",
                                    value: repository(),
                                    placeholder: translate("vault-repository-name"),
                                    oninput: move |event| repository.set(event.value()),
                                }
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
                            }
                            label { class: "mt-3 flex cursor-pointer items-center gap-2 text-xs text-muted-foreground",
                                input {
                                    r#type: "checkbox",
                                    checked: private(),
                                    onchange: move |event| private.set(event.checked()),
                                }
                                {translate("vault-private")}
                            }
                            if !private() {
                                div { class: "mt-2 text-[10px] text-amber-600 dark:text-amber-300", {translate("vault-public-warning")} }
                            }
                        }
                    }
                    div { class: "{cloud_card_class}",
                        div { class: "flex items-start gap-3",
                            svg { class: "mt-0.5 h-5 w-5 shrink-0 text-foreground/75", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                            }
                            div { class: "min-w-0 flex-1",
                                div { class: "text-sm font-medium text-foreground", {translate("vault-cloud-folder")} }
                                div { class: "mt-0.5 text-xs text-muted-foreground/70", {translate("vault-cloud-folder-description")} }
                            }
                        }
                        div { class: "mt-4",
                            ManagerButton {
                                variant: ManagerButtonVariant::Secondary,
                                disabled: pending().is_some(),
                                onclick: move |_| send_action(
                                    pending,
                                    VaultAction::ConnectFolder,
                                    String::new(),
                                    true,
                                ),
                                {translate("vault-choose-folder")}
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

fn request_snapshot(load_repositories: bool) {
    let _ = try_cef_bin_emit_rkyv(&VaultRefreshRequest { load_repositories });
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
        VaultAction::ConnectGithub => "vault-result-github-connected",
        VaultAction::ConnectFolder => "vault-result-folder-connected",
    })
}
