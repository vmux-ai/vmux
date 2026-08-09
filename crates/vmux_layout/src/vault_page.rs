#![allow(non_snake_case)]

use dioxus::prelude::*;
use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use vmux_core::tools::{TOOLS_SNAPSHOT_EVENT, ToolsSnapshot};
use vmux_core::vault::{
    VAULT_ACTION_RESULT_EVENT, VAULT_AUTH_PROGRESS_EVENT, VaultAction, VaultActionRequest,
    VaultActionResult, VaultAuthProgress, VaultRefreshRequest, VaultSnapshot,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerList, ManagerPage, ManagerSelect,
    ManagerSelectItem, ManagerSelectItemKind, ManagerSpinner,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_listener, use_theme};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VaultDestination {
    Create,
    Existing,
}

const PASSKEY_UI_ENABLED: bool = false;

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
    let mut passkey_setup_blocked = use_signal(|| false);
    let mut generated_recovery_key = use_signal(String::new);
    let mut recovery_key_confirmation = use_signal(String::new);
    let mut recovery_key_copied = use_signal(|| false);
    let mut recovery_key_input = use_signal(String::new);
    let mut recovery_upload_pending = use_signal(|| false);
    let mut github_device_code = use_signal(String::new);
    let mut github_device_code_copied = use_signal(|| false);
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

    let _snapshot_listener = use_listener::<ToolsSnapshot, _>(TOOLS_SNAPSHOT_EVENT, move |event| {
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
        use_listener::<VaultActionResult, _>(VAULT_ACTION_RESULT_EVENT, move |mut result| {
            if result.action == VaultAction::ConnectGithub {
                github_device_code.set(String::new());
                github_device_code_copied.set(false);
            }
            if result.action == VaultAction::Sync && result.success {
                recovery_upload_pending.set(false);
            }
            if result.success
                && matches!(
                    result.action,
                    VaultAction::Sync
                        | VaultAction::PreparePasskey
                        | VaultAction::AddPasskey
                        | VaultAction::UnlockPasskey
                        | VaultAction::CreateRecoveryKey
                        | VaultAction::UnlockRecoveryKey
                )
            {
                passkey_setup_blocked.set(false);
            } else if !result.success && is_vault_key_locked(&result.message) {
                passkey_setup_blocked.set(true);
            }
            if !result.success {
                match result.action {
                    VaultAction::Sync => {
                        result.message = translate("vault-backup-failed");
                    }
                    VaultAction::CreateRecoveryKey => {
                        result.message = translate("vault-recovery-key-create-failed");
                    }
                    VaultAction::UnlockRecoveryKey => {
                        result.message = translate("vault-recovery-key-invalid");
                    }
                    _ => {}
                }
            }
            if result.action == VaultAction::PreparePasskey {
                pending.set(None);
                if result.success {
                    passkey_setup_blocked.set(false);
                    start_passkey(VaultAction::AddPasskey, snapshot().vault, pending, notice);
                } else {
                    passkey_setup_blocked.set(true);
                    notice.set(Some(result));
                }
            } else if result.action == VaultAction::CreateRecoveryKey && result.success {
                generated_recovery_key.set(String::new());
                recovery_key_confirmation.set(String::new());
                recovery_key_copied.set(false);
                recovery_upload_pending.set(result.pending_upload);
                pending.set(None);
                notice.set(None);
            } else if result.action == VaultAction::UnlockRecoveryKey && result.success {
                recovery_key_input.set(String::new());
                pending.set(None);
                notice.set(Some(result));
            } else if result.action == VaultAction::ConnectCloud && result.success {
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
    let _auth_progress_listener =
        use_listener::<VaultAuthProgress, _>(VAULT_AUTH_PROGRESS_EVENT, move |progress| {
            github_device_code_copied.set(false);
            github_device_code.set(progress.code);
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
                                "rounded-xl bg-success/10 px-4 py-3 text-xs text-success ring-1 ring-inset ring-success/20"
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
                        github_device_code,
                        github_device_code_copied,
                        cloud_root,
                        private,
                        pending,
                        notice,
                        passkey_setup_blocked,
                        generated_recovery_key,
                        recovery_key_confirmation,
                        recovery_key_copied,
                        recovery_key_input,
                        recovery_upload_pending,
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
    github_device_code: Signal<String>,
    github_device_code_copied: Signal<bool>,
    cloud_root: Signal<String>,
    private: Signal<bool>,
    pending: Signal<Option<VaultAction>>,
    notice: Signal<Option<VaultActionResult>>,
    passkey_setup_blocked: Signal<bool>,
    generated_recovery_key: Signal<String>,
    recovery_key_confirmation: Signal<String>,
    recovery_key_copied: Signal<bool>,
    recovery_key_input: Signal<String>,
    recovery_upload_pending: Signal<bool>,
) -> Element {
    let mut destination = use_signal(|| VaultDestination::Create);
    let is_connected = vault.initialized && !vault.remote.is_empty();
    let pending_changes = vault
        .dirty
        .saturating_add(vault.ahead)
        .saturating_add(vault.behind);
    let status = if vault.sync_failed {
        translate("vault-backup-failed-short")
    } else if pending_changes > 0 {
        translate_with(
            "vault-change-count",
            &[("count", TranslationValue::Number(pending_changes as i64))],
        )
    } else {
        translate("vault-clean")
    };
    let github_connected = !vault.github_owner.is_empty();
    let github_repositories_loaded = vault.repositories_loaded;
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
    let connecting = pending().is_some_and(|action| {
        action == VaultAction::ConnectGithub || action == VaultAction::ConnectCloud
    }) || provider
        .is_some_and(|provider| provider.is_github() && repositories_requested());
    rsx! {
        div { class: "relative overflow-hidden rounded-[28px] bg-foreground/[0.03] p-6 shadow-2xl shadow-black/[0.06] ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl",
            div { class: "pointer-events-none absolute -right-24 -top-28 h-64 w-64 rounded-full bg-cyan-400/[0.08] blur-3xl motion-safe:animate-pulse [animation-duration:7s]" }
            div { class: "pointer-events-none absolute -bottom-36 -left-24 h-72 w-72 rounded-full bg-violet-400/[0.07] blur-3xl motion-safe:animate-pulse [animation-delay:-2.5s] [animation-duration:7s]" }
            div { class: "relative flex items-start gap-4",
                div { class: "grid h-12 w-12 shrink-0 place-items-center rounded-2xl bg-violet-500/10 text-violet-700 shadow-lg shadow-violet-500/10 ring-1 ring-inset ring-violet-500/20 dark:text-violet-300",
                    svg { class: "h-5.5 w-5.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M12 3 4.5 6v5.5c0 4.7 3.2 8.1 7.5 9.5 4.3-1.4 7.5-4.8 7.5-9.5V6Z" }
                        path { d: "m9 12 2 2 4-4" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "text-base font-semibold tracking-tight text-foreground/95", {translate("vault-title")} }
                    if !is_connected || vault.encrypted {
                        div { class: "mt-1 flex items-center gap-1.5 text-xs text-muted-foreground/70",
                            svg { class: "h-3 w-3 shrink-0", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                rect { x: "5", y: "11", width: "14", height: "10", rx: "2" }
                                path { d: "M8 11V7a4 4 0 0 1 8 0v4" }
                            }
                            {translate("vault-encrypted")}
                        }
                    }
                    if is_connected {
                        div { class: "mt-1 truncate text-xs text-muted-foreground/70", "{vault.remote}" }
                        div { class: "mt-1.5 flex gap-2 text-[10px] text-muted-foreground/60",
                            if !vault.branch.is_empty() {
                                span { "{vault.branch}" }
                            }
                            span { class: if vault.sync_failed { "text-ansi-1" } else { "" }, "{status}" }
                            if !vault.sync_failed {
                                span { {translate("vault-auto-sync")} }
                            }
                            if vault.ahead > 0 {
                                span { "↑{vault.ahead}" }
                            }
                            if vault.behind > 0 {
                                span { "↓{vault.behind}" }
                            }
                        }
                    } else {
                        div { class: "mt-1 text-xs text-muted-foreground/70", {translate("vault-description")} }
                        div { class: "mt-1.5 truncate font-mono text-[10px] text-muted-foreground/50", "{vault.root}" }
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
                div { class: "relative mt-6 overflow-hidden rounded-[24px] bg-background/35 p-4 shadow-inner ring-1 ring-inset ring-foreground/[0.08]",
                    div { class: "mx-auto flex w-fit flex-wrap items-center justify-center gap-1.5 rounded-2xl bg-foreground/[0.035] p-1.5 ring-1 ring-inset ring-foreground/[0.07]",
                        for option in RemoteProvider::ALL {
                            button {
                                class: if provider == Some(option) {
                                    "grid h-12 w-12 scale-105 place-items-center rounded-xl bg-background text-foreground shadow-lg shadow-black/10 ring-1 ring-inset ring-cyan-400/40 transition-all duration-300 ease-out"
                                } else {
                                    "grid h-12 w-12 place-items-center rounded-xl text-muted-foreground transition-all duration-300 ease-out hover:-translate-y-0.5 hover:scale-105 hover:bg-foreground/[0.06] hover:text-foreground active:scale-95"
                                },
                                title: option.name(),
                                aria_label: option.name(),
                                onclick: move |_| {
                                    selected_provider.set(Some(option));
                                    github_device_code.set(String::new());
                                    cloud_root.set(String::new());
                                    selected_repository.set(None);
                                    destination.set(VaultDestination::Create);
                                    notice.set(None);
                                    if option.is_github() {
                                        if !github_connected {
                                            repositories_requested.set(false);
                                            send_action(
                                                pending,
                                                VaultAction::ConnectGithub,
                                                String::new(),
                                                true,
                                            );
                                        } else if !github_repositories_loaded
                                            && !repositories_requested()
                                        {
                                            repositories_requested.set(true);
                                            request_snapshot(true);
                                        }
                                    } else {
                                        repository.set("vmux-vault".to_string());
                                        send_action(
                                            pending,
                                            VaultAction::ConnectCloud,
                                            option.name().to_string(),
                                            true,
                                        );
                                    }
                                },
                                ProviderIcon { provider: option }
                            }
                        }
                    }
                    if let Some(provider) = provider {
                        if !authenticated {
                            div {
                                key: "connect-{provider.name()}",
                                class: "flex min-h-52 flex-col items-center justify-center px-5 py-8 text-center transition-[opacity,transform] duration-300 ease-out starting:translate-y-2 starting:scale-[0.985] starting:opacity-0",
                                div { class: "relative grid h-20 w-20 place-items-center",
                                    div { class: "absolute inset-0 rounded-[26px] bg-cyan-400/15 blur-xl motion-safe:animate-pulse [animation-duration:2.4s]" }
                                    div { class: "relative grid h-16 w-16 place-items-center rounded-[22px] bg-background/80 text-foreground shadow-xl shadow-black/10 ring-1 ring-inset ring-foreground/10",
                                        ProviderIcon { provider, large: true }
                                    }
                                }
                                div { class: "mt-4 text-sm font-medium text-foreground/90",
                                    if connecting {
                                        {translate("common-loading")}
                                    } else {
                                        {translate("vault-not-connected")}
                                    }
                                }
                                if provider.is_github() && !github_device_code().is_empty() {
                                    button {
                                        r#type: "button",
                                        class: "group mt-4 rounded-xl bg-foreground/[0.06] px-4 py-2.5 text-foreground shadow-sm ring-1 ring-inset ring-foreground/10 transition-[opacity,transform,background-color] duration-200 ease-out hover:bg-foreground/[0.09] active:scale-[0.98] starting:scale-90 starting:opacity-0",
                                        title: translate("common-copy"),
                                        aria_label: translate("common-copy"),
                                        onclick: move |_| copy_text(
                                            github_device_code(),
                                            github_device_code_copied,
                                        ),
                                        code { class: "font-mono text-base font-semibold tracking-[0.2em]", {github_device_code()} }
                                    }
                                    div {
                                        class: if github_device_code_copied() {
                                            "mt-2 text-[10px] font-medium text-success"
                                        } else {
                                            "mt-2 text-[10px] text-muted-foreground/60"
                                        },
                                        if github_device_code_copied() {
                                            {translate("vault-recovery-key-copied")}
                                        } else {
                                            {translate("vault-recovery-key-copy-hint")}
                                        }
                                    }
                                }
                                if connecting {
                                    div { class: "mt-5 flex items-center gap-1.5",
                                        span { class: "h-1.5 w-1.5 rounded-full bg-cyan-500/70 motion-safe:animate-bounce [animation-duration:1.15s]" }
                                        span { class: "h-1.5 w-1.5 rounded-full bg-cyan-500/70 motion-safe:animate-bounce [animation-delay:120ms] [animation-duration:1.15s]" }
                                        span { class: "h-1.5 w-1.5 rounded-full bg-cyan-500/70 motion-safe:animate-bounce [animation-delay:240ms] [animation-duration:1.15s]" }
                                    }
                                }
                            }
                        } else {
                            div {
                                key: "destination-{provider.name()}",
                                class: "pt-5 transition-[opacity,transform] duration-300 ease-out starting:translate-y-2 starting:scale-[0.985] starting:opacity-0",
                                div { class: "flex items-center justify-center gap-2 text-xs text-success",
                                    span { class: "grid h-5 w-5 place-items-center rounded-full bg-success/15 ring-1 ring-inset ring-success/25",
                                        svg { class: "h-3 w-3", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
                                            path { d: "m5 12 4 4L19 6" }
                                        }
                                    }
                                    if provider.is_github() {
                                        {translate_with(
                                            "vault-connected-as",
                                            &[("name", TranslationValue::String(&vault.github_owner))],
                                        )}
                                    } else {
                                        span { class: "max-w-md truncate", "{cloud_root()}" }
                                    }
                                }
                                if provider == RemoteProvider::Github {
                                    div { class: "mx-auto mt-4 max-w-md",
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
                                }
                                div { class: "mx-auto mt-4 grid max-w-md grid-cols-2 gap-1 rounded-xl bg-foreground/[0.04] p-1 ring-1 ring-inset ring-foreground/[0.07]",
                                    button {
                                        class: if destination() == VaultDestination::Create {
                                            "rounded-lg bg-background px-4 py-2 text-xs font-medium text-foreground shadow-sm ring-1 ring-inset ring-foreground/[0.08] transition-all duration-200"
                                        } else {
                                            "rounded-lg px-4 py-2 text-xs font-medium text-muted-foreground transition-all duration-200 hover:text-foreground"
                                        },
                                        onclick: move |_| destination.set(VaultDestination::Create),
                                        {translate("vault-create")}
                                    }
                                    button {
                                        class: if destination() == VaultDestination::Existing {
                                            "rounded-lg bg-background px-4 py-2 text-xs font-medium text-foreground shadow-sm ring-1 ring-inset ring-foreground/[0.08] transition-all duration-200"
                                        } else {
                                            "rounded-lg px-4 py-2 text-xs font-medium text-muted-foreground transition-all duration-200 hover:text-foreground"
                                        },
                                        onclick: move |_| destination.set(VaultDestination::Existing),
                                        if provider == RemoteProvider::Github {
                                            {translate("vault-choose-repository")}
                                        } else {
                                            {translate("vault-choose-folder")}
                                        }
                                    }
                                }
                                div { class: "mx-auto max-w-md py-4",
                                    if destination() == VaultDestination::Create {
                                        div { class: "rounded-2xl bg-foreground/[0.025] p-3 ring-1 ring-inset ring-foreground/[0.07] transition-[opacity,transform] duration-300 ease-out starting:translate-y-2 starting:scale-[0.985] starting:opacity-0",
                                            div { class: "flex gap-2",
                                                if provider == RemoteProvider::Github {
                                                    div { class: "flex min-w-0 flex-1 items-center rounded-xl bg-background/60 ring-1 ring-inset ring-foreground/10 focus-within:ring-cyan-400/40",
                                                        span { class: "shrink-0 pl-3 text-xs text-muted-foreground/60", "{owner}/" }
                                                        input {
                                                            class: "min-w-0 flex-1 bg-transparent py-2.5 pl-0.5 pr-3 text-sm text-foreground outline-none placeholder:text-muted-foreground/50",
                                                            value: repository(),
                                                            placeholder: translate("vault-repository-name"),
                                                            oninput: move |event| repository.set(event.value()),
                                                        }
                                                    }
                                                } else {
                                                    input {
                                                        class: "min-w-0 flex-1 rounded-xl bg-background/60 px-3 py-2.5 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/40",
                                                        value: repository(),
                                                        placeholder: translate("vault-repository-name"),
                                                        oninput: move |event| repository.set(event.value()),
                                                    }
                                                }
                                                ManagerButton {
                                                    variant: ManagerButtonVariant::Primary,
                                                    disabled: pending().is_some() || repository().trim().is_empty() || (provider == RemoteProvider::Github && owner.is_empty()),
                                                    onclick: move |_| {
                                                        if provider == RemoteProvider::Github {
                                                            send_action(
                                                                pending,
                                                                VaultAction::Create,
                                                                format!("{owner}/{}", repository().trim()),
                                                                private(),
                                                            );
                                                        } else {
                                                            send_cloud_create(
                                                                pending,
                                                                &cloud_root(),
                                                                repository().trim(),
                                                            );
                                                        }
                                                    },
                                                    {translate("vault-create")}
                                                }
                                            }
                                            if provider == RemoteProvider::Github {
                                                label { class: "mt-3 flex cursor-pointer items-center gap-2 px-1 text-xs text-muted-foreground",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked: private(),
                                                        onchange: move |event| private.set(event.checked()),
                                                    }
                                                    {translate("vault-private")}
                                                }
                                                if !private() {
                                                    div { class: "mt-2 px-1 text-[10px] text-amber-600 dark:text-amber-300", {translate("vault-public-warning")} }
                                                }
                                            }
                                        }
                                    } else {
                                        div { class: "rounded-2xl bg-foreground/[0.025] p-3 ring-1 ring-inset ring-foreground/[0.07] transition-[opacity,transform] duration-300 ease-out starting:translate-y-2 starting:scale-[0.985] starting:opacity-0",
                                            if provider == RemoteProvider::Github {
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
                                                        variant: ManagerButtonVariant::Primary,
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
                                            } else {
                                                button {
                                                    class: "flex w-full items-center justify-center gap-2 rounded-xl bg-background/60 px-4 py-3 text-xs font-medium text-foreground shadow-sm ring-1 ring-inset ring-foreground/10 transition-all duration-200 hover:-translate-y-0.5 hover:bg-foreground/[0.07] active:translate-y-0",
                                                    disabled: pending().is_some(),
                                                    onclick: move |_| send_action(
                                                        pending,
                                                        VaultAction::ChooseCloudFolder,
                                                        cloud_root(),
                                                        true,
                                                    ),
                                                    svg { class: "h-4 w-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                        path { d: "M3 7h5l2 2h11v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" }
                                                        path { d: "M3 7V5a2 2 0 0 1 2-2h3l2 2h4" }
                                                    }
                                                    {translate("vault-choose-folder")}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "flex min-h-52 flex-col items-center justify-center px-5 py-10 text-center transition-[opacity,transform] duration-300 ease-out starting:translate-y-2 starting:scale-[0.985] starting:opacity-0",
                            div { class: "text-sm font-medium text-foreground/85", {translate("vault-connect")} }
                            div { class: "mt-1 max-w-sm text-xs leading-relaxed text-muted-foreground/60", {translate("vault-description")} }
                        }
                    }
                }
                if !vault.error.is_empty() {
                    div { class: "relative mt-3 text-center text-[10px] text-amber-600 dark:text-amber-300", "{vault.error}" }
                }
            } else {
                RecoveryCard {
                    vault: vault.clone(),
                    pending,
                    generated_recovery_key,
                    recovery_key_confirmation,
                    recovery_key_copied,
                    recovery_key_input,
                    recovery_upload_pending,
                    notice,
                }
                if PASSKEY_UI_ENABLED
                    && (!passkey_setup_blocked() || !vault.passkey_credentials.is_empty())
                {
                    PasskeyCard {
                        vault,
                        pending,
                        notice,
                        passkey_setup_blocked,
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderIcon(provider: RemoteProvider, #[props(default)] large: bool) -> Element {
    let class = if large {
        "h-8 w-8 shrink-0"
    } else {
        "h-5 w-5 shrink-0"
    };
    match provider {
        RemoteProvider::Github => rsx! {
            svg { class, view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M12 .7a11.3 11.3 0 0 0-3.57 22.02c.57.1.78-.25.78-.55v-2.16c-3.18.69-3.85-1.35-3.85-1.35-.52-1.32-1.27-1.67-1.27-1.67-1.04-.71.08-.7.08-.7 1.15.08 1.75 1.18 1.75 1.18 1.02 1.75 2.68 1.24 3.33.95.1-.74.4-1.24.73-1.53-2.54-.29-5.21-1.27-5.21-5.65 0-1.25.45-2.27 1.18-3.07-.12-.29-.51-1.45.11-3.03 0 0 .96-.31 3.11 1.17A10.8 10.8 0 0 1 12 5.93c.96 0 1.92.13 2.82.38 2.15-1.48 3.11-1.17 3.11-1.17.62 1.58.23 2.74.11 3.03.73.8 1.18 1.82 1.18 3.07 0 4.39-2.68 5.35-5.23 5.64.41.36.78 1.06.78 2.14v3.15c0 .3.21.66.79.55A11.3 11.3 0 0 0 12 .7Z" }
            }
        },
        RemoteProvider::GoogleDrive => rsx! {
            svg { class, view_box: "0 0 24 24", fill: "none",
                path { d: "M8.1 3h7.8l4 7h-7.8Z", fill: "#fbbc04" }
                path { d: "m8.1 3 4 7-4.1 7H4Z", fill: "#34a853" }
                path { d: "M8 17h8l3.9-7h-7.8Z", fill: "#4285f4" }
            }
        },
        RemoteProvider::Dropbox => rsx! {
            svg { class: "{class} text-[#0061ff]", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "m6.5 3.5 5.5 3.4-5.5 3.5L1 6.9Zm11 0L23 6.9l-5.5 3.5L12 6.9Zm-11 8L12 15l-5.5 3.4L1 15Zm11 0L23 15l-5.5 3.4L12 15ZM6.6 19.6l5.4-3.4 5.4 3.4L12 23Z" }
            }
        },
        RemoteProvider::OneDrive => rsx! {
            svg { class: "{class} text-[#0078d4]", view_box: "0 0 24 24", fill: "currentColor",
                path { d: "M9.3 7.3A6 6 0 0 1 19.8 11a4.5 4.5 0 0 1-.3 9H6a5 5 0 0 1-.6-10A5.8 5.8 0 0 1 9.3 7.3Z" }
            }
        },
    }
}

#[component]
fn RecoveryCard(
    vault: VaultSnapshot,
    pending: Signal<Option<VaultAction>>,
    mut generated_recovery_key: Signal<String>,
    mut recovery_key_confirmation: Signal<String>,
    mut recovery_key_copied: Signal<bool>,
    mut recovery_key_input: Signal<String>,
    recovery_upload_pending: Signal<bool>,
    mut notice: Signal<Option<VaultActionResult>>,
) -> Element {
    let generated = generated_recovery_key();
    let confirmation = recovery_key_confirmation();
    let confirmation_complete = recovery_key_complete(&confirmation);
    let confirmation_matches = recovery_keys_match(&generated, &confirmation);
    rsx! {
        div { class: "mt-4 rounded-xl bg-background/35 p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "flex items-start gap-3",
                svg { class: "mt-0.5 h-5 w-5 shrink-0 text-foreground/70", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M21 2 13.6 9.4" }
                    circle { cx: "8", cy: "15", r: "5" }
                    path { d: "m18 5 1 1" }
                    path { d: "m15 8 1 1" }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "text-sm font-medium text-foreground", {translate("vault-recovery-key")} }
                    div { class: "mt-0.5 text-xs leading-relaxed text-muted-foreground/70", {translate("vault-recovery-key-description")} }
                }
                if vault.unlocked && !vault.recovery_enabled && generated.is_empty() {
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: pending().is_some() || !generated.is_empty(),
                        onclick: move |_| match generate_recovery_key() {
                            Ok(key) => {
                                generated_recovery_key.set(key);
                                recovery_key_confirmation.set(String::new());
                                recovery_key_copied.set(false);
                                notice.set(None);
                            }
                            Err(_) => notice.set(Some(VaultActionResult {
                                action: VaultAction::CreateRecoveryKey,
                                success: false,
                                message: translate("vault-recovery-key-create-failed"),
                                pending_upload: false,
                            })),
                        },
                        {translate("vault-recovery-key-create")}
                    }
                }
            }
            if !generated.is_empty() {
                div { class: "mt-4 space-y-3 transition-[opacity,transform] duration-300 ease-out starting:translate-y-1 starting:opacity-0",
                    button {
                        r#type: "button",
                        title: translate("vault-recovery-key-copy-hint"),
                        class: if recovery_key_copied() {
                            "flex w-full cursor-pointer items-center gap-3 rounded-xl bg-success/[0.08] px-3 py-3 text-left ring-1 ring-inset ring-success/25 transition-colors hover:bg-success/[0.12]"
                        } else {
                            "flex w-full cursor-pointer items-center gap-3 rounded-xl bg-foreground/[0.04] px-3 py-3 text-left ring-1 ring-inset ring-foreground/10 transition-colors hover:bg-foreground/[0.07]"
                        },
                        onclick: move |_| {
                            recovery_key_copied.set(false);
                            copy_text(generated_recovery_key(), recovery_key_copied);
                        },
                        code { class: "min-w-0 flex-1 break-all font-mono text-[11px] leading-relaxed text-foreground",
                            if recovery_key_copied() {
                                "vmux-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••-••••"
                            } else {
                                "{generated}"
                            }
                        }
                        span { class: if recovery_key_copied() {
                                "shrink-0 text-[11px] font-medium text-success"
                            } else {
                                "shrink-0 text-[11px] text-muted-foreground/60"
                            },
                            if recovery_key_copied() {
                                {translate("vault-recovery-key-copied")}
                            } else {
                                {translate("vault-recovery-key-copy-hint")}
                            }
                        }
                    }
                    if recovery_key_copied() {
                        div { class: "space-y-2 transition-[opacity,transform] duration-300 ease-out starting:translate-y-1 starting:opacity-0",
                            div { class: "text-xs leading-relaxed text-muted-foreground/70", {translate("vault-recovery-key-verify")} }
                            input {
                                autofocus: true,
                                class: if confirmation_complete && !confirmation_matches {
                                    "w-full rounded-xl bg-background/60 px-3 py-2.5 font-mono text-xs text-foreground outline-none ring-1 ring-inset ring-ansi-1/45 transition focus:ring-ansi-1/65"
                                } else {
                                    "w-full rounded-xl bg-background/60 px-3 py-2.5 font-mono text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 transition focus:ring-cyan-400/50"
                                },
                                r#type: "password",
                                value: "{confirmation}",
                                placeholder: translate("vault-recovery-key-verify-placeholder"),
                                disabled: pending().is_some(),
                                oninput: move |event| {
                                    let value = event.value();
                                    recovery_key_confirmation.set(value.clone());
                                    if pending().is_none()
                                        && recovery_keys_match(&generated_recovery_key(), &value)
                                    {
                                        send_recovery_action(
                                            pending,
                                            VaultAction::CreateRecoveryKey,
                                            generated_recovery_key(),
                                        );
                                    }
                                },
                            }
                            if pending() == Some(VaultAction::CreateRecoveryKey) {
                                div { class: "text-[11px] text-cyan-700 dark:text-cyan-300", {translate("common-loading")} }
                            } else if confirmation_complete && !confirmation_matches {
                                div { class: "text-[11px] text-ansi-1", {translate("vault-recovery-key-mismatch")} }
                            }
                        }
                    }
                    if recovery_upload_pending() {
                        div { class: "text-[11px] leading-relaxed text-amber-700 dark:text-amber-300", {translate("vault-recovery-key-upload-pending")} }
                    }
                }
            } else if !vault.unlocked && vault.recovery_enabled {
                div { class: "mt-4 space-y-2",
                    input {
                        class: "w-full rounded-xl bg-background/60 px-3 py-2.5 font-mono text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 transition focus:ring-cyan-400/50",
                        r#type: "password",
                        value: "{recovery_key_input}",
                        placeholder: translate("vault-recovery-key-placeholder"),
                        disabled: pending().is_some(),
                        oninput: move |event| {
                            let value = event.value();
                            recovery_key_input.set(value.clone());
                            if pending().is_none() && recovery_key_complete(&value) {
                                send_recovery_action(
                                    pending,
                                    VaultAction::UnlockRecoveryKey,
                                    value,
                                );
                            }
                        },
                    }
                    if pending() == Some(VaultAction::UnlockRecoveryKey) {
                        div { class: "text-[11px] text-cyan-700 dark:text-cyan-300", {translate("common-loading")} }
                    }
                }
            } else if vault.recovery_enabled {
                div { class: "mt-3 text-xs font-medium text-success", {translate("vault-recovery-key-ready")} }
                if recovery_upload_pending() {
                    div { class: "mt-2 text-[11px] leading-relaxed text-amber-700 dark:text-amber-300", {translate("vault-recovery-key-upload-pending")} }
                }
            }
        }
    }
}

#[component]
fn PasskeyCard(
    vault: VaultSnapshot,
    pending: Signal<Option<VaultAction>>,
    notice: Signal<Option<VaultActionResult>>,
    passkey_setup_blocked: Signal<bool>,
) -> Element {
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
                if !vault.unlocked && !vault.passkey_credentials.is_empty() {
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
                if vault.unlocked && !passkey_setup_blocked() {
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: pending().is_some(),
                        onclick: move |_| send_action(
                            pending,
                            VaultAction::PreparePasskey,
                            String::new(),
                            true,
                        ),
                        {translate("vault-passkey-add")}
                    }
                }
            }
        }
    }
}

fn send_recovery_action(
    mut pending: Signal<Option<VaultAction>>,
    action: VaultAction,
    recovery_key: String,
) {
    pending.set(Some(action));
    if try_cef_bin_emit_rkyv(&VaultActionRequest {
        action,
        repository: String::new(),
        private: true,
        credential_id: String::new(),
        prf_output: Vec::new(),
        recovery_key,
    })
    .is_err()
    {
        pending.set(None);
    }
}

fn generate_recovery_key() -> Result<String, String> {
    let encoded = encode_hex(&random_bytes(32)?.to_vec());
    let groups = encoded
        .as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap())
        .collect::<Vec<_>>();
    Ok(format!("vmux-{}", groups.join("-")))
}

fn normalized_recovery_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != '-')
        .collect()
}

fn recovery_key_complete(value: &str) -> bool {
    normalized_recovery_key(value).len() == 68
}

fn recovery_keys_match(expected: &str, actual: &str) -> bool {
    recovery_key_complete(actual)
        && normalized_recovery_key(expected) == normalized_recovery_key(actual)
}

fn copy_text(value: String, mut copied: Signal<bool>) {
    spawn(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(navigator) = Reflect::get(window.as_ref(), &JsValue::from_str("navigator")) else {
            return;
        };
        let Ok(clipboard) = Reflect::get(&navigator, &JsValue::from_str("clipboard")) else {
            return;
        };
        let Ok(function) = Reflect::get(&clipboard, &JsValue::from_str("writeText"))
            .and_then(|function| function.dyn_into::<Function>())
        else {
            return;
        };
        let Ok(promise) = function
            .call1(&clipboard, &JsValue::from_str(&value))
            .and_then(|promise| promise.dyn_into::<Promise>())
        else {
            return;
        };
        if JsFuture::from(promise).await.is_ok() {
            copied.set(true);
        }
    });
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
            pending_upload: false,
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
                    recovery_key: String::new(),
                }) {
                    pending.set(None);
                    notice.set(Some(VaultActionResult {
                        action,
                        success: false,
                        message: error.to_string(),
                        pending_upload: false,
                    }));
                }
            }
            Err(message) => {
                pending.set(None);
                notice.set(Some(VaultActionResult {
                    action,
                    success: false,
                    message,
                    pending_upload: false,
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
    let extensions = client_extension_results(&credential)?;
    match prf_output(&extensions) {
        Ok(output) => Ok((credential_id, output)),
        Err(_) if prf_enabled(&extensions) => get_passkey(&[credential_id], salt).await,
        Err(message) => Err(message),
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
    let extensions = client_extension_results(&credential)?;
    Ok((credential_id(&credential)?, prf_output(&extensions)?))
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

fn client_extension_results(credential: &JsValue) -> Result<JsValue, String> {
    let function = Reflect::get(credential, &JsValue::from_str("getClientExtensionResults"))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(js_error)?;
    let extensions = function.call0(credential).map_err(js_error)?;
    if !extensions.is_object() {
        return Err(translate("vault-passkey-provider-unsupported"));
    }
    Ok(extensions)
}

fn prf_enabled(extensions: &JsValue) -> bool {
    let Ok(prf) = Reflect::get(extensions, &JsValue::from_str("prf")) else {
        return false;
    };
    if !prf.is_object() {
        return false;
    }
    Reflect::get(&prf, &JsValue::from_str("enabled"))
        .ok()
        .and_then(|enabled| enabled.as_bool())
        .unwrap_or(false)
}

fn prf_output(extensions: &JsValue) -> Result<Vec<u8>, String> {
    let prf = Reflect::get(extensions, &JsValue::from_str("prf")).map_err(js_error)?;
    if !prf.is_object() {
        return Err(translate("vault-passkey-provider-unsupported"));
    }
    let results = Reflect::get(&prf, &JsValue::from_str("results")).map_err(js_error)?;
    if !results.is_object() {
        return Err(translate("vault-passkey-provider-unsupported"));
    }
    let first = Reflect::get(&results, &JsValue::from_str("first")).map_err(js_error)?;
    if first.is_null() || first.is_undefined() {
        return Err(translate("vault-passkey-provider-unsupported"));
    }
    let output = Uint8Array::new(&first).to_vec();
    if output.len() != 32 {
        return Err(translate("vault-passkey-provider-unsupported"));
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
            error.is_object().then(|| {
                Reflect::get(&error, &JsValue::from_str("message"))
                    .ok()
                    .and_then(|message| message.as_string())
            })?
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
        recovery_key: String::new(),
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
        recovery_key: String::new(),
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
        VaultAction::PreparePasskey => "vault-result-connected",
        VaultAction::UnlockPasskey => "vault-result-connected",
        VaultAction::CreateRecoveryKey => "vault-result-created",
        VaultAction::UnlockRecoveryKey => "vault-result-connected",
        VaultAction::ConnectCloud => "vault-result-connected",
        VaultAction::CreateCloudFolder | VaultAction::ChooseCloudFolder => {
            "vault-result-folder-connected"
        }
    })
}

fn is_vault_key_locked(message: &str) -> bool {
    message.starts_with("This Vault is locked on this device.")
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
