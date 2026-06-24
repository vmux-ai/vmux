#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::*;

use crate::page_model::{PkgAction, pkg_action, pkg_status_class, pkg_status_label};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

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

    let _cat = use_bin_event_listener::<LspCatalogEvent, _>(LSP_CATALOG_EVENT, move |e| {
        packages.set(e.packages);
        loading.set(false);
    });

    let _prog =
        use_bin_event_listener::<LspInstallProgress, _>(LSP_INSTALL_PROGRESS_EVENT, move |p| {
            let name = p.name.clone();
            let phase = p.phase;
            progress.write().insert(name.clone(), p);
            if let Some(pk) = packages.write().iter_mut().find(|x| x.name == name) {
                pk.status = match phase {
                    InstallPhase::Failed => LspPkgStatus::Failed,
                    InstallPhase::Done => LspPkgStatus::Installed,
                    _ => LspPkgStatus::Installing,
                };
            }
        });

    let _stat = use_bin_event_listener::<LspPkgStatusEvent, _>(LSP_PKG_STATUS_EVENT, move |s| {
        let name = s.name.clone();
        if let Some(pk) = packages.write().iter_mut().find(|x| x.name == name) {
            pk.status = s.status;
            pk.version = s.version;
        }
        progress.write().remove(&name);
    });

    use_effect(move || {
        request_catalog(String::new(), false);
    });

    let pkgs = packages();
    let total = pkgs.len();

    rsx! {
        div {
            class: "flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-sans text-sm",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);",

            div { class: "flex shrink-0 items-center gap-3 border-b border-white/[0.07] px-5 py-3",
                div { class: "text-base font-semibold tracking-tight", "Language Servers" }
                div { class: "rounded-full bg-white/[0.06] px-2 py-0.5 text-xs text-muted-foreground", "{total}" }
                div { class: "flex-1" }
                button {
                    class: "rounded-lg bg-white/[0.05] px-3 py-1.5 text-xs text-foreground/80 ring-1 ring-inset ring-white/10 transition-colors hover:bg-white/[0.09]",
                    onclick: move |_| { loading.set(true); request_catalog(query(), true); },
                    "Refresh"
                }
            }

            div { class: "shrink-0 px-5 py-3",
                input {
                    r#type: "text",
                    value: "{query}",
                    placeholder: "Search language servers, linters, formatters…",
                    class: "w-full rounded-xl bg-white/[0.04] px-4 py-2.5 text-sm text-foreground placeholder:text-muted-foreground/60 ring-1 ring-inset ring-white/10 outline-none focus:ring-cyan-400/30",
                    oninput: move |e| { query.set(e.value()); request_catalog(e.value(), false); },
                }
            }

            div { class: "min-h-0 flex-1 overflow-auto px-3 pb-4",
                if loading() && pkgs.is_empty() {
                    div { class: "px-3 py-6 text-center text-xs text-muted-foreground", "Loading catalog…" }
                }
                for pkg in pkgs.iter() {
                    {
                        let p = pkg.clone();
                        let prog = progress().get(&p.name).cloned();
                        let action = pkg_action(p.status, p.installable);
                        let name_for_btn = p.name.clone();
                        rsx! {
                            div {
                                key: "{p.name}",
                                class: "flex items-center gap-3 rounded-xl px-3 py-2.5 transition-colors hover:bg-white/[0.04]",

                                div { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                                    div { class: "flex items-center gap-2",
                                        span { class: "truncate font-medium text-foreground/95", "{p.name}" }
                                        if let Some(v) = p.version.as_ref() {
                                            span { class: "text-xs text-muted-foreground/70", "{v}" }
                                        }
                                    }
                                    div { class: "flex flex-wrap items-center gap-1.5",
                                        for lang in p.languages.iter().take(3) {
                                            span { class: "rounded-full bg-white/[0.05] px-2 py-0.5 text-[10px] text-foreground/60", "{lang}" }
                                        }
                                        for cat in p.categories.iter().take(2) {
                                            span { class: "rounded-full bg-cyan-400/10 px-2 py-0.5 text-[10px] text-cyan-300/80", "{cat}" }
                                        }
                                    }
                                    if let Some(pr) = prog.as_ref() {
                                        div { class: "mt-0.5 truncate text-[10px] text-muted-foreground/70",
                                            {format!("{}{}", pr.message, pr.pct.map(|p| format!(" {p}%")).unwrap_or_default())}
                                        }
                                    }
                                }

                                span { class: "shrink-0 text-xs {pkg_status_class(p.status)}", "{pkg_status_label(p.status)}" }

                                {render_action(action, &name_for_btn, p.requires.as_deref())}
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_action(action: PkgAction, name: &str, requires: Option<&str>) -> Element {
    let n = name.to_string();
    match action {
        PkgAction::Install => rsx! {
            button {
                class: "shrink-0 rounded-lg bg-cyan-400/15 px-3 py-1.5 text-xs font-medium text-cyan-200 ring-1 ring-inset ring-cyan-400/30 transition-colors hover:bg-cyan-400/25",
                onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&LspInstallRequest { name: n.clone() }); },
                "Install"
            }
        },
        PkgAction::Update => rsx! {
            button {
                class: "shrink-0 rounded-lg bg-amber-400/15 px-3 py-1.5 text-xs font-medium text-amber-200 ring-1 ring-inset ring-amber-400/30 transition-colors hover:bg-amber-400/25",
                onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&LspUpdateRequest { name: n.clone() }); },
                "Update"
            }
        },
        PkgAction::Uninstall => rsx! {
            button {
                class: "shrink-0 rounded-lg bg-white/[0.05] px-3 py-1.5 text-xs text-foreground/70 ring-1 ring-inset ring-white/10 transition-colors hover:bg-ansi-1/15 hover:text-ansi-1",
                onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&LspUninstallRequest { name: n.clone() }); },
                "Uninstall"
            }
        },
        PkgAction::None => match requires {
            Some(tool) => rsx! {
                span { class: "shrink-0 rounded-lg px-3 py-1.5 text-[10px] text-muted-foreground/60", "needs {tool}" }
            },
            None => rsx! { span {} },
        },
    }
}
