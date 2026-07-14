#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSpinner, ManagerTone,
};
use vmux_ui::file_icon::{FileIcon, file_icon_kind, type_icon};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

use crate::page_model::{PkgAction, pkg_action, pkg_status_class, pkg_status_label};

fn request_catalog(query: String, refresh: bool) {
    let _ = try_cef_bin_emit_rkyv(&LspCatalogRequest {
        query,
        language: String::new(),
        category: String::new(),
        installed_only: false,
        refresh,
    });
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut packages = use_signal(Vec::<LspPackage>::new);
    let mut query = use_signal(String::new);
    let mut progress = use_signal(HashMap::<String, LspInstallProgress>::new);
    let mut loading = use_signal(|| true);

    let _catalog = use_bin_event_listener::<LspCatalogEvent, _>(LSP_CATALOG_EVENT, move |event| {
        packages.set(event.packages);
        loading.set(false);
    });
    let _progress =
        use_bin_event_listener::<LspInstallProgress, _>(LSP_INSTALL_PROGRESS_EVENT, move |item| {
            let name = item.name.clone();
            let phase = item.phase;
            progress.write().insert(name.clone(), item);
            if let Some(package) = packages
                .write()
                .iter_mut()
                .find(|package| package.name == name)
            {
                package.status = match phase {
                    InstallPhase::Failed => LspPkgStatus::Failed,
                    InstallPhase::Done => LspPkgStatus::Installed,
                    _ => LspPkgStatus::Installing,
                };
            }
        });
    let _status =
        use_bin_event_listener::<LspPkgStatusEvent, _>(LSP_PKG_STATUS_EVENT, move |status| {
            let name = status.name.clone();
            if let Some(package) = packages
                .write()
                .iter_mut()
                .find(|package| package.name == name)
            {
                package.status = status.status;
                package.version = status.version;
            }
            progress.write().remove(&name);
        });

    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
            doc.set_title("Language Servers");
        }
        request_catalog(String::new(), false);
    });

    let visible = packages();
    rsx! {
        ManagerPage {
            ManagerHeader {
                title: "Language Servers",
                count: visible.len(),
                search_value: query(),
                search_placeholder: "Search language servers, linters, formatters…",
                onsearch: move |event: FormEvent| {
                    let value = event.value();
                    query.set(value.clone());
                    request_catalog(value, false);
                },
                onkeydown: None,
                actions: rsx! {
                    ManagerButton {
                        variant: ManagerButtonVariant::Secondary,
                        onclick: move |_| {
                            loading.set(true);
                            request_catalog(query(), true);
                        },
                        "Refresh"
                    }
                },
            }
            ManagerList {
                if loading() && visible.is_empty() {
                    ManagerSpinner { detail: "Loading catalog…" }
                } else if visible.is_empty() {
                    ManagerEmpty {
                        title: "No matching language servers",
                        detail: "Try another language, linter, or formatter.",
                    }
                }
                for package in visible.iter() {
                    {render_package(package, progress)}
                }
            }
        }
    }
}

fn render_package(
    package: &LspPackage,
    progress: Signal<HashMap<String, LspInstallProgress>>,
) -> Element {
    let item = package.clone();
    let install_progress = progress().get(&item.name).cloned();
    let action = pkg_action(item.status, item.installable);
    let action_name = item.name.clone();
    let mut subtitle = item.version.clone().unwrap_or_default();
    if let Some(progress) = install_progress.as_ref() {
        subtitle = format!(
            "{}{}",
            progress.message,
            progress
                .pct
                .map(|percent| format!(" {percent}%"))
                .unwrap_or_default()
        );
    }
    let icon_path = language_icon_path(&item.languages);
    let show_icon = icon_path.is_some();
    rsx! {
        ManagerRow {
            show_icon,
            icon: rsx! {
                if let Some(path) = icon_path.as_ref() {
                    {type_icon(path, false, "h-6 w-6 text-foreground/80")}
                }
            },
            title: item.name.clone(),
            subtitle,
            meta: rsx! {
                for language in item.languages.iter().take(3) {
                    ManagerBadge { tone: ManagerTone::Neutral, "{language}" }
                }
                for category in item.categories.iter().take(2) {
                    ManagerBadge { tone: ManagerTone::Cyan, "{category}" }
                }
            },
            actions: rsx! {
                span { class: "shrink-0 text-xs {pkg_status_class(item.status)}", "{pkg_status_label(item.status)}" }
                {render_action(action, &action_name, item.requires.as_deref())}
            },
        }
    }
}

fn language_icon_path(languages: &[String]) -> Option<String> {
    languages.iter().find_map(|language| {
        let normalized = language.trim().to_ascii_lowercase();
        let extension = match normalized.as_str() {
            "rust" => "rs",
            "typescript" => "ts",
            "typescriptreact" | "typescript react" => "tsx",
            "javascript" => "js",
            "javascriptreact" | "javascript react" => "jsx",
            "python" => "py",
            "ruby" => "rb",
            "shell" | "bash" | "zsh" => "sh",
            "c++" | "cpp" => "cpp",
            "kotlin" => "kt",
            "elixir" => "ex",
            "haskell" => "hs",
            "ocaml" => "ml",
            "clojure" => "clj",
            "erlang" => "erl",
            "julia" => "jl",
            "perl" => "pl",
            "f#" | "fsharp" => "fs",
            "markdown" => "md",
            "sass" => "scss",
            "graphql" => "graphql",
            "yml" => "yaml",
            "docker" => "dockerfile",
            "terraform" | "hcl" => "tf",
            "nix" | "nixos" => "nix",
            "jupyter" => "ipynb",
            "webassembly" => "wasm",
            "powershell" => "ps1",
            "sql" => "sqlite",
            other => other,
        };
        let path = format!("language.{extension}");
        matches!(file_icon_kind(&path, false), FileIcon::Logo(_)).then_some(path)
    })
}

fn render_action(action: PkgAction, name: &str, requires: Option<&str>) -> Element {
    let install_name = name.to_string();
    let update_name = name.to_string();
    let uninstall_name = name.to_string();
    match action {
        PkgAction::Install => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Primary,
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&LspInstallRequest { name: install_name.clone() });
                },
                "Install"
            }
        },
        PkgAction::Update => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Secondary,
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&LspUpdateRequest { name: update_name.clone() });
                },
                "Update"
            }
        },
        PkgAction::Uninstall => rsx! {
            ManagerButton {
                variant: ManagerButtonVariant::Danger,
                onclick: move |_| {
                    let _ = try_cef_bin_emit_rkyv(&LspUninstallRequest { name: uninstall_name.clone() });
                },
                "Uninstall"
            }
        },
        PkgAction::None => match requires {
            Some(tool) => {
                rsx! { span { class: "text-[10px] text-muted-foreground/60", "needs {tool}" } }
            }
            None => rsx! {},
        },
    }
}
