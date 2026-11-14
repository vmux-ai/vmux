use crate::event::{LayoutOverlayEvent, LayoutOverlayKind};
use dioxus::prelude::*;
use std::rc::Rc;
use vmux_api::browser::{BrowserModalBoundsRequest, BrowserModalCloseRequest, BrowserModalState};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn BrowserModal(popup: BrowserModalState) -> Element {
    let id = popup.id.clone();
    let region = BrowserModalRegion {
        id: id.clone(),
        node: use_signal(|| None),
    };
    let mut mounted = region.clone();
    let resized = region.clone();
    use_effect(move || region.publish());
    let close = BrowserModalClose { id: id.clone() };
    let keyboard = close.clone();
    let outside = close.clone();
    let overlay = format!("browser-modal-{id}");
    let opened = overlay.clone();
    use_effect(move || {
        let _ = send(&LayoutOverlayEvent {
            id: opened.clone(),
            active: true,
            kind: LayoutOverlayKind::Panel,
        });
    });
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id: overlay,
            active: false,
            kind: LayoutOverlayKind::Panel,
        });
        let _ = send(&BrowserModalCloseRequest { id });
    });
    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0 z-[1000] bg-black/30",
            role: "dialog",
            aria_modal: "true",
            aria_label: popup.title.clone(),
            tabindex: "-1",
            onmounted: move |event: MountedEvent| { let node = event.data(); spawn(async move { let _ = node.set_focus(true).await; }); },
            onkeydown: move |event| { if event.key() == Key::Escape { event.prevent_default(); keyboard.close(); } },
            onpointerdown: move |_| outside.close(),
            div {
                class: "absolute left-1/2 top-1/2 flex w-[min(520px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-[14px] bg-foreground text-background",
                onpointerdown: move |event| event.stop_propagation(),
                header { class: "flex h-12 shrink-0 items-center gap-2 px-3",
                    if let Some(icon) = popup.icon { img { src: icon, alt: "", class: "size-5 shrink-0 object-contain" } }
                    div { class: "min-w-0 flex-1",
                        p { class: "truncate text-sm font-medium", "{popup.title}" }
                        p { class: "truncate text-[11px] opacity-60", "{popup.origin}" }
                    }
                    button { r#type: "button", aria_label: translate("common-close"), class: "flex size-7 items-center justify-center rounded-md opacity-60 hover:bg-current/10 hover:opacity-100", onclick: move |_| close.close(),
                        LineIconView { icon: LineIcon::X, class: "size-4".to_string() }
                    }
                }
                div {
                    class: "pointer-events-none h-[min(640px,calc(100vh-96px))] w-full",
                    onmounted: move |event: MountedEvent| mounted.mount(event.data()),
                    onresize: move |_: ResizeEvent| resized.publish(),
                }
            }
        }
    }
}

#[derive(Clone)]
struct BrowserModalRegion {
    id: String,
    node: Signal<Option<Rc<MountedData>>>,
}

impl BrowserModalRegion {
    fn mount(&mut self, node: Rc<MountedData>) {
        self.node.set(Some(node));
        self.publish();
    }
    fn publish(&self) {
        let node = self.node;
        let id = self.id.clone();
        spawn(async move {
            let Some(node) = node.peek().clone() else {
                return;
            };
            let Ok(rect) = node.get_client_rect().await else {
                return;
            };
            let _ = send(&BrowserModalBoundsRequest {
                id,
                left: rect.origin.x as f32,
                top: rect.origin.y as f32,
                width: rect.size.width as f32,
                height: rect.size.height as f32,
            });
        });
    }
}

#[derive(Clone)]
struct BrowserModalClose {
    id: String,
}

impl BrowserModalClose {
    fn close(&self) {
        let _ = send(&BrowserModalCloseRequest {
            id: self.id.clone(),
        });
    }
}
