#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerHeader, ManagerList,
    ManagerPage, ManagerRow, ManagerSkeleton, ManagerTone,
};
use vmux_ui::hooks::{send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

/// What enabling an extension is asking the reader to agree to.
///
/// Kept apart from its rendering because the two halves are not the same shape: the message is one
/// of two sentences, and the permissions are a list. They were one newline-joined string while
/// `window.confirm` was showing them, which is a plain-text box and the only thing it could take.
#[derive(Clone, PartialEq)]
struct Approval {
    message: String,
    permissions: Vec<String>,
}

impl Approval {
    fn of(extension: &ExtRow) -> Self {
        let mut permissions = extension.required_permissions.clone();
        permissions.extend(extension.required_host_permissions.iter().cloned());
        let id = if permissions.is_empty() {
            "extensions-enable-confirm"
        } else {
            "extensions-enable-permissions"
        };

        Self {
            message: translate_with(id, &[("name", TranslationValue::String(&extension.name))]),
            permissions,
        }
    }
}

#[component]
pub fn Page() -> Element {
    let locale = use_theme();
    let mut state = use_signal(ExtensionsEvent::default);
    let mut progress = use_signal(HashMap::<String, ExtInstallProgress>::new);
    let mut loaded = use_signal(|| false);
    let mut search = use_signal(String::new);

    let _list = use_listener::<ExtensionsEvent, _>(EXTENSIONS_LIST_EVENT, move |event| {
        state.set(event);
        loaded.set(true);
    });
    let _progress =
        use_listener::<ExtInstallProgress, _>(EXT_INSTALL_PROGRESS_EVENT, move |item| {
            if matches!(item.phase, ExtInstallPhase::Done | ExtInstallPhase::Failed) {
                progress.write().remove(&item.key);
            } else {
                progress.write().insert(item.key.clone(), item);
            }
        });
    let _status = use_listener::<ExtStatusEvent, _>(EXT_STATUS_EVENT, move |_| {});

    use_effect(move || {
        locale();
        let _ = send(&ExtListRequest);
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
                            let _ = send(&ExtBrowseStoreRequest { query });
                        }
                    }
                },
                actions: rsx! {
                    if snapshot.pending {
                        ManagerButton {
                            variant: ManagerButtonVariant::Primary,
                            onclick: move |_| {
                                let _ = send(&crate::event::RestartRequestEvent);
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
                    ExtensionRow { extension: extension.clone() }
                }
            }
        }
    }
}

/// One installed extension, with its enable and remove controls.
#[component]
fn ExtensionRow(extension: ExtRow) -> Element {
    let item = extension;
    let toggle_id = item.id.clone();
    let toggle_enabled = item.enabled;
    let needs_approval = item.needs_approval;
    let approval = Approval::of(&item);
    let name = item.name.clone();
    let remove_id = item.id.clone();
    let icon = item.icon.clone();
    let mut asking = use_signal(|| Some(false));
    let approve_id = toggle_id.clone();
    rsx! {
        AlertDialogRoot {
            open: Into::<ReadSignal<Option<bool>>>::into(asking),
            on_open_change: Callback::new(move |open| asking.set(Some(open))),
            default_open: false,
            attributes: vec![],
            AlertDialogContent { attributes: vec![],
                AlertDialogTitle { attributes: vec![], "{name}" }
                AlertDialogDescription { attributes: vec![], "{approval.message}" }
                if !approval.permissions.is_empty() {
                    ul { class: "mt-3 space-y-1 text-sm text-muted-foreground",
                        for permission in approval.permissions.iter() {
                            li { class: "font-mono text-xs", "{permission}" }
                        }
                    }
                }
                AlertDialogActions { attributes: vec![],
                    AlertDialogCancel {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |_| asking.set(Some(false)))),
                        {translate("common-cancel")}
                    }
                    AlertDialogAction {
                        attributes: vec![],
                        on_click: Some(EventHandler::new(move |_| {
                            asking.set(Some(false));
                            let _ = send(&ExtToggleRequest {
                                id: approve_id.clone(),
                                enabled: true,
                                approve_permissions: true,
                            });
                        })),
                        {translate("common-enable")}
                    }
                }
            }
        }
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
                        if enabling && needs_approval {
                            asking.set(Some(true));
                            return;
                        }
                        let _ = send(&ExtToggleRequest {
                            id: toggle_id.clone(),
                            enabled: enabling,
                            approve_permissions: false,
                        });
                    },
                    if item.enabled { {translate("common-disable")} } else { {translate("common-enable")} }
                }
                ManagerButton {
                    variant: ManagerButtonVariant::Danger,
                    onclick: move |_| {
                        let _ = send(&ExtUninstallRequest { id: remove_id.clone() });
                    },
                    {translate("common-remove")}
                }
            },
        }
    }
}
