use dioxus::prelude::*;

#[component]
pub(super) fn PopupLayer(
    #[props(default)] label: String,
    on_close: EventHandler<()>,
    #[props(default)] on_resize: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "pointer-events-auto fixed inset-0 z-[1000]",
            role: (!label.is_empty()).then_some("dialog"),
            aria_label: (!label.is_empty()).then_some(label.clone()),
            tabindex: "-1",
            onmounted: move |event: MountedEvent| { let node = event.data(); spawn(async move { let _ = node.set_focus(true).await; }); },
            onkeydown: move |event| { if event.key() == Key::Escape { event.prevent_default(); on_close.call(()); } },
            onpointerdown: move |_| on_close.call(()),
            onresize: move |_: ResizeEvent| { if let Some(handler) = on_resize { handler.call(()); } },
            {children}
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct PopupPlacement {
    pub(super) right: i32,
    pub(super) top: i32,
}

impl PopupPlacement {
    pub(super) fn new(right: i32, bottom: i32) -> Self {
        Self {
            right: right.max(48),
            top: bottom.max(40) + 8,
        }
    }

    pub(super) fn style(self, width: f32, height: f32) -> String {
        format!(
            "right:max(8px,calc(100vw - {}px));top:{}px;width:min({:.0}px,calc(100vw - 16px));height:min({:.0}px,calc(100vh - {}px));",
            self.right,
            self.top,
            width.clamp(200.0, 360.0),
            height.max(80.0),
            self.top + 8
        )
    }
}
