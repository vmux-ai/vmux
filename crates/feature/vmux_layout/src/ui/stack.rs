use dioxus::prelude::*;
use vmux_api::bookmark::{BookmarkAddRequest, BookmarkFolderChoice, BookmarkPinUrlRequest};
use vmux_core::{PageIcon, PageMetadata};
use vmux_ui::components::context_menu::{ContextMenuItem, ContextMenuTrigger};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::PageIconView;

use super::bookmark::{
    BookmarkDragItem, BookmarkDragState, LayoutContextMenu, SideSheetContextMenuContent,
};
use crate::event::StackNode;

#[component]
pub(super) fn SideSheetStackRow(stack: StackNode, pane_id: u64) -> Element {
    let folder_context: Signal<Vec<BookmarkFolderChoice>> = use_context();
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let folders = folder_context();
    let is_active = stack.is_active;
    let stack_id = stack.id;
    let mut hovered = use_signal(|| false);
    let menu_val = use_signal(|| stack.url.clone());
    let metadata = PageMetadata {
        title: stack.title.clone(),
        url: stack.url.clone(),
        icon: stack.icon.clone(),
        bg_color: stack.bg_color.clone(),
    };
    let drag_item = BookmarkDragItem::Page {
        metadata: metadata.clone(),
    };
    let bookmark_metadata = metadata.clone();
    let pin_metadata = metadata;
    let pin_index = 1 + folders.len();
    let display_title = StackTitle::resolve(&stack);
    let close_title = translate("layout-close-stack");

    let title_class = if is_active {
        format!(
            "min-w-0 flex-1 {} text-ui font-medium text-foreground",
            StackTitle::truncate_class(&display_title)
        )
    } else {
        format!(
            "min-w-0 flex-1 {} text-ui",
            StackTitle::truncate_class(&display_title)
        )
    };

    rsx! {
        LayoutContextMenu {
            ContextMenuTrigger {
                attributes: vec![],
                div {
                    "data-bookmark-drag-source": "true",
                    onpointerdown: {
                        let item = drag_item.clone();
                        move |event| BookmarkDragState::begin(drag_state, &event, item.clone())
                    },
                    id: "sidesheet-stack-{pane_id}-{stack_id}",
                    class: if is_active {
                        "flex h-9 cursor-default items-center gap-2 rounded-md bg-primary/[0.07] px-2 shadow-[inset_2px_0_0_var(--primary)] ring-1 ring-inset ring-primary/15"
                    } else {
                        "flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-muted-foreground hover:bg-glass-hover hover:text-foreground"
                    },
                    onmouseenter: move |_| hovered.set(true),
                    onmouseleave: move |_| hovered.set(false),
                    onclick: move |event| {
                        if BookmarkDragState::blocks_click(drag_state) {
                            event.prevent_default();
                            event.stop_propagation();
                            return;
                        }
                        let _ = send(&crate::event::SideSheetStackActivateRequest {
                            pane_id,
                            stack_id,
                        });
                    },
                    StackIcon { icon: stack.icon.clone(), url: stack.url.clone(), title: stack.title.clone() }
                    span { class: "{title_class}", "{display_title}" }
                    button {
                        r#type: "button",
                        aria_label: "{close_title}",
                        title: "{close_title}",
                        class: if hovered() {
                            "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-100 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                        } else {
                            "ml-auto flex h-6 w-6 cursor-pointer shrink-0 items-center justify-center rounded-sm opacity-0 transition-opacity focus-visible:opacity-100 hover:bg-foreground/10"
                        },
                        onmousedown: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                        },
                        onpointerdown: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                        },
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            let _ = send(&crate::event::SideSheetStackCloseRequest {
                                pane_id,
                                stack_id,
                            });
                        },
                        Icon { class: "h-3 w-3 pointer-events-none",
                            path { d: "M18 6 6 18" }
                            path { d: "m6 6 12 12" }
                        }
                    }
                }
            }
            SideSheetContextMenuContent {
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| {
                        let _ = send(&BookmarkAddRequest {
                            metadata: bookmark_metadata.clone(),
                            folder: None,
                        });
                    },
                    attributes: vec![],
                    {translate("layout-bookmark")}
                }
                for (index, folder) in folders.iter().enumerate() {
                    ContextMenuItem {
                        key: "{folder.uuid}",
                        index: 1usize + index,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let metadata = PageMetadata {
                                title: stack.title.clone(),
                                url: stack.url.clone(),
                                icon: stack.icon.clone(),
                                bg_color: stack.bg_color.clone(),
                            };
                            let folder_uuid = folder.uuid.clone();
                            move |_: String| {
                                let _ = send(&BookmarkAddRequest {
                                    metadata: metadata.clone(),
                                    folder: Some(folder_uuid.clone()),
                                });
                            }
                        },
                        attributes: vec![],
                    {translate_with(
                        "layout-bookmark-in",
                        &[("folder", TranslationValue::String(&folder.label))],
                    )}
                    }
                }
                ContextMenuItem {
                    index: pin_index,
                    value: Into::<ReadSignal<String>>::into(menu_val),
                    on_select: move |_: String| {
                        let _ = send(&BookmarkPinUrlRequest {
                            metadata: pin_metadata.clone(),
                        });
                    },
                    attributes: vec![],
                    {translate("layout-pin")}
                }
            }
        }
    }
}

#[component]
pub(super) fn NewStackRow(pane_id: u64) -> Element {
    rsx! {
        SheetNewButton {
            label: translate("layout-new-stack"),
            icon: rsx! {
                Icon { class: "h-4 w-4 shrink-0",
                    path { d: "M12 5v14" }
                    path { d: "M5 12h14" }
                }
            },
            onclick: move |_| {
                let _ = send(&crate::event::SideSheetStackCreateRequest { pane_id });
            },
        }
    }
}

#[component]
pub(super) fn StackIcon(icon: PageIcon, url: String, title: String) -> Element {
    if title == "New Stack" && url.is_empty() {
        return rsx! {
            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                path { d: "M5 12h14" }
                path { d: "M12 5v14" }
            }
        };
    }
    rsx! {
        PageIconView {
            icon,
            url,
            img_class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
            icon_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
        }
    }
}

pub(super) struct StackTitle;

impl StackTitle {
    pub(super) fn truncate_class(title: &str) -> &'static str {
        if title.contains('/') {
            "truncate-start"
        } else {
            "truncate"
        }
    }

    fn resolve(stack: &StackNode) -> String {
        let title = localized_stack_title(stack);
        if !title.trim().is_empty() {
            return title;
        }
        if let Some(host) = vmux_ui::favicon::host_for_favicon_fallback(&stack.url) {
            return host.to_string();
        }
        if stack.is_loading {
            return translate("layout-loading");
        }
        if stack.url.is_empty() {
            return translate("layout-new-stack");
        }
        stack.url.clone()
    }
}

fn localized_stack_title(stack: &StackNode) -> String {
    if let Some(route) = vmux_api::VmuxRoute::parse(&stack.url) {
        if route.is_host("start") && route.is_root() {
            return translate("start-title");
        }
        if route.is_host("settings") && route.is_root() {
            return translate("settings-title");
        }
    }
    if stack.url.is_empty() && stack.title == "New Stack" {
        return translate("layout-new-stack");
    }
    stack.title.clone()
}

#[component]
fn SheetNewButton(label: String, icon: Element, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "group flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 border border-transparent text-left text-muted-foreground hover:bg-glass-hover hover:text-foreground",
            onclick: move |e| onclick.call(e),
            {icon}
            span { class: "min-w-0 flex-1 truncate text-ui font-medium", "{label}" }
        }
    }
}
