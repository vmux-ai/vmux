#![allow(non_snake_case)]

use std::collections::BTreeSet;

use dioxus::prelude::*;
use vmux_core::tools::{
    TOOL_ACTION_RESULT_EVENT, TOOLS_SNAPSHOT_EVENT, ToolAction, ToolActionRequest,
    ToolActionResult, ToolItem, ToolOpenRequest, ToolProvider, ToolStatus, ToolsRefreshRequest,
    ToolsSnapshot,
};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSpinner, ManagerTone,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut snapshot = use_signal(ToolsSnapshot::default);
    let mut loaded = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut pending = use_signal(BTreeSet::<String>::new);
    let mut notice = use_signal(|| None::<ToolActionResult>);
    let add_provider = use_signal(|| ToolProvider::HomebrewFormula.id().to_string());
    let add_name = use_signal(String::new);
    let adopt_package = use_signal(String::new);
    let adopt_path = use_signal(String::new);
    let import_provider = use_signal(|| ToolProvider::Npm.id().to_string());
    let import_path = use_signal(String::new);
    let brewfile_import_path = use_signal(String::new);

    let _snapshot_listener =
        use_bin_event_listener::<ToolsSnapshot, _>(TOOLS_SNAPSHOT_EVENT, move |event| {
            snapshot.set(event);
            loaded.set(true);
        });
    let _action_listener =
        use_bin_event_listener::<ToolActionResult, _>(TOOL_ACTION_RESULT_EVENT, move |result| {
            pending
                .write()
                .remove(&action_key(result.provider, result.action, &result.id));
            notice.set(Some(result));
            request_snapshot(false);
        });

    use_effect(move || {
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title("Tools");
        }
        request_snapshot(false);
    });

    let current = snapshot();
    let search = query().trim().to_ascii_lowercase();
    let visible_count = current
        .categories
        .iter()
        .flat_map(|category| &category.items)
        .filter(|item| item_matches(item, &search))
        .count();
    rsx! {
        ManagerPage {
            ManagerHeader {
                title: "Tools",
                count: visible_count,
                search_value: query(),
                search_placeholder: "Search packages, agents, MCP, language tools, dotfiles…",
                onsearch: move |event: FormEvent| query.set(event.value()),
                onkeydown: None,
                actions: rsx! {
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: pending().contains(&action_key(
                            ToolProvider::Dotfiles,
                            ToolAction::Apply,
                            "",
                        )),
                        onclick: move |_| {
                            send_action(
                                pending,
                                ToolProvider::Dotfiles,
                                ToolAction::Apply,
                                String::new(),
                                String::new(),
                            );
                        },
                        "Apply"
                    }
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        onclick: move |_| {
                            loaded.set(false);
                            request_snapshot(true);
                        },
                        "Refresh"
                    }
                },
            }
            ManagerList {
                HomebrewSourceCard {
                    root: current.root.clone(),
                    import_path: brewfile_import_path,
                    pending,
                }
                ToolsControls {
                    add_provider,
                    add_name,
                    adopt_package,
                    adopt_path,
                    import_provider,
                    import_path,
                    pending,
                }
                if let Some(result) = notice() {
                    div {
                        class: if result.success {
                            "rounded-xl bg-emerald-400/10 px-4 py-3 text-xs text-emerald-700 ring-1 ring-inset ring-emerald-400/20 dark:text-emerald-300"
                        } else {
                            "rounded-xl bg-ansi-1/10 px-4 py-3 text-xs text-ansi-1 ring-1 ring-inset ring-ansi-1/20"
                        },
                        "{result.message}"
                    }
                }
                if !current.error.is_empty() {
                    div { class: "whitespace-pre-wrap rounded-xl bg-amber-400/10 px-4 py-3 text-xs text-amber-700 ring-1 ring-inset ring-amber-400/20 dark:text-amber-300",
                        "{current.error}"
                    }
                }
                if !loaded() {
                    ManagerSpinner { detail: "Scanning local tools…" }
                } else if visible_count == 0 {
                    ManagerEmpty {
                        title: "No matching tools",
                        detail: "Install a package or add a Stow-style dotfile package.",
                    }
                } else {
                    for category in current.categories.iter() {
                        if category.items.iter().any(|item| item_matches(item, &search)) {
                            div { class: "mt-3 flex items-center gap-2 px-1 first:mt-0",
                                h2 { class: "text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground", "{category.provider.title()}" }
                                span { class: "text-[10px] text-muted-foreground/60",
                                    "{category.items.iter().filter(|item| item_matches(item, &search)).count()}"
                                }
                            }
                            for item in category.items.iter().filter(|item| item_matches(item, &search)) {
                                ToolRow { key: "{category.provider.id()}:{item.id}", item: item.clone(), pending }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomebrewSourceCard(
    root: String,
    mut import_path: Signal<String>,
    pending: Signal<BTreeSet<String>>,
) -> Element {
    let brewfile = format!("{root}/Brewfile");
    let open_brewfile = brewfile.clone();
    let import_key = action_key(ToolProvider::HomebrewFormula, ToolAction::Import, "");
    rsx! {
        div { class: "flex flex-col gap-3 rounded-2xl bg-foreground/[0.035] p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "flex items-center gap-3",
                div { class: "grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-foreground/[0.06] ring-1 ring-inset ring-foreground/10",
                    svg { class: "h-5 w-5 text-foreground/80", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "m15 12-8.5 8.5a2.12 2.12 0 1 1-3-3L12 9" }
                        path { d: "M17.64 15 22 10.64" }
                        path { d: "m20.91 11.7-1.25-1.25a1 1 0 0 1 0-1.41l.35-.35a1 1 0 0 0 0-1.41l-3.33-3.33a1 1 0 0 0-1.41 0l-.35.35a1 1 0 0 1-1.41 0l-1.25-1.25a3.09 3.09 0 0 0-4.37 0L6.8 4.14a1 1 0 0 0 0 1.41l11.65 11.65a1 1 0 0 0 1.41 0l1.09-1.09a3.09 3.09 0 0 0-.04-4.41Z" }
                    }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "font-medium text-foreground/95", "Brewfile" }
                    div { class: "truncate text-xs text-muted-foreground/70", "{brewfile}" }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    disabled: root.is_empty(),
                    onclick: move |_| open_tool_file(open_brewfile.clone()),
                    "Open"
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Primary,
                    disabled: pending().contains(&import_key),
                    onclick: move |_| send_action(
                        pending,
                        ToolProvider::HomebrewFormula,
                        ToolAction::Import,
                        String::new(),
                        String::new(),
                    ),
                    "Import installed"
                }
            }
            div { class: "flex min-w-0 gap-2 border-t border-foreground/[0.07] pt-3",
                input {
                    r#type: "text",
                    class: "min-w-0 flex-1 rounded-lg bg-foreground/[0.05] px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/30",
                    placeholder: "Import another Brewfile…",
                    value: "{import_path}",
                    oninput: move |event| import_path.set(event.value()),
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    disabled: import_path().trim().is_empty() || pending().contains(&import_key),
                    onclick: move |_| {
                        let path = import_path().trim().to_string();
                        if path.is_empty() {
                            return;
                        }
                        send_action(
                            pending,
                            ToolProvider::HomebrewFormula,
                            ToolAction::Import,
                            String::new(),
                            path,
                        );
                        import_path.set(String::new());
                    },
                    "Import Brewfile"
                }
            }
            div { class: "text-[10px] text-muted-foreground/60",
                "Edit the Brewfile directly or use the package browser below."
            }
        }
    }
}

#[component]
fn ToolsControls(
    mut add_provider: Signal<String>,
    mut add_name: Signal<String>,
    mut adopt_package: Signal<String>,
    mut adopt_path: Signal<String>,
    mut import_provider: Signal<String>,
    mut import_path: Signal<String>,
    pending: Signal<BTreeSet<String>>,
) -> Element {
    let selected_import_provider =
        provider_from_id(&import_provider()).unwrap_or(ToolProvider::Npm);
    let import_copy = import_copy(selected_import_provider, import_path().trim().is_empty());
    let import_requires_path = selected_import_provider == ToolProvider::Dotfiles;
    let import_supports_path = matches!(
        selected_import_provider,
        ToolProvider::Npm | ToolProvider::Mcp | ToolProvider::Dotfiles
    );
    rsx! {
        div { class: "grid gap-3 rounded-2xl bg-foreground/[0.035] p-4 ring-1 ring-inset ring-foreground/10 md:grid-cols-2",
            div { class: "flex min-w-0 flex-col gap-2",
                div { class: "text-xs font-medium text-foreground/90", "Add package" }
                div { class: "flex min-w-0 gap-2",
                    select {
                        class: "min-w-0 rounded-lg bg-foreground/[0.05] px-2.5 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10",
                        value: "{add_provider}",
                        onchange: move |event| add_provider.set(event.value()),
                        option { value: "homebrew-formula", "Homebrew formula" }
                        option { value: "homebrew-cask", "Homebrew cask" }
                        option { value: "npm", "npm" }
                        option { value: "acp", "ACP agent" }
                        option { value: "lsp", "Language tool" }
                    }
                    input {
                        r#type: "text",
                        class: "min-w-0 flex-1 rounded-lg bg-foreground/[0.05] px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/30",
                        placeholder: "Package name",
                        value: "{add_name}",
                        oninput: move |event| add_name.set(event.value()),
                    }
                    ManagerButton {
                        disabled: add_name().trim().is_empty(),
                        onclick: move |_| {
                            let Some(provider) = provider_from_id(&add_provider()) else {
                                return;
                            };
                            let name = add_name().trim().to_string();
                            if name.is_empty() {
                                return;
                            }
                            send_action(
                                pending,
                                provider,
                                ToolAction::Install,
                                name,
                                String::new(),
                            );
                            add_name.set(String::new());
                        },
                        "Install"
                    }
                }
            }
            div { class: "flex min-w-0 flex-col gap-2",
                div { class: "text-xs font-medium text-foreground/90", "Adopt dotfile" }
                div { class: "flex min-w-0 gap-2",
                    input {
                        r#type: "text",
                        class: "w-24 min-w-0 rounded-lg bg-foreground/[0.05] px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/30",
                        placeholder: "Package",
                        value: "{adopt_package}",
                        oninput: move |event| adopt_package.set(event.value()),
                    }
                    input {
                        r#type: "text",
                        class: "min-w-0 flex-1 rounded-lg bg-foreground/[0.05] px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/30",
                        placeholder: "~/.config/tool/config",
                        value: "{adopt_path}",
                        oninput: move |event| adopt_path.set(event.value()),
                    }
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: adopt_package().trim().is_empty() || adopt_path().trim().is_empty(),
                        onclick: move |_| {
                            let package = adopt_package().trim().to_string();
                            let path = adopt_path().trim().to_string();
                            if package.is_empty() || path.is_empty() {
                                return;
                            }
                            send_action(
                                pending,
                                ToolProvider::Dotfiles,
                                ToolAction::Adopt,
                                package,
                                path,
                            );
                            adopt_path.set(String::new());
                        },
                        "Adopt"
                    }
                }
            }
            div { class: "flex min-w-0 flex-col gap-2 md:col-span-2",
                div { class: "flex items-baseline gap-2",
                    div { class: "text-xs font-medium text-foreground/90", "Import existing" }
                    div { class: "text-[10px] text-muted-foreground/60", "{import_copy.detail}" }
                }
                div { class: "flex min-w-0 gap-2",
                    select {
                        class: "min-w-0 rounded-lg bg-foreground/[0.05] px-2.5 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10",
                        value: "{import_provider}",
                        onchange: move |event| {
                            import_provider.set(event.value());
                            import_path.set(String::new());
                        },
                        option { value: "npm", "npm" }
                        option { value: "acp", "Installed ACP agents" }
                        option { value: "lsp", "Installed language tools" }
                        option { value: "mcp", "MCP servers" }
                        option { value: "dotfiles", "Stow dotfiles" }
                    }
                    if import_supports_path {
                        input {
                            r#type: "text",
                            class: "min-w-0 flex-1 rounded-lg bg-foreground/[0.05] px-3 py-2 text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/10 placeholder:text-muted-foreground/50 focus:ring-cyan-400/30",
                            placeholder: "{import_copy.placeholder}",
                            value: "{import_path}",
                            oninput: move |event| import_path.set(event.value()),
                        }
                    } else {
                        div { class: "min-w-0 flex-1" }
                    }
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        disabled: (import_requires_path && import_path().trim().is_empty())
                            || pending().contains(&action_key(
                                selected_import_provider,
                                ToolAction::Import,
                                "",
                            )),
                        onclick: move |_| {
                            let Some(provider) = provider_from_id(&import_provider()) else {
                                return;
                            };
                            send_action(
                                pending,
                                provider,
                                ToolAction::Import,
                                String::new(),
                                import_path().trim().to_string(),
                            );
                        },
                        "{import_copy.action}"
                    }
                }
            }
        }
    }
}

struct ImportCopy {
    detail: &'static str,
    placeholder: &'static str,
    action: &'static str,
}

fn import_copy(provider: ToolProvider, path_empty: bool) -> ImportCopy {
    match provider {
        ToolProvider::Npm if path_empty => ImportCopy {
            detail: "Adopt every globally installed package.",
            placeholder: "package.json path (optional)",
            action: "Import installed",
        },
        ToolProvider::Npm => ImportCopy {
            detail: "Import dependencies from package.json.",
            placeholder: "package.json path (optional)",
            action: "Import package.json",
        },
        ToolProvider::Acp => ImportCopy {
            detail: "Adopt every installed ACP agent.",
            placeholder: "",
            action: "Import installed",
        },
        ToolProvider::Lsp => ImportCopy {
            detail: "Adopt every installed language tool.",
            placeholder: "",
            action: "Import installed",
        },
        ToolProvider::Mcp if path_empty => ImportCopy {
            detail: "Import servers from Claude, Codex, and Vibe configs.",
            placeholder: "Config path (optional)",
            action: "Import configs",
        },
        ToolProvider::Mcp => ImportCopy {
            detail: "Import servers from one MCP config.",
            placeholder: "Config path (optional)",
            action: "Import config",
        },
        ToolProvider::Dotfiles => ImportCopy {
            detail: "Copy packages from an existing Stow directory.",
            placeholder: "Stow root path",
            action: "Import dotfiles",
        },
        ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask => ImportCopy {
            detail: "Managed through Brewfile above.",
            placeholder: "",
            action: "Import",
        },
    }
}

#[component]
fn ToolRow(item: ToolItem, pending: Signal<BTreeSet<String>>) -> Element {
    let version = item.version.clone().unwrap_or_default();
    let subtitle = if version.is_empty() {
        item.detail.clone()
    } else if item.detail.is_empty() {
        version
    } else {
        format!("{version} · {}", item.detail)
    };
    let provider = item.provider;
    let id = item.id.clone();
    rsx! {
        ManagerRow {
            show_icon: false,
            icon: rsx! {},
            title: item.name.clone(),
            subtitle,
            meta: rsx! {
                ManagerBadge { tone: ManagerTone::Neutral, "{provider_short_label(provider)}" }
                if item.managed {
                    ManagerBadge { tone: ManagerTone::Cyan, "managed" }
                }
                ManagerBadge { tone: status_tone(item.status), "{status_label(item.status)}" }
            },
            actions: rsx! {
                for action in item.actions.iter().copied() {
                    {
                        let action_id = id.clone();
                        let key = action_key(provider, action, &action_id);
                        rsx! {
                            ManagerButton {
                                key: "{key}",
                                variant: action_variant(action),
                                disabled: pending().contains(&key),
                                onclick: move |_| {
                                    send_action(
                                        pending,
                                        provider,
                                        action,
                                        action_id.clone(),
                                        String::new(),
                                    );
                                },
                                "{action_label(action)}"
                            }
                        }
                    }
                }
            },
        }
    }
}

fn request_snapshot(refresh: bool) {
    let _ = try_cef_bin_emit_rkyv(&ToolsRefreshRequest { refresh });
}

fn send_action(
    mut pending: Signal<BTreeSet<String>>,
    provider: ToolProvider,
    action: ToolAction,
    id: String,
    value: String,
) {
    pending.write().insert(action_key(provider, action, &id));
    let _ = try_cef_bin_emit_rkyv(&ToolActionRequest {
        provider,
        action,
        id,
        value,
    });
}

fn item_matches(item: &ToolItem, query: &str) -> bool {
    query.is_empty()
        || item.name.to_ascii_lowercase().contains(query)
        || item.id.to_ascii_lowercase().contains(query)
        || item.detail.to_ascii_lowercase().contains(query)
        || item.provider.title().to_ascii_lowercase().contains(query)
}

fn provider_from_id(id: &str) -> Option<ToolProvider> {
    ToolProvider::ALL
        .into_iter()
        .find(|provider| provider.id() == id)
}

fn provider_short_label(provider: ToolProvider) -> &'static str {
    match provider {
        ToolProvider::HomebrewFormula | ToolProvider::HomebrewCask => "brew",
        ToolProvider::Npm => "npm",
        ToolProvider::Acp => "acp",
        ToolProvider::Lsp => "lsp",
        ToolProvider::Mcp => "mcp",
        ToolProvider::Dotfiles => "dotfiles",
    }
}

fn status_label(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Available => "available",
        ToolStatus::Installed => "installed",
        ToolStatus::Outdated => "update",
        ToolStatus::Missing => "missing",
        ToolStatus::Conflict => "conflict",
        ToolStatus::Failed => "failed",
    }
}

fn status_tone(status: ToolStatus) -> ManagerTone {
    match status {
        ToolStatus::Installed => ManagerTone::Green,
        ToolStatus::Outdated | ToolStatus::Conflict => ManagerTone::Amber,
        _ => ManagerTone::Neutral,
    }
}

fn action_label(action: ToolAction) -> &'static str {
    match action {
        ToolAction::Install => "Install",
        ToolAction::Update => "Update",
        ToolAction::Uninstall => "Uninstall",
        ToolAction::Forget => "Forget",
        ToolAction::Adopt => "Manage",
        ToolAction::Link => "Link",
        ToolAction::Unlink => "Unlink",
        ToolAction::Apply => "Apply",
        ToolAction::Import => "Import",
    }
}

fn action_variant(action: ToolAction) -> ManagerButtonVariant {
    match action {
        ToolAction::Install | ToolAction::Link => ManagerButtonVariant::Primary,
        ToolAction::Uninstall | ToolAction::Forget | ToolAction::Unlink => {
            ManagerButtonVariant::Danger
        }
        _ => ManagerButtonVariant::Secondary,
    }
}

fn action_key(provider: ToolProvider, action: ToolAction, id: &str) -> String {
    format!("{}:{action:?}:{id}", provider.id())
}

fn open_tool_file(path: String) {
    let _ = try_cef_bin_emit_rkyv(&ToolOpenRequest { path });
}
