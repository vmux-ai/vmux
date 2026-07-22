#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::extension::*;
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSkeleton, ManagerTone,
};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

fn approval_message(extension: &ExtRow) -> String {
    let mut requested = extension.required_permissions.clone();
    requested.extend(extension.required_host_permissions.iter().cloned());
    if requested.is_empty() {
        translate_with(
            "extensions-enable-confirm",
            &[("name", TranslationValue::String(&extension.name))],
        )
    } else {
        let permissions = requested.join("\n");
        let message = translate_with(
            "extensions-enable-permissions",
            &[("name", TranslationValue::String(&extension.name))],
        );
        format!("{message}\n\n{permissions}")
    }
}

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let mut state = use_signal(ExtensionsEvent::default);
    let mut progress = use_signal(HashMap::<String, ExtInstallProgress>::new);
    let mut loaded = use_signal(|| false);
    let mut search = use_signal(String::new);

    let _list = use_bin_event_listener::<ExtensionsEvent, _>(EXTENSIONS_LIST_EVENT, move |event| {
        state.set(event);
        loaded.set(true);
    });
    let _progress =
        use_bin_event_listener::<ExtInstallProgress, _>(EXT_INSTALL_PROGRESS_EVENT, move |item| {
            if matches!(item.phase, ExtInstallPhase::Done | ExtInstallPhase::Failed) {
                progress.write().remove(&item.key);
            } else {
                progress.write().insert(item.key.clone(), item);
            }
        });
    let _status = use_bin_event_listener::<ExtStatusEvent, _>(EXT_STATUS_EVENT, move |_| {});

    use_effect(move || {
        locale();
        if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
            doc.set_title(&translate("extensions-title"));
        }
        let _ = try_cef_bin_emit_rkyv(&ExtListRequest);
    });

    let snapshot = state();
    let query = search().trim().to_lowercase();
    let visible: Vec<ExtRow> = snapshot
        .extensions
        .iter()
        .filter(|extension| {
            query.is_empty()
                || extension.name.to_lowercase().contains(&query)
                || extension.id.to_lowercase().contains(&query)
                || extension.version.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    let installing: Vec<ExtInstallProgress> = progress().values().cloned().collect();

    rsx! {
        ManagerPage {
            ManagerHeader {
                title: translate("extensions-title"),
                count: snapshot.extensions.len(),
                search_value: search(),
                search_placeholder: translate("extensions-search"),
                onsearch: move |event: FormEvent| search.set(event.value()),
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Enter {
                        let query = search();
                        if !query.trim().is_empty() {
                            let _ = try_cef_bin_emit_rkyv(&ExtBrowseStoreRequest { query });
                        }
                    }
                },
                actions: rsx! {
                    if snapshot.pending {
                        ManagerButton {
                            variant: ManagerButtonVariant::Primary,
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&crate::event::RestartRequestEvent);
                            },
                            {translate("extensions-relaunch")}
                        }
                    }
                },
            }
            if !installing.is_empty() {
                div { class: "shrink-0 px-5 pt-3",
                    for item in installing.iter() {
                        div { class: "truncate text-[10px] text-muted-foreground/70",
                            {format!(
                                "{}: {}{}",
                                item.key,
                                item.message,
                                item.pct.map(|percent| format!(" {percent}%")).unwrap_or_default()
                            )}
                        }
                    }
                }
            }
            ManagerList {
                if !loaded() {
                    ManagerSkeleton {}
                } else if visible.is_empty() {
                    ManagerEmpty {
                        title: if snapshot.extensions.is_empty() { translate("extensions-empty") } else { translate("extensions-no-match") },
                        detail: if snapshot.extensions.is_empty() {
                            translate("extensions-empty-detail")
                        } else {
                            translate("extensions-no-match-detail")
                        },
                    }
                }
                for extension in visible.iter() {
                    {render_extension(extension)}
                }
            }
        }
    }
}

fn render_extension(extension: &ExtRow) -> Element {
    let item = extension.clone();
    let toggle_id = item.id.clone();
    let toggle_enabled = item.enabled;
    let needs_approval = item.needs_approval;
    let approval = approval_message(&item);
    let remove_id = item.id.clone();
    let icon = item.icon.clone();
    rsx! {
        ManagerRow {
            icon: rsx! {
                if let Some(icon) = icon.as_ref() {
                    img { class: "h-6 w-6 rounded object-contain", src: "{icon}" }
                } else {
                    span { class: "font-mono text-[10px] text-muted-foreground", "EXT" }
                }
            },
            title: item.name.clone(),
            subtitle: format!("v{}", item.version),
            meta: rsx! {
                ManagerBadge {
                    tone: if item.enabled { ManagerTone::Green } else { ManagerTone::Neutral },
                    if item.enabled { {translate("extensions-on")} } else { {translate("extensions-off")} }
                }
            },
            actions: rsx! {
                ManagerButton {
                    variant: ManagerButtonVariant::Secondary,
                    onclick: move |_| {
                        let enabling = !toggle_enabled;
                        let approve_permissions = if enabling && needs_approval {
                            web_sys::window()
                                .and_then(|window| window.confirm_with_message(&approval).ok())
                                .unwrap_or(false)
                        } else {
                            false
                        };
                        if enabling && needs_approval && !approve_permissions {
                            return;
                        }
                        let _ = try_cef_bin_emit_rkyv(&ExtToggleRequest {
                            id: toggle_id.clone(),
                            enabled: enabling,
                            approve_permissions,
                        });
                    },
                    if item.enabled { {translate("common-disable")} } else { {translate("common-enable")} }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Danger,
                    onclick: move |_| {
                        let _ = try_cef_bin_emit_rkyv(&ExtUninstallRequest { id: remove_id.clone() });
                    },
                    {translate("common-remove")}
                }
            },
        }
    }
}
