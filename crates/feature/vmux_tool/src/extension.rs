#![allow(non_snake_case)]

use crate::state::ToolProviderMetadata;
use crate::ui::ToolsRouteSelector;
use dioxus::prelude::*;
use vmux_api::extension::*;
use vmux_ui::components::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use vmux_ui::components::manager::{
    ManagerBadge, ManagerButton, ManagerButtonVariant, ManagerEmpty, ManagerList, ManagerRow,
    ManagerSearch, ManagerSkeleton, ManagerThumbnail, ManagerTone, ManagerToolbar,
};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[derive(Clone, PartialEq)]
struct Approval {
    message: String,
    permissions: Vec<String>,
}

impl From<&ExtRow> for Approval {
    fn from(extension: &ExtRow) -> Self {
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
pub(crate) fn ExtensionManager(
    state: ExtensionsUiState,
    providers: Vec<ToolProviderMetadata>,
) -> Element {
    let installing = &state.installing;
    let browse_query = state.query.clone();

    rsx! {
        ManagerToolbar {
            ToolsRouteSelector {
                active: "extensions".to_string(),
                providers,
            }
            ManagerSearch {
                value: state.query.clone(),
                placeholder: translate("extensions-search"),
                onsearch: move |query: String| {
                    let _ = send(&ExtFilterRequest { query });
                },
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Enter {
                        let query = browse_query.clone();
                        if !query.trim().is_empty() {
                            let _ = send(&ExtBrowseStoreRequest { query });
                        }
                    }
                },
            }
            if state.pending {
                ManagerButton {
                    variant: ManagerButtonVariant::Primary,
                    onclick: move |_| {
                        let _ = send(&vmux_api::service::RelaunchRequest);
                    },
                    {translate("extensions-relaunch")}
                }
            }
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
            if !state.loaded {
                ManagerSkeleton {}
            } else if state.visible.is_empty() {
                ManagerEmpty {
                    title: if state.extensions.is_empty() { translate("extensions-empty") } else { translate("extensions-no-match") },
                    detail: if state.extensions.is_empty() {
                        translate("extensions-empty-detail")
                    } else {
                        translate("extensions-no-match-detail")
                    },
                }
            }
            for extension in state.visible.iter() {
                ExtensionRow { extension: extension.clone() }
            }
        }
    }
}

#[component]
fn ExtensionRow(extension: ExtRow) -> Element {
    let item = extension;
    let toggle_id = item.id.clone();
    let toggle_enabled = item.enabled;
    let needs_approval = item.needs_approval;
    let approval = Approval::from(&item);
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
            icon: rsx! { ManagerThumbnail { src: icon, fallback: "EXT".to_string() } },
            title: item.name.clone(),
            subtitle: format!("v{}", item.version),
            meta: rsx! {
                ManagerBadge {
                    tone: if item.enabled { ManagerTone::Primary } else { ManagerTone::Neutral },
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
