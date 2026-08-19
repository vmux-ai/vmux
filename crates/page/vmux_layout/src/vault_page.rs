#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_core::tools::{TOOLS_SNAPSHOT_EVENT, ToolsSnapshot};
use vmux_core::vault::{
    VAULT_ACTION_RESULT_EVENT, VAULT_AUTH_PROGRESS_EVENT, VaultAction, VaultActionRequest,
    VaultActionResult, VaultAuthProgress, VaultRefreshRequest, VaultSnapshot,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerList, ManagerPage, ManagerSelect,
    ManagerSelectItem, ManagerSelectItemKind, ManagerSpinner,
};
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

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
    let preferred_provider = requested_provider();
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
            if !result.success {
                match result.action {
                    VaultAction::Sync => {
                        result.message = translate("vault-backup-failed");
                    }
                    VaultAction::GenerateRecoveryKey | VaultAction::CreateRecoveryKey => {
                        result.message = translate("vault-recovery-key-create-failed");
                    }
                    VaultAction::UnlockRecoveryKey => {
                        result.message = translate("vault-recovery-key-invalid");
                    }
                    _ => {}
                }
            }
            if result.action == VaultAction::GenerateRecoveryKey && result.success {
                generated_recovery_key.set(result.message);
                recovery_key_confirmation.set(String::new());
                recovery_key_copied.set(false);
                pending.set(None);
                notice.set(None);
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
        request_snapshot(false);
    });

    let current = snapshot();
    rsx! {
        document::Title { {translate("vault-title")} }
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
                                        onclick: move |_| copy_recovery_key(
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
                        onclick: move |_| send_action(
                            pending,
                            VaultAction::GenerateRecoveryKey,
                            String::new(),
                            true,
                        ),
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
                            copy_recovery_key(generated_recovery_key(), recovery_key_copied);
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

fn send_recovery_action(
    mut pending: Signal<Option<VaultAction>>,
    action: VaultAction,
    recovery_key: String,
) {
    pending.set(Some(action));
    if send(&VaultActionRequest {
        action,
        repository: String::new(),
        private: true,
        folder_name: String::new(),
        recovery_key,
    })
    .is_err()
    {
        pending.set(None);
    }
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

/// The provider the Vault was asked to connect, from `vmux://vault/?provider=<name>`.
///
/// A native host puts the view's [`vmux_core::PageMetadata`] in the root scope, so the query
/// rides the url the host opened. On the web the document carries it instead.
fn requested_provider() -> String {
    fn from_query(source: &str) -> Option<String> {
        Some(
            source
                .split_once("?provider=")?
                .1
                .split(['&', '#'])
                .next()?
                .to_string(),
        )
    }

    if let Some(meta) = try_consume_context::<vmux_core::PageMetadata>()
        && let Some(provider) = from_query(&meta.url)
    {
        return provider;
    }
    #[cfg(web)]
    if let Some(provider) = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| from_query(&search))
    {
        return provider;
    }
    String::new()
}

fn copy_recovery_key(value: String, mut copied: Signal<bool>) {
    spawn(async move {
        if vmux_ui::platform::copy_to_clipboard(value).await {
            copied.set(true);
        }
    });
}

fn recovery_keys_match(expected: &str, actual: &str) -> bool {
    recovery_key_complete(actual)
        && normalized_recovery_key(expected) == normalized_recovery_key(actual)
}

fn request_snapshot(load_repositories: bool) {
    let _ = send(&VaultRefreshRequest { load_repositories });
}

fn send_action(
    mut pending: Signal<Option<VaultAction>>,
    action: VaultAction,
    repository: String,
    private: bool,
) {
    pending.set(Some(action));
    let _ = send(&VaultActionRequest {
        action,
        repository,
        private,
        folder_name: String::new(),
        recovery_key: String::new(),
    });
}

fn send_cloud_create(mut pending: Signal<Option<VaultAction>>, root: &str, name: &str) {
    pending.set(Some(VaultAction::CreateCloudFolder));
    let _ = send(&VaultActionRequest {
        action: VaultAction::CreateCloudFolder,
        repository: root.to_string(),
        private: true,
        folder_name: name.to_string(),
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
        VaultAction::GenerateRecoveryKey | VaultAction::CreateRecoveryKey => "vault-result-created",
        VaultAction::UnlockRecoveryKey => "vault-result-connected",
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
