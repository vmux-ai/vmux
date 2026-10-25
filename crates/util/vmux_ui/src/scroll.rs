use crate::platform::Platform;
use dioxus::core::Task;
use dioxus::html::ScrollBehavior;
use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::{MountedData, ScrollData, spawn};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub struct ScrollPreload {
    viewports: i32,
    minimum: i32,
}

impl ScrollPreload {
    pub const LIST: Self = Self {
        viewports: 4,
        minimum: 960,
    };
    pub const DOCUMENT: Self = Self {
        viewports: 16,
        minimum: 4800,
    };
    pub const HISTORY: Self = Self {
        viewports: 3,
        minimum: 1200,
    };

    pub fn reached(self, event: &ScrollData) -> bool {
        let remaining = event.scroll_height() - event.scroll_top() as i32 - event.client_height();
        remaining <= self.distance(event)
    }

    pub fn reached_start(self, event: &ScrollData) -> bool {
        event.scroll_top() <= f64::from(self.distance(event))
    }

    fn distance(self, event: &ScrollData) -> i32 {
        event
            .client_height()
            .saturating_mul(self.viewports)
            .max(self.minimum)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrependScrollAnchor {
    top: f64,
    height: f64,
}

impl PrependScrollAnchor {
    pub fn capture(event: &ScrollData) -> Self {
        Self {
            top: event.scroll_top(),
            height: f64::from(event.scroll_height()),
        }
    }

    pub fn restore(self, element: Rc<MountedData>) -> Task {
        spawn(async move {
            let mut previous_height = self.height;
            let mut stable_frames = 0;
            let mut changed = false;
            for _ in 0..8 {
                Platform::sleep(16).await;
                let Ok(size) = element.get_scroll_size().await else {
                    return;
                };
                changed |= (size.height - self.height).abs() > 1.0;
                let top = (self.top + size.height - self.height).max(0.0);
                let _ = element
                    .scroll(PixelsVector2D::new(0.0, top), ScrollBehavior::Instant)
                    .await;
                if changed && (size.height - previous_height).abs() <= 1.0 {
                    stable_frames += 1;
                    if stable_frames >= 2 {
                        return;
                    }
                } else {
                    stable_frames = 0;
                }
                previous_height = size.height;
            }
        })
    }
}

pub struct ScrollIntoView;

impl ScrollIntoView {
    pub fn nearest(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view(element_id)
    }

    pub fn nearest_vertical(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view_vertical(element_id)
    }

    pub fn element_to(element_id: &str, top: f64) {
        crate::transport::Host::scroll_element_to(element_id, top.max(0.0));
    }
    pub fn first_rendered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, false);
    }

    pub fn first_rendered_centered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, true);
    }
}
