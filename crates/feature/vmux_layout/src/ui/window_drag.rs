#![allow(non_snake_case)]

use std::rc::Rc;

use dioxus::prelude::*;
use vmux_ui::hooks::send;

use crate::event::WindowDragRegionEvent;

#[component]
pub(crate) fn WindowDragRegion(
    #[props(into)] id: String,
    revision: String,
    #[props(default)] blocked: bool,
    class: &'static str,
    style: &'static str,
) -> Element {
    let reporter = WindowDragReporter {
        id,
        blocked,
        region: use_signal(|| None::<Rc<MountedData>>),
    };
    let dropped = reporter.clone();
    use_drop(move || dropped.remove());
    let revised = reporter.clone();
    use_effect(use_reactive!(|revision| {
        let _ = revision;
        revised.publish();
    }));
    let mounted = reporter.clone();
    let resized = reporter;

    rsx! {
        div {
            class,
            style,
            onmounted: move |event: Event<MountedData>| {
                mounted.clone().mount(event.data());
            },
            onresize: move |_: Event<ResizeData>| resized.publish(),
        }
    }
}

#[derive(Clone)]
struct WindowDragReporter {
    id: String,
    blocked: bool,
    region: Signal<Option<Rc<MountedData>>>,
}

impl WindowDragReporter {
    fn mount(mut self, region: Rc<MountedData>) {
        self.region.set(Some(region));
        self.publish();
    }

    fn publish(&self) {
        let region = self.region;
        let id = self.id.clone();
        let blocked = self.blocked;
        spawn(async move {
            let Some(region) = region() else {
                return;
            };
            let Ok(rect) = region.get_client_rect().await else {
                return;
            };
            let _ = send(&WindowDragRegionEvent {
                id,
                removed: false,
                blocked,
                left: rect.origin.x as f32,
                top: rect.origin.y as f32,
                width: rect.size.width as f32,
                height: rect.size.height as f32,
            });
        });
    }

    fn remove(&self) {
        let _ = send(&WindowDragRegionEvent {
            id: self.id.clone(),
            removed: true,
            ..Default::default()
        });
    }
}
