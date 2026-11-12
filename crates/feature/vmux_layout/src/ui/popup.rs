use std::rc::Rc;

use dioxus::prelude::*;
use vmux_ui::hooks::send;

use crate::event::LayoutOverlayEvent;

#[component]
pub(super) fn NativePopupSurface(
    id: String,
    right: i32,
    top: i32,
    width: f32,
    height: f32,
    onbounds: EventHandler<(f32, f32, f32, f32)>,
    ondismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let reporter = BoundsReporter {
        region: use_signal(|| None),
        onbounds,
    };
    let overlay_id = id.clone();
    use_effect(use_reactive!(|(overlay_id)| {
        let _ = send(&LayoutOverlayEvent {
            id: overlay_id.clone(),
            active: true,
            ..Default::default()
        });
    }));
    use_effect(use_reactive!(|(right, top, width, height)| {
        let _ = (right, top, width, height);
        reporter.publish();
    }));
    use_drop(move || {
        let _ = send(&LayoutOverlayEvent {
            id,
            active: false,
            ..Default::default()
        });
        ondismiss.call(());
    });
    let style = format!(
        "right:max(8px,calc(100vw - {right}px));top:{top}px;width:min({width:.0}px,calc(100vw - 16px));height:min({height:.0}px,calc(100vh - {}px));",
        top + 8
    );
    rsx! {
        div { class: "pointer-events-auto fixed inset-0 z-[1000]", tabindex: "-1",
            onmounted: move |event| { let target = event.data(); spawn(async move { let _ = target.set_focus(true).await; }); },
            onkeydown: move |event| { if event.key() == Key::Escape { event.prevent_default(); ondismiss.call(()); } },
            onpointerdown: move |_| ondismiss.call(()),
            div { class: "absolute flex flex-col overflow-hidden rounded-2xl", style,
                onpointerdown: move |event| event.stop_propagation(),
                {children}
                div { class: "pointer-events-none min-h-0 flex-1",
                    onmounted: move |event| reporter.mount(event.data()),
                    onresize: move |_| reporter.publish(),
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BoundsReporter {
    region: Signal<Option<Rc<MountedData>>>,
    onbounds: EventHandler<(f32, f32, f32, f32)>,
}

impl BoundsReporter {
    fn mount(mut self, node: Rc<MountedData>) {
        self.region.set(Some(node));
        self.publish();
    }

    fn publish(self) {
        spawn(async move {
            let Some(node) = self.region.peek().clone() else {
                return;
            };
            let Ok(rect) = node.get_client_rect().await else {
                return;
            };
            self.onbounds.call((
                rect.origin.x as f32,
                rect.origin.y as f32,
                rect.size.width as f32,
                rect.size.height as f32,
            ));
        });
    }
}
