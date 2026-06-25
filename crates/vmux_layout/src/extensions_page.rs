#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::extension::*;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(ExtensionsEvent::default);
    let mut progress = use_signal(HashMap::<String, ExtInstallProgress>::new);
    let mut loaded = use_signal(|| false);
    let mut search = use_signal(String::new);

    let _list = use_bin_event_listener::<ExtensionsEvent, _>(EXTENSIONS_LIST_EVENT, move |e| {
        state.set(e);
        loaded.set(true);
    });
    let _prog =
        use_bin_event_listener::<ExtInstallProgress, _>(EXT_INSTALL_PROGRESS_EVENT, move |p| {
            let done = matches!(p.phase, ExtInstallPhase::Done | ExtInstallPhase::Failed);
            if done {
                progress.write().remove(&p.key);
            } else {
                progress.write().insert(p.key.clone(), p);
            }
        });
    let _stat = use_bin_event_listener::<ExtStatusEvent, _>(EXT_STATUS_EVENT, move |_s| {});

    use_effect(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title("Extensions");
        }
        let _ = try_cef_bin_emit_rkyv(&ExtListRequest);
    });

    let snap = state();
    let installing: Vec<ExtInstallProgress> = progress().values().cloned().collect();

    rsx! {
        div {
            class: "flex h-full w-full flex-col overflow-hidden bg-background text-foreground font-sans text-sm",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);",

            div { class: "flex shrink-0 items-center gap-3 border-b border-white/[0.07] px-5 py-3",
                div { class: "text-base font-semibold tracking-tight", "Extensions" }
                div { class: "rounded-full bg-white/[0.06] px-2 py-0.5 text-xs text-muted-foreground", "{snap.extensions.len()}" }
                div { class: "flex min-w-0 flex-1 items-center",
                    input {
                        r#type: "search",
                        class: "w-full max-w-80 rounded-lg bg-white/[0.05] px-3 py-1.5 text-xs text-foreground outline-none ring-1 ring-inset ring-white/10 transition-colors placeholder:text-muted-foreground/60 focus:bg-white/[0.08] focus:ring-cyan-400/30",
                        placeholder: "Search the Chrome Web Store…",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                let q = search();
                                if !q.trim().is_empty() {
                                    let _ = try_cef_bin_emit_rkyv(&ExtBrowseStoreRequest { query: q });
                                }
                            }
                        },
                    }
                }
                if snap.pending {
                    button {
                        class: "rounded-lg bg-cyan-400/15 px-3 py-1.5 text-xs font-medium text-cyan-200 ring-1 ring-inset ring-cyan-400/30 transition-colors hover:bg-cyan-400/25",
                        onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&crate::event::RestartRequestEvent); },
                        "Relaunch to apply"
                    }
                }
            }

            if !installing.is_empty() {
                div { class: "shrink-0 px-5 pb-2",
                    for pr in installing.iter() {
                        div { class: "truncate text-[10px] text-muted-foreground/70",
                            {format!("{}: {}{}", pr.key, pr.message, pr.pct.map(|p| format!(" {p}%")).unwrap_or_default())}
                        }
                    }
                }
            }

            div { class: "min-h-0 flex-1 overflow-auto px-3 pb-4",
                if !loaded() {
                    for i in 0..3 {
                        div {
                            key: "{i}",
                            class: "flex items-center gap-3 rounded-xl px-3 py-2.5",
                            div { class: "h-6 w-6 shrink-0 animate-pulse rounded bg-white/[0.06]" }
                            div { class: "flex min-w-0 flex-1 flex-col gap-1.5",
                                div { class: "h-3 w-32 animate-pulse rounded bg-white/[0.06]" }
                                div { class: "h-2.5 w-16 animate-pulse rounded bg-white/[0.05]" }
                            }
                        }
                    }
                } else if snap.extensions.is_empty() {
                    div { class: "flex flex-col items-center gap-3 px-3 py-16 text-center",
                        div { class: "text-sm text-muted-foreground", "No extensions installed yet." }
                        div { class: "text-xs text-muted-foreground/70",
                            "Search the Chrome Web Store above, then click \"Add to Vmux\"."
                        }
                    }
                }
                for ext in snap.extensions.iter() {
                    {
                        let e = ext.clone();
                        let toggle_id = e.id.clone();
                        let toggle_enabled = e.enabled;
                        let remove_id = e.id.clone();
                        rsx! {
                            div {
                                key: "{e.id}",
                                class: "flex items-center gap-3 rounded-xl px-3 py-2.5 transition-colors hover:bg-white/[0.04]",
                                if let Some(icon) = e.icon.as_ref() {
                                    img { class: "h-6 w-6 shrink-0 rounded", src: "{icon}" }
                                }
                                div { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                                    span { class: "truncate font-medium text-foreground/95", "{e.name}" }
                                    span { class: "text-xs text-muted-foreground/70", "v{e.version}" }
                                }
                                button {
                                    class: "shrink-0 rounded-lg px-3 py-1.5 text-xs ring-1 ring-inset ring-white/10 transition-colors hover:bg-white/[0.09]",
                                    onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtToggleRequest { id: toggle_id.clone(), enabled: !toggle_enabled }); },
                                    if e.enabled { "On" } else { "Off" }
                                }
                                button {
                                    class: "shrink-0 rounded-lg bg-white/[0.05] px-3 py-1.5 text-xs text-foreground/70 ring-1 ring-inset ring-white/10 transition-colors hover:bg-ansi-1/15 hover:text-ansi-1",
                                    onclick: move |_| { let _ = try_cef_bin_emit_rkyv(&ExtUninstallRequest { id: remove_id.clone() }); },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
