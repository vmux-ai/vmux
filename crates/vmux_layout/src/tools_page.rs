#![allow(non_snake_case)]

use std::collections::BTreeSet;

use dioxus::prelude::*;
use vmux_core::tools::{
    TOOL_ACTION_RESULT_EVENT, TOOLS_SNAPSHOT_EVENT, ToolAction, ToolActionRequest,
    ToolActionResult, ToolItem, ToolOpenRequest, ToolProvider, ToolStatus, ToolsRefreshRequest,
    ToolsSnapshot,
};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList, ManagerPage,
    ManagerRow, ManagerSpinner,
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
fn HomebrewSourceCard(root: String) -> Element {
    let brewfile = format!("{root}/Brewfile");
    let open_brewfile = brewfile.clone();
    rsx! {
        div { class: "flex items-center gap-3 rounded-2xl bg-foreground/[0.035] p-4 ring-1 ring-inset ring-foreground/10",
            div { class: "grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-amber-500/10 text-amber-700 ring-1 ring-inset ring-amber-500/20 dark:text-amber-300",
                svg { class: "h-5 w-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M17 11h1a4 4 0 0 1 0 8h-1" }
                    path { d: "M9 12v6" }
                    path { d: "M13 12v6" }
                    path { d: "M14 7.5c0-1.5-2.5-1.5-2.5 0 0-1.5-2.5-1.5-2.5 0 0-1.5-2.5-1.5-2.5 0" }
                    path { d: "M5 8h12v10a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2Z" }
                }
            }
            div { class: "min-w-0 flex-1",
                div { class: "font-medium text-foreground/95", "Homebrew" }
                div { class: "truncate text-xs text-muted-foreground/70", "{brewfile}" }
                div { class: "mt-1 text-[10px] text-muted-foreground/60",
                    "Installed formulae and casks sync automatically."
                }
            }
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                disabled: root.is_empty(),
                onclick: move |_| open_tool_file(open_brewfile.clone()),
                "Open Brewfile"
            }
        }
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
                span { class: "shrink-0 text-[10px] text-muted-foreground/60", "{provider_short_label(provider)}" }
                if item.managed {
                    span { class: "shrink-0 text-[10px] text-muted-foreground/60", "· managed" }
                }
                span { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground/70",
                    span { class: "size-1.5 rounded-full {status_dot_class(item.status)}" }
                    "{status_label(item.status)}"
                }
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

fn status_dot_class(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Installed => "bg-emerald-500",
        ToolStatus::Outdated => "bg-amber-500",
        ToolStatus::Conflict | ToolStatus::Failed => "bg-rose-500",
        ToolStatus::Missing => "bg-muted-foreground/40",
        ToolStatus::Available => "bg-cyan-500/70",
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
