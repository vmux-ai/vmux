#![allow(non_snake_case)]

use std::rc::Rc;

use dioxus::prelude::*;
use vmux_core::event::{
    ExtOpenManagerRequest, ExtPinRequest, ExtRow, ExtensionPopupAnchor,
    ExtensionPopupBoundsRequest, ExtensionPopupCloseRequest, ExtensionPopupEvent,
    ExtensionPopupOpenRequest, ExtensionPopupSizeEvent,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

use crate::event::LayoutOverlayEvent;

#[component]
pub(crate) fn ExtensionPopupModal(
    popup: Signal<ExtensionPopupEvent>,
    preferred_size: ExtensionPopupSizeEvent,
) -> Element {
    let current = popup();
    let state = ExtensionPopupState { popup };
    let placement = ExtensionPopupPlacement::from(current.anchor);
    let size = if preferred_size.id == current.id {
        preferred_size
    } else {
        ExtensionPopupSizeEvent {
            id: current.id.clone(),
            width: 360.0,
            height: 600.0,
        }
    };
    let reporter = ExtensionPopupBoundsReporter {
        region: use_signal(|| None::<Rc<MountedData>>),
    };
    let mounted = reporter;
    let resized = reporter;
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: true,
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: false,
        });
        let _ = send(&ExtensionPopupCloseRequest);
    });

    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0 z-[1000]",
            tabindex: "-1",
            onmounted: move |event: Event<MountedData>| {
                let target = event.data();
                spawn(async move {
                    let _ = target.set_focus(true).await;
                });
            },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    state.close();
                }
            },
            onpointerdown: move |_| state.close(),
            div {
                key: "{current.id}",
                class: "pointer-events-none absolute",
                style: "{placement.style(&size)}",
                onpointerdown: move |event| event.stop_propagation(),
                onmounted: move |event: Event<MountedData>| mounted.mount(event.data()),
                onresize: move |_: Event<ResizeData>| resized.publish(),
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupPlacement {
    right: i32,
    top: i32,
}

impl From<ExtensionPopupAnchor> for ExtensionPopupPlacement {
    fn from(anchor: ExtensionPopupAnchor) -> Self {
        Self {
            right: anchor.right.max(48),
            top: anchor.bottom.max(40) + 14,
        }
    }
}

impl ExtensionPopupPlacement {
    fn style(self, size: &ExtensionPopupSizeEvent) -> String {
        format!(
            "right:max(8px,calc(100vw - {}px));top:{}px;width:min({:.0}px,calc(100vw - 16px));height:min({:.0}px,calc(100vh - {}px));",
            self.right,
            self.top,
            size.width.clamp(200.0, 360.0),
            size.height.clamp(80.0, 600.0),
            self.top + 8
        )
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupState {
    popup: Signal<ExtensionPopupEvent>,
}

impl ExtensionPopupState {
    fn close(mut self) {
        let _ = send(&LayoutOverlayEvent {
            id: "extension-popup".to_string(),
            active: false,
        });
        let _ = send(&ExtensionPopupCloseRequest);
        self.popup.set(ExtensionPopupEvent::default());
    }
}

#[derive(Clone, Copy)]
struct ExtensionPopupBoundsReporter {
    region: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionPopupBoundsReporter {
    fn mount(mut self, region: Rc<MountedData>) {
        self.region.set(Some(region));
        self.publish();
    }

    fn publish(self) {
        spawn(async move {
            let Some(region) = (self.region)() else {
                return;
            };
            let Ok(rect) = region.get_client_rect().await else {
                return;
            };
            let _ = send(&ExtensionPopupBoundsRequest {
                left: rect.origin.x as f32,
                top: rect.origin.y as f32,
                width: rect.size.width as f32,
                height: rect.size.height as f32,
            });
        });
    }
}

#[component]
pub(crate) fn ExtensionBar(extensions: Vec<ExtRow>) -> Element {
    let open = use_signal(|| false);
    let mut trigger = use_signal(|| None::<Rc<MountedData>>);
    let menu = ExtensionMenuState { open, trigger };
    let enabled = extensions
        .into_iter()
        .filter(|extension| extension.enabled)
        .map(Rc::new)
        .collect::<Vec<_>>();
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extensions".to_string(),
            active: open(),
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: "extensions".to_string(),
            active: false,
        });
    });

    rsx! {
        div { class: "relative flex shrink-0 items-center gap-1 pl-1",
            for ext in enabled.iter().filter(|extension| extension.pinned) {
                ExtensionActionButton { key: "{ext.id}", extension: ext.clone() }
            }
            button {
                r#type: "button",
                class: "flex h-7 w-7 items-center justify-center rounded-lg text-foreground/80 hover:bg-foreground/[0.08]",
                title: translate("layout-manage-extensions"),
                aria_label: translate("extensions-title"),
                aria_expanded: open(),
                onmounted: move |event| trigger.set(Some(event.data())),
                onclick: move |_| menu.toggle(),
                Icon { class: "h-4 w-4",
                    path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                }
            }
            if open() {
                div {
                    class: "pointer-events-auto fixed inset-0 z-[998] h-screen w-screen bg-transparent",
                    aria_hidden: "true",
                    onpointerdown: move |event| {
                        event.prevent_default();
                        menu.close();
                    },
                    oncontextmenu: move |event| {
                        event.prevent_default();
                        menu.close();
                    },
                }
                div {
                    class: "glass absolute right-0 top-9 z-[999] w-72 overflow-hidden rounded-xl border border-border/80 bg-background/95 shadow-2xl backdrop-blur-xl outline-none",
                    tabindex: "-1",
                    onmounted: move |event: Event<MountedData>| {
                        let target = event.data();
                        spawn(async move {
                            let _ = target.set_focus(true).await;
                        });
                    },
                    onkeydown: move |event| {
                        if event.key() == Key::Escape {
                            event.prevent_default();
                            menu.close();
                        }
                    },
                    div { class: "border-b border-border/70 px-3 py-2 text-xs font-semibold text-foreground",
                        {translate("extensions-title")}
                    }
                    if enabled.is_empty() {
                        div { class: "px-3 py-4 text-center text-xs text-muted-foreground",
                            {translate("extensions-empty")}
                        }
                    } else {
                        div { class: "max-h-72 overflow-y-auto p-1.5",
                            for extension in enabled.iter() {
                                ExtensionMenuRow {
                                    extension: extension.clone(),
                                    anchor: trigger,
                                    on_close: move |_| menu.close(),
                                }
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "flex w-full items-center gap-2 border-t border-border/70 px-3 py-2 text-left text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground",
                        onclick: move |_| {
                            menu.close();
                            let _ = send(&ExtOpenManagerRequest);
                        },
                        Icon { class: "size-3.5",
                            path { d: "M12 15.5A3.5 3.5 0 1 0 12 8a3.5 3.5 0 0 0 0 7.5Z" }
                            path { d: "M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3a2 2 0 1 1 4 0v.09A1.7 1.7 0 0 0 15.4 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.4 9c.12.37.34.7.65.94.3.24.68.37 1.06.37H21a2 2 0 1 1 0 4h-.09c-.38 0-.76.13-1.06.37-.31.24-.53.57-.65.94Z" }
                        }
                        {translate("layout-manage-extensions")}
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionActionButton(extension: Rc<ExtRow>) -> Element {
    let mounted = use_signal(|| None::<Rc<MountedData>>);
    let action = ExtensionActionState {
        id: extension.id.clone(),
        mounted,
    };

    rsx! {
        button {
            r#type: "button",
            class: "flex h-7 w-7 items-center justify-center rounded-lg hover:bg-foreground/[0.08]",
            title: "{extension.name}",
            aria_label: "{extension.name}",
            onmounted: move |event: Event<MountedData>| {
                let mut mounted = mounted;
                mounted.set(Some(event.data()));
            },
            onclick: move |_| action.clone().open(),
            if let Some(icon) = extension.icon.as_ref() {
                img { class: "h-4 w-4", src: "{icon}", alt: "" }
            } else {
                Icon { class: "h-4 w-4",
                    path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                }
            }
        }
    }
}

#[derive(Clone)]
struct ExtensionActionState {
    id: String,
    mounted: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionActionState {
    fn open(self) {
        spawn(async move {
            let anchor = match self.mounted.peek().clone() {
                Some(mounted) => mounted
                    .get_client_rect()
                    .await
                    .ok()
                    .map(|rect| ExtensionPopupAnchor {
                        right: (rect.origin.x + rect.size.width) as i32,
                        bottom: (rect.origin.y + rect.size.height) as i32,
                    })
                    .unwrap_or_default(),
                None => ExtensionPopupAnchor::default(),
            };
            let _ = send(&ExtensionPopupOpenRequest {
                id: self.id,
                anchor,
            });
        });
    }
}

#[derive(Clone, Copy)]
struct ExtensionMenuState {
    open: Signal<bool>,
    trigger: Signal<Option<Rc<MountedData>>>,
}

impl ExtensionMenuState {
    fn toggle(mut self) {
        if (self.open)() {
            self.close();
        } else {
            self.open.set(true);
        }
    }

    fn close(mut self) {
        self.open.set(false);
        let Some(trigger) = self.trigger.peek().clone() else {
            return;
        };
        spawn(async move {
            let _ = trigger.set_focus(true).await;
        });
    }
}

#[component]
fn ExtensionMenuRow(
    extension: Rc<ExtRow>,
    anchor: Signal<Option<Rc<MountedData>>>,
    on_close: EventHandler<()>,
) -> Element {
    let action_id = extension.id.clone();
    let pin_id = extension.id.clone();
    let pinned = extension.pinned;

    rsx! {
        div { class: "group flex min-w-0 items-center gap-1 rounded-lg hover:bg-foreground/[0.06]",
            button {
                r#type: "button",
                class: "flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left",
                title: "{extension.name}",
                onclick: move |_| {
                    on_close.call(());
                    ExtensionActionState {
                        id: action_id.clone(),
                        mounted: anchor,
                    }
                    .open();
                },
                if let Some(icon) = extension.icon.as_ref() {
                    img { class: "size-4 shrink-0 rounded object-contain", src: "{icon}", alt: "" }
                } else {
                    Icon { class: "size-4 shrink-0 text-muted-foreground",
                        path { d: "M20.5 11H19V7c0-1.1-.9-2-2-2h-4V3.5C13 2.12 11.88 1 10.5 1S8 2.12 8 3.5V5H4c-1.1 0-1.99.9-1.99 2v3.8H3.5c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7H2V20c0 1.1.9 2 2 2h3.8v-1.5c0-1.49 1.21-2.7 2.7-2.7 1.49 0 2.7 1.21 2.7 2.7V22H17c1.1 0 2-.9 2-2v-4h1.5c1.38 0 2.5-1.12 2.5-2.5S21.88 11 20.5 11z" }
                    }
                }
                span { class: "min-w-0 flex-1 truncate text-xs font-medium text-foreground", "{extension.name}" }
            }
            button {
                r#type: "button",
                class: if pinned {
                    "flex size-7 shrink-0 items-center justify-center rounded-md text-primary hover:bg-primary/10"
                } else {
                    "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-60 hover:bg-foreground/[0.08] hover:text-foreground group-hover:opacity-100"
                },
                title: if pinned { translate("layout-unpin") } else { translate("layout-pin") },
                aria_label: if pinned { translate("layout-unpin") } else { translate("layout-pin") },
                onclick: move |event| {
                    event.stop_propagation();
                    let _ = send(&ExtPinRequest {
                        id: pin_id.clone(),
                        pinned: !pinned,
                    });
                },
                Icon { class: "size-3.5",
                    path { d: "M12 17v5" }
                    path { d: "M5 17h14" }
                    path { d: "M6 3h12" }
                    path {
                        d: "M8 3v5a6 6 0 0 1-2 4v1h12v-1a6 6 0 0 1-2-4V3",
                        fill: if pinned { "currentColor" } else { "none" },
                    }
                }
            }
        }
    }
}
