#![allow(non_snake_case)]

use dioxus::prelude::*;
use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use vmux_core::tools::{TOOLS_SNAPSHOT_EVENT, ToolsSnapshot};
use vmux_core::vault::{
    VAULT_ACTION_RESULT_EVENT, VaultAction, VaultActionRequest, VaultActionResult,
    VaultRefreshRequest, VaultSnapshot,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerList, ManagerPage, ManagerSelect,
    ManagerSelectItem, ManagerSelectItemKind, ManagerSpinner,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemoteProvider {
    Github,
    GoogleDrive,
    Dropbox,
    OneDrive,
}

impl RemoteProvider {
    const ALL: [Self; 4] = [
        Self::Github,
        Self::GoogleDrive,
        Self::Dropbox,
        Self::OneDrive,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::GoogleDrive => "Google Drive",
            Self::Dropbox => "Dropbox",
            Self::OneDrive => "OneDrive",
        }
    }

    fn is_github(self) -> bool {
        self == Self::Github
    }
}

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let mut snapshot = use_signal(ToolsSnapshot::default);
    let mut loaded = use_signal(|| false);
    let mut pending = use_signal(|| None::<VaultAction>);
    let mut notice = use_signal(|| None::<VaultActionResult>);
    let mut repositories_requested = use_signal(|| false);
    let mut repository = use_signal(|| "vmux-vault".to_string());
    let mut selected_owner = use_signal(|| None::<String>);
    let selected_repository = use_signal(|| None::<String>);
    let preferred_provider = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| search.strip_prefix("?provider=").map(str::to_string))
        .unwrap_or_default();
    let selected_provider = use_signal(|| match preferred_provider.as_str() {
        "github" => Some(RemoteProvider::Github),
        "google_drive" | "cloud_folder" => Some(RemoteProvider::GoogleDrive),
        "dropbox" => Some(RemoteProvider::Dropbox),
        "onedrive" => Some(RemoteProvider::OneDrive),
        _ => None,
    });
    let mut cloud_root = use_signal(String::new);
    let private = use_signal(|| true);

    let _snapshot_listener =
        use_bin_event_listener::<ToolsSnapshot, _>(TOOLS_SNAPSHOT_EVENT, move |event| {
            if pending() == Some(VaultAction::ConnectGithub)
                && (event.vault.repositories_loaded || !event.vault.error.is_empty())
            {
                pending.set(None);
            }
            let needs_repositories = !event.vault.github_owner.is_empty()
                && (!event.vault.initialized || event.vault.remote.is_empty())
                && !event.vault.repositories_loaded;
            if needs_repositories && !repositories_requested() {
                repositories_requested.set(true);
                request_snapshot(true);
            } else if !needs_repositories {
                repositories_requested.set(false);
            }
            if !event.vault.github_owner.is_empty()
                && selected_owner()
                    .as_ref()
                    .is_none_or(|owner| !event.vault.github_owners.contains(owner))
            {
                selected_owner.set(Some(event.vault.github_owner.clone()));
            }
            if event.vault.repositories_loaded && repository() == "vmux-vault" {
                let owner = selected_owner().unwrap_or_else(|| event.vault.github_owner.clone());
                repository.set(suggested_repository_name(&owner, &event.vault.repositories));
            }
            snapshot.set(event);
            loaded.set(true);
        });
    let _action_listener =
        use_bin_event_listener::<VaultActionResult, _>(VAULT_ACTION_RESULT_EVENT, move |result| {
            if result.action == VaultAction::ConnectCloud && result.success {
                cloud_root.set(result.message);
                pending.set(None);
                notice.set(None);
            } else if result.action == VaultAction::ConnectGithub && result.success {
                let mut current = snapshot();
                current.vault.github_owner = result.message.clone();
                current.vault.github_owners = vec![result.message.clone()];
                current.vault.repositories.clear();
                current.vault.repositories_loaded = false;
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
        let Some(window) = web_sys::window() else {
            return;
        };
        let location = window.location();
        if location.protocol().ok().as_deref() == Some("vmux:") {
            let search = location.search().unwrap_or_default();
            let _ = location.replace(&format!("https://vault.vmux.ai/{search}"));
            return;
        }
        if let Some(document) = window.document() {
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
                        selected_owner,
                        selected_repository,
                        selected_provider,
                        repositories_requested,
                        cloud_root,
                        private,
                        pending,
                        notice,
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
    selected_owner: Signal<Option<String>>,
    selected_repository: Signal<Option<String>>,
    selected_provider: Signal<Option<RemoteProvider>>,
    repositories_requested: Signal<bool>,
    cloud_root: Signal<String>,
    private: Signal<bool>,
    pending: Signal<Option<VaultAction>>,
    notice: Signal<Option<VaultActionResult>>,
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
    let provider = selected_provider();
    let authenticated = provider.is_some_and(|provider| {
        if provider.is_github() {
            !vault.github_owner.is_empty() && vault.repositories_loaded
        } else {
            !cloud_root().is_empty()
        }
    });
    let owner = selected_owner().unwrap_or_else(|| vault.github_owner.clone());
    let owner_items = vault
        .github_owners
        .iter()
        .map(|owner| ManagerSelectItem {
            value: owner.clone(),
            label: owner.clone(),
            kind: if owner == &vault.github_owner {
                ManagerSelectItemKind::User
            } else {
                ManagerSelectItemKind::Organization
            },
        })
        .collect::<Vec<_>>();
    let owner_prefix = format!("{owner}/");
    let repository_items = vault
        .repositories
        .iter()
        .filter(|repository| repository.name.starts_with(&owner_prefix))
        .map(|repository| ManagerSelectItem {
            value: repository.url.clone(),
            label: if repository.empty {
                format!("{} · {}", repository.name, translate("vault-empty"))
            } else {
                repository.name.clone()
            },
            kind: ManagerSelectItemKind::Default,
        })
        .collect::<Vec<_>>();
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
                    if !is_connected || vault.encrypted {
                        div { class: "mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground/70",
                            svg { class: "h-3 w-3 shrink-0", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                rect { x: "5", y: "11", width: "14", height: "10", rx: "2" }
                                path { d: "M8 11V7a4 4 0 0 1 8 0v4" }
                            }
                            {translate("vault-encrypted")}
                        }
                    }
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
                div { class: "mt-5 flex flex-col gap-3",
                    VaultStep { number: 1, active: true, complete: provider.is_some(),
                        div { class: "grid gap-2 sm:grid-cols-2 lg:grid-cols-4",
                            for option in RemoteProvider::ALL {
                                button {
                                    class: if provider == Some(option) {
                                        "flex items-center gap-3 rounded-xl bg-primary/10 px-3 py-3 text-left text-foreground ring-2 ring-inset ring-primary/35"
                                    } else {
                                        "flex items-center gap-3 rounded-xl bg-background/45 px-3 py-3 text-left text-muted-foreground ring-1 ring-inset ring-foreground/10 transition-colors hover:bg-foreground/[0.07] hover:text-foreground"
                                    },
                                    onclick: move |_| {
                                        selected_provider.set(Some(option));
                                        cloud_root.set(String::new());
                                        selected_repository.set(None);
                                        if option.is_github() {
                                            if !vault.repositories_loaded
                                                && !repositories_requested()
                                            {
                                                repositories_requested.set(true);
                                                request_snapshot(true);
                                            }
                                        } else {
                                            repository.set("vmux-vault".to_string());
                                        }
                                    },
                                    ProviderIcon { provider: option }
                                    span { class: "min-w-0 flex-1 truncate text-xs font-medium", "{option.name()}" }
                                    if provider == Some(option) {
                                        svg { class: "h-3.5 w-3.5 shrink-0 text-primary", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                                            path { d: "m5 12 4 4L19 6" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    VaultStep { number: 2, active: provider.is_some(), complete: authenticated,
                        if let Some(provider) = provider {
                            div { class: "flex min-w-0 items-center gap-3",
                                ProviderIcon { provider }
                                div { class: "min-w-0 flex-1",
                                    div { class: "text-xs font-medium text-foreground", "{provider.name()}" }
                                    if provider.is_github() && !vault.github_owner.is_empty() {
                                        div { class: "truncate text-[10px] text-emerald-700 dark:text-emerald-300",
                                            {translate_with(
                                                "vault-connected-as",
                                                &[("name", TranslationValue::String(&vault.github_owner))],
                                            )}
                                        }
                                    } else if !provider.is_github() && !cloud_root().is_empty() {
                                        div { class: "truncate text-[10px] text-emerald-700 dark:text-emerald-300", "{cloud_root()}" }
                                    } else {
                                        div { class: "text-[10px] text-muted-foreground/60", {translate("vault-not-connected")} }
                                    }
                                }
                                if !authenticated {
                                    ManagerButton {
                                        variant: ManagerButtonVariant::Primary,
                                        disabled: pending().is_some()
                                            || (provider.is_github() && repositories_requested()),
                                        onclick: move |_| send_action(
                                            pending,
                                            if provider.is_github() {
                                                VaultAction::ConnectGithub
                                            } else {
                                                VaultAction::ConnectCloud
                                            },
                                            if provider.is_github() {
                                                String::new()
                                            } else {
                                                provider.name().to_string()
                                            },
                                            true,
                                        ),
                                        if pending().is_some_and(|action| {
                                            action == VaultAction::ConnectGithub
                                                || action == VaultAction::ConnectCloud
                                        }) || (provider.is_github() && repositories_requested()) {
                                            span { class: "flex items-center gap-2",
                                                span { class: "h-3 w-3 animate-spin rounded-full border-2 border-current/25 border-t-current" }
                                                {translate("common-loading")}
                                            }
                                        } else {
                                            if provider.is_github() {
                                                {translate("vault-connect-github")}
                                            } else {
                                                {translate("vault-choose-folder")}
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "py-1 text-xs text-muted-foreground/60", {translate("vault-connect")} }
                        }
                    }
                    VaultStep { number: 3, active: authenticated, complete: false,
                        if authenticated {
                            if provider == Some(RemoteProvider::Github) {
                                div { class: "mb-3",
                                    ManagerSelect {
                                        items: owner_items,
                                        value: Some(owner.clone()),
                                        placeholder: vault.github_owner.clone(),
                                        onselect: move |value: String| {
                                            repository.set(suggested_repository_name(
                                                &value,
                                                &vault.repositories,
                                            ));
                                            selected_owner.set(Some(value));
                                            selected_repository.set(None);
                                        },
                                    }
                                }
                                div { class: "grid gap-3 lg:grid-cols-2",
                                    div { class: "rounded-xl bg-background/45 p-3 ring-1 ring-inset ring-foreground/10",
                                        div { class: "mb-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/60", {translate("vault-create")} }
                                        div { class: "flex gap-2",
                                            div { class: "flex min-w-0 flex-1 items-center rounded-xl bg-background/55 ring-1 ring-inset ring-foreground/10 focus-within:ring-primary/40",
                                                span { class: "shrink-0 pl-3 text-xs text-muted-foreground/60", "{owner}/" }
                                                input {
                                                    class: "min-w-0 flex-1 bg-transparent py-2 pl-0.5 pr-3 text-sm text-foreground outline-none placeholder:text-muted-foreground/50",
                                                    value: repository(),
                                                    placeholder: translate("vault-repository-name"),
                                                    oninput: move |event| repository.set(event.value()),
                                                }
                                            }
                                            ManagerButton {
                                                variant: ManagerButtonVariant::Primary,
                                                disabled: pending().is_some() || owner.is_empty() || repository().trim().is_empty(),
                                                onclick: move |_| send_action(
                                                    pending,
                                                    VaultAction::Create,
                                                    format!("{owner}/{}", repository().trim()),
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
                                    div { class: "rounded-xl bg-background/45 p-3 ring-1 ring-inset ring-foreground/10",
                                        div { class: "mb-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/60", {translate("vault-choose-repository")} }
                                        div { class: "flex gap-2",
                                            div { class: "min-w-0 flex-1",
                                                ManagerSelect {
                                                    items: repository_items,
                                                    value: selected_repository(),
                                                    placeholder: translate("vault-choose-repository"),
                                                    onselect: move |value| selected_repository.set(Some(value)),
                                                }
                                            }
                                            ManagerButton {
                                                variant: ManagerButtonVariant::Secondary,
                                                disabled: pending().is_some() || selected_repository().is_none(),
                                                onclick: move |_| send_action(
                                                    pending,
                                                    VaultAction::Connect,
                                                    selected_repository().unwrap_or_default(),
                                                    true,
                                                ),
                                                {translate("vault-use-repository")}
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "grid gap-3 lg:grid-cols-2",
                                    div { class: "rounded-xl bg-background/45 p-3 ring-1 ring-inset ring-foreground/10",
                                        div { class: "mb-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/60", {translate("vault-create")} }
                                        div { class: "flex gap-2",
                                            input {
                                                class: "min-w-0 flex-1 rounded-xl bg-background/55 px-3 py-2 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-primary/40",
                                                value: repository(),
                                                placeholder: translate("vault-repository-name"),
                                                oninput: move |event| repository.set(event.value()),
                                            }
                                            ManagerButton {
                                                variant: ManagerButtonVariant::Primary,
                                                disabled: pending().is_some() || repository().trim().is_empty(),
                                                onclick: move |_| send_cloud_create(
                                                    pending,
                                                    &cloud_root(),
                                                    repository().trim(),
                                                ),
                                                {translate("vault-create")}
                                            }
                                        }
                                    }
                                    div { class: "rounded-xl bg-background/45 p-3 ring-1 ring-inset ring-foreground/10",
                                        div { class: "mb-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/60", {translate("vault-choose-folder")} }
                                        ManagerButton {
                                            variant: ManagerButtonVariant::Secondary,
                                            disabled: pending().is_some(),
                                            onclick: move |_| send_action(
                                                pending,
                                                VaultAction::ChooseCloudFolder,
                                                cloud_root(),
                                                true,
                                            ),
                                            {translate("vault-choose-folder")}
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "grid gap-3 lg:grid-cols-2",
                                div { class: "rounded-xl bg-background/30 p-3 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/50 ring-1 ring-inset ring-foreground/[0.07]", {translate("vault-create")} }
                                div { class: "rounded-xl bg-background/30 p-3 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/50 ring-1 ring-inset ring-foreground/[0.07]", {translate("vault-choose-folder")} }
                            }
                        }
                    }
                }
                if !vault.error.is_empty() {
                    div { class: "mt-3 text-[10px] text-amber-600 dark:text-amber-300", "{vault.error}" }
                }
            } else {
                PasskeyCard {
                    vault,
                    pending,
                    notice,
                }
            }
        }
    }
}

#[component]
fn VaultStep(number: u8, active: bool, complete: bool, children: Element) -> Element {
    rsx! {
        div { class: if active {
                "rounded-2xl bg-background/35 p-4 ring-1 ring-inset ring-foreground/10 transition-opacity"
            } else {
                "pointer-events-none rounded-2xl bg-background/20 p-4 opacity-40 ring-1 ring-inset ring-foreground/[0.07]"
            },
            div { class: "flex items-start gap-3",
                div { class: if complete {
                        "grid h-6 w-6 shrink-0 place-items-center rounded-full bg-emerald-400/15 text-emerald-700 ring-1 ring-inset ring-emerald-400/25 dark:text-emerald-300"
                    } else if active {
                        "grid h-6 w-6 shrink-0 place-items-center rounded-full bg-primary/10 text-primary ring-1 ring-inset ring-primary/25"
                    } else {
                        "grid h-6 w-6 shrink-0 place-items-center rounded-full bg-foreground/[0.05] text-muted-foreground ring-1 ring-inset ring-foreground/10"
                    },
                    if complete {
                        svg { class: "h-3.5 w-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "m5 12 4 4L19 6" }
                        }
                    } else {
                        span { class: "text-[10px] font-semibold", "{number}" }
                    }
                }
                div { class: "min-w-0 flex-1", {children} }
            }
        }
    }
}

#[component]
fn ProviderIcon(provider: RemoteProvider) -> Element {
    match provider {
        RemoteProvider::Github => rsx! {
            svg { class: "h-5 w-5 shrink-0", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M12 .7a11.3 11.3 0 0 0-3.57 22.02c.57.1.78-.25.78-.55v-2.16c-3.18.69-3.85-1.35-3.85-1.35-.52-1.32-1.27-1.67-1.27-1.67-1.04-.71.08-.7.08-.7 1.15.08 1.75 1.18 1.75 1.18 1.02 1.75 2.68 1.24 3.33.95.1-.74.4-1.24.73-1.53-2.54-.29-5.21-1.27-5.21-5.65 0-1.25.45-2.27 1.18-3.07-.12-.29-.51-1.45.11-3.03 0 0 .96-.31 3.11 1.17A10.8 10.8 0 0 1 12 5.93c.96 0 1.92.13 2.82.38 2.15-1.48 3.11-1.17 3.11-1.17.62 1.58.23 2.74.11 3.03.73.8 1.18 1.82 1.18 3.07 0 4.39-2.68 5.35-5.23 5.64.41.36.78 1.06.78 2.14v3.15c0 .3.21.66.79.55A11.3 11.3 0 0 0 12 .7Z" }
            }
        },
        RemoteProvider::GoogleDrive => rsx! {
            svg { class: "h-5 w-5 shrink-0", view_box: "0 0 24 24", fill: "none",
                path { d: "M8.1 3h7.8l4 7h-7.8Z", fill: "#fbbc04" }
                path { d: "m8.1 3 4 7-4.1 7H4Z", fill: "#34a853" }
                path { d: "M8 17h8l3.9-7h-7.8Z", fill: "#4285f4" }
            }
        },
        RemoteProvider::Dropbox => rsx! {
            svg { class: "h-5 w-5 shrink-0 text-[#0061ff]", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "m6.5 3.5 5.5 3.4-5.5 3.5L1 6.9Zm11 0L23 6.9l-5.5 3.5L12 6.9Zm-11 8L12 15l-5.5 3.4L1 15Zm11 0L23 15l-5.5 3.4L12 15ZM6.6 19.6l5.4-3.4 5.4 3.4L12 23Z" }
            }
        },
        RemoteProvider::OneDrive => rsx! {
            svg { class: "h-5 w-5 shrink-0 text-[#0078d4]", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M9.3 7.3A6 6 0 0 1 19.8 11a4.5 4.5 0 0 1-.3 9H6a5 5 0 0 1-.6-10A5.8 5.8 0 0 1 9.3 7.3Z" }
            }
        },
    }
}

#[component]
fn PasskeyCard(
    vault: VaultSnapshot,
    pending: Signal<Option<VaultAction>>,
    notice: Signal<Option<VaultActionResult>>,
) -> Element {
    let add_vault = vault.clone();
    let unlock_vault = vault.clone();
    rsx! {
        div { class: "mt-4 rounded-xl bg-background/35 p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "flex items-center gap-3",
                svg { class: "h-5 w-5 shrink-0 text-foreground/70", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    circle { cx: "8", cy: "15", r: "4" }
                    path { d: "m11 12 8-8" }
                    path { d: "m18 5 1 1" }
                    path { d: "m15 8 1 1" }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "text-sm font-medium text-foreground", {translate("vault-passkey")} }
                    div { class: "mt-0.5 text-xs text-muted-foreground/70", {translate("vault-passkey-description")} }
                }
                if !vault.passkey_credentials.is_empty() {
                    ManagerButton {
                        variant: ManagerButtonVariant::Primary,
                        disabled: pending().is_some(),
                        onclick: move |_| start_passkey(
                            VaultAction::UnlockPasskey,
                            unlock_vault.clone(),
                            pending,
                            notice,
                        ),
                        {translate("vault-passkey-unlock")}
                    }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    disabled: pending().is_some(),
                    onclick: move |_| start_passkey(
                        VaultAction::AddPasskey,
                        add_vault.clone(),
                        pending,
                        notice,
                    ),
                    {translate("vault-passkey-add")}
                }
            }
        }
    }
}

fn start_passkey(
    action: VaultAction,
    vault: VaultSnapshot,
    mut pending: Signal<Option<VaultAction>>,
    mut notice: Signal<Option<VaultActionResult>>,
) {
    if vault.vault_id.is_empty() || vault.passkey_salt.len() != 32 {
        notice.set(Some(VaultActionResult {
            action,
            success: false,
            message: translate("vault-not-connected"),
        }));
        return;
    }
    pending.set(Some(action));
    notice.set(None);
    spawn(async move {
        let result = match action {
            VaultAction::AddPasskey => create_passkey(&vault.vault_id, &vault.passkey_salt).await,
            VaultAction::UnlockPasskey => {
                get_passkey(&vault.passkey_credentials, &vault.passkey_salt).await
            }
            _ => unreachable!(),
        };
        match result {
            Ok((credential_id, prf_output)) => {
                if let Err(error) = try_cef_bin_emit_rkyv(&VaultActionRequest {
                    action,
                    repository: String::new(),
                    private: true,
                    credential_id,
                    prf_output,
                }) {
                    pending.set(None);
                    notice.set(Some(VaultActionResult {
                        action,
                        success: false,
                        message: error.to_string(),
                    }));
                }
            }
            Err(message) => {
                pending.set(None);
                notice.set(Some(VaultActionResult {
                    action,
                    success: false,
                    message,
                }));
            }
        }
    });
}

async fn create_passkey(vault_id: &str, salt: &[u8]) -> Result<(String, Vec<u8>), String> {
    let public_key = Object::new();
    set_property(&public_key, "challenge", random_bytes(32)?.as_ref())?;

    let rp = Object::new();
    set_property(&rp, "id", &JsValue::from_str("vault.vmux.ai"))?;
    set_property(&rp, "name", &JsValue::from_str("vmux"))?;
    set_property(&public_key, "rp", rp.as_ref())?;

    let user = Object::new();
    set_property(&user, "id", Uint8Array::from(vault_id.as_bytes()).as_ref())?;
    set_property(&user, "name", &JsValue::from_str("vmux"))?;
    set_property(&user, "displayName", &JsValue::from_str("vmux"))?;
    set_property(&public_key, "user", user.as_ref())?;

    let parameters = Array::new();
    for algorithm in [-7, -257] {
        let parameter = Object::new();
        set_property(&parameter, "type", &JsValue::from_str("public-key"))?;
        set_property(&parameter, "alg", &JsValue::from_f64(algorithm as f64))?;
        parameters.push(parameter.as_ref());
    }
    set_property(&public_key, "pubKeyCredParams", parameters.as_ref())?;

    let selection = Object::new();
    set_property(&selection, "residentKey", &JsValue::from_str("required"))?;
    set_property(
        &selection,
        "userVerification",
        &JsValue::from_str("required"),
    )?;
    set_property(&public_key, "authenticatorSelection", selection.as_ref())?;
    set_property(&public_key, "attestation", &JsValue::from_str("none"))?;
    set_property(&public_key, "timeout", &JsValue::from_f64(120_000.0))?;
    set_property(
        &public_key,
        "extensions",
        prf_create_extensions(salt)?.as_ref(),
    )?;

    let options = Object::new();
    set_property(&options, "publicKey", public_key.as_ref())?;
    let credential = call_credentials("create", &options).await?;
    let credential_id = credential_id(&credential)?;
    match prf_output(&credential) {
        Ok(output) => Ok((credential_id, output)),
        Err(_) => get_passkey(&[credential_id], salt).await,
    }
}

async fn get_passkey(credential_ids: &[String], salt: &[u8]) -> Result<(String, Vec<u8>), String> {
    if credential_ids.is_empty() {
        return Err("No encryption-capable passkey is registered".to_string());
    }
    let public_key = Object::new();
    set_property(&public_key, "challenge", random_bytes(32)?.as_ref())?;
    set_property(&public_key, "rpId", &JsValue::from_str("vault.vmux.ai"))?;
    set_property(
        &public_key,
        "userVerification",
        &JsValue::from_str("required"),
    )?;
    set_property(&public_key, "timeout", &JsValue::from_f64(120_000.0))?;

    let allowed = Array::new();
    let evaluations = Object::new();
    for credential_id in credential_ids {
        let bytes = decode_hex(credential_id)?;
        let descriptor = Object::new();
        set_property(&descriptor, "type", &JsValue::from_str("public-key"))?;
        set_property(
            &descriptor,
            "id",
            Uint8Array::from(bytes.as_slice()).as_ref(),
        )?;
        allowed.push(descriptor.as_ref());
        let evaluation = Object::new();
        set_property(&evaluation, "first", Uint8Array::from(salt).as_ref())?;
        set_property(&evaluations, &base64url(&bytes), evaluation.as_ref())?;
    }
    set_property(&public_key, "allowCredentials", allowed.as_ref())?;
    let prf = Object::new();
    set_property(&prf, "evalByCredential", evaluations.as_ref())?;
    let extensions = Object::new();
    set_property(&extensions, "prf", prf.as_ref())?;
    set_property(&public_key, "extensions", extensions.as_ref())?;

    let options = Object::new();
    set_property(&options, "publicKey", public_key.as_ref())?;
    let credential = call_credentials("get", &options).await?;
    Ok((credential_id(&credential)?, prf_output(&credential)?))
}

fn prf_create_extensions(salt: &[u8]) -> Result<Object, String> {
    let evaluation = Object::new();
    set_property(&evaluation, "first", Uint8Array::from(salt).as_ref())?;
    let prf = Object::new();
    set_property(&prf, "eval", evaluation.as_ref())?;
    let extensions = Object::new();
    set_property(&extensions, "prf", prf.as_ref())?;
    Ok(extensions)
}

async fn call_credentials(method: &str, options: &Object) -> Result<JsValue, String> {
    let window = web_sys::window().ok_or_else(|| "Passkeys are unavailable".to_string())?;
    let navigator =
        Reflect::get(window.as_ref(), &JsValue::from_str("navigator")).map_err(js_error)?;
    let credentials =
        Reflect::get(&navigator, &JsValue::from_str("credentials")).map_err(js_error)?;
    let function = Reflect::get(&credentials, &JsValue::from_str(method))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(js_error)?;
    let promise = function
        .call1(&credentials, options.as_ref())
        .map_err(js_error)?
        .dyn_into::<Promise>()
        .map_err(js_error)?;
    JsFuture::from(promise).await.map_err(js_error)
}

fn credential_id(credential: &JsValue) -> Result<String, String> {
    let raw = Reflect::get(credential, &JsValue::from_str("rawId")).map_err(js_error)?;
    let bytes = Uint8Array::new(&raw).to_vec();
    if bytes.is_empty() {
        return Err("Passkey returned an empty credential".to_string());
    }
    Ok(encode_hex(&bytes))
}

fn prf_output(credential: &JsValue) -> Result<Vec<u8>, String> {
    let function = Reflect::get(credential, &JsValue::from_str("getClientExtensionResults"))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(js_error)?;
    let extensions = function.call0(credential).map_err(js_error)?;
    let prf = Reflect::get(&extensions, &JsValue::from_str("prf")).map_err(js_error)?;
    let results = Reflect::get(&prf, &JsValue::from_str("results")).map_err(js_error)?;
    let first = Reflect::get(&results, &JsValue::from_str("first")).map_err(js_error)?;
    let output = Uint8Array::new(&first).to_vec();
    if output.len() != 32 {
        return Err("Passkey provider does not support encryption".to_string());
    }
    Ok(output)
}

fn random_bytes(length: u32) -> Result<Uint8Array, String> {
    let window = web_sys::window().ok_or_else(|| "Passkeys are unavailable".to_string())?;
    let crypto = Reflect::get(window.as_ref(), &JsValue::from_str("crypto")).map_err(js_error)?;
    let function = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(js_error)?;
    let bytes = Uint8Array::new_with_length(length);
    function.call1(&crypto, bytes.as_ref()).map_err(js_error)?;
    Ok(bytes)
}

fn set_property(target: &Object, name: &str, value: &JsValue) -> Result<(), String> {
    if Reflect::set(target.as_ref(), &JsValue::from_str(name), value).map_err(js_error)? {
        Ok(())
    } else {
        Err(format!("failed to set passkey option {name}"))
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(source: &str) -> Result<Vec<u8>, String> {
    if !source.len().is_multiple_of(2) {
        return Err("invalid passkey credential".to_string());
    }
    source
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("invalid passkey credential".to_string()),
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(value & 63) as usize] as char);
        }
    }
    output
}

fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| "Passkey operation failed".to_string())
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
        credential_id: String::new(),
        prf_output: Vec::new(),
    });
}

fn send_cloud_create(mut pending: Signal<Option<VaultAction>>, root: &str, name: &str) {
    pending.set(Some(VaultAction::CreateCloudFolder));
    let _ = try_cef_bin_emit_rkyv(&VaultActionRequest {
        action: VaultAction::CreateCloudFolder,
        repository: root.to_string(),
        private: true,
        credential_id: name.to_string(),
        prf_output: Vec::new(),
    });
}

fn action_result_message(action: VaultAction) -> String {
    translate(match action {
        VaultAction::Create => "vault-result-created",
        VaultAction::Connect => "vault-result-connected",
        VaultAction::Sync => "vault-result-synced",
        VaultAction::ConnectGithub => "vault-result-github-connected",
        VaultAction::ConnectFolder => "vault-result-folder-connected",
        VaultAction::AddPasskey => "vault-result-created",
        VaultAction::UnlockPasskey => "vault-result-connected",
        VaultAction::ConnectCloud => "vault-result-connected",
        VaultAction::CreateCloudFolder | VaultAction::ChooseCloudFolder => {
            "vault-result-folder-connected"
        }
    })
}

fn suggested_repository_name(
    owner: &str,
    repositories: &[vmux_core::vault::VaultRepository],
) -> String {
    let names = repositories
        .iter()
        .filter_map(|repository| repository.name.strip_prefix(&format!("{owner}/")))
        .collect::<std::collections::BTreeSet<_>>();
    if !names.contains("vmux-vault") {
        return "vmux-vault".to_string();
    }
    (2..)
        .map(|suffix| format!("vmux-vault-{suffix}"))
        .find(|name| !names.contains(name.as_str()))
        .unwrap()
}
