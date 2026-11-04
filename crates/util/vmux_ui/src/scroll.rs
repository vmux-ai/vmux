use crate::platform::Platform;
use dioxus::core::Task;
use dioxus::html::ScrollBehavior;
use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::{
    MountedData, ReadableExt, ScrollData, Signal, WritableExt, spawn, use_signal,
};
use std::rc::Rc;

#[derive(Clone, PartialEq)]
pub struct Target {
    pub target: String,
    pub revision: u64,
    pub behavior: ScrollBehavior,
}

impl Default for Target {
    fn default() -> Self {
        Self {
            target: String::new(),
            revision: 0,
            behavior: ScrollBehavior::Instant,
        }
    }
}

impl Target {
    pub fn request(&mut self, target: String, behavior: ScrollBehavior) {
        self.target = target;
        self.revision = self.revision.wrapping_add(1).max(1);
        self.behavior = behavior;
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SectionNavigation {
    pub active: Signal<String>,
    pub reveal: Signal<Target>,
    cursor: Signal<LocationCursor>,
}

impl SectionNavigation {
    pub fn use_state() -> Self {
        Self::with_active(use_signal(String::new))
    }

    pub fn with_active(active: Signal<String>) -> Self {
        Self {
            active,
            reveal: use_signal(Target::default),
            cursor: use_signal(LocationCursor::default),
        }
    }

    pub fn select(mut self, target: String, fragment: String) {
        self.active.set(target.clone());
        self.reveal.write().request(target, ScrollBehavior::Smooth);
        self.publish(fragment);
    }

    pub fn follow(mut self, target: String, fragment: String) {
        if *self.active.peek() != target {
            self.active.set(target);
        }
        self.publish(fragment);
    }

    pub fn receive(mut self, fragment: &str, target: String) -> bool {
        if !self.cursor.write().receive(fragment) {
            return false;
        }
        self.active.set(target.clone());
        self.reveal.write().request(target, ScrollBehavior::Instant);
        true
    }

    pub fn reset(mut self) {
        self.active.set(String::new());
        self.reveal.set(Target::default());
        self.cursor.set(LocationCursor::default());
    }

    fn publish(mut self, fragment: String) {
        if !self.cursor.write().publish(&fragment) {
            return;
        }
        crate::hooks::send(&vmux_api::page_location::PageLocationRequest { fragment });
    }
}

#[derive(Clone, Default, PartialEq)]
struct LocationCursor {
    current: String,
    received: Option<String>,
    pending: std::collections::VecDeque<String>,
}

impl LocationCursor {
    fn publish(&mut self, fragment: &str) -> bool {
        if self.current == fragment {
            return false;
        }
        self.current = fragment.to_string();
        self.pending.push_back(fragment.to_string());
        while self.pending.len() > 64 {
            self.pending.pop_front();
        }
        true
    }

    fn receive(&mut self, fragment: &str) -> bool {
        if self.received.as_deref() == Some(fragment) {
            return false;
        }
        self.received = Some(fragment.to_string());
        if let Some(index) = self.pending.iter().position(|pending| pending == fragment) {
            self.pending.drain(..=index);
            return false;
        }
        if self.current == fragment {
            return false;
        }
        self.pending.clear();
        self.current = fragment.to_string();
        true
    }
}

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
    pub async fn section_at_top(container: &str) -> Option<u32> {
        crate::transport::Host::section_at_top(container).await
    }

    pub fn section(container: &str, element: &str, behavior: ScrollBehavior) {
        crate::transport::Host::scroll_section(container, element, behavior);
    }
    pub fn nearest(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view(element_id)
    }

    pub fn nearest_vertical(element_id: &str) -> bool {
        crate::transport::Host::scroll_item_into_view_vertical(element_id)
    }

    pub fn element_to(element_id: &str, top: f64) {
        crate::transport::Host::scroll_element_to(element_id, top.max(0.0));
    }

    pub fn element_to_smooth(element_id: &str, top: f64) {
        crate::transport::Host::scroll_element_to_smooth(element_id, top.max(0.0));
    }

    pub fn first_rendered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, false);
    }

    pub fn first_rendered_centered(element_ids: &[&str]) {
        crate::transport::Host::reveal_first_rendered(element_ids, true);
    }
}

pub struct ScrollRetention;

impl ScrollRetention {
    pub fn preserve_next_render() {
        crate::transport::Host::preserve_scroll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_scroll_updates_do_not_replay_as_navigation_when_the_host_echoes_them() {
        let mut cursor = LocationCursor::default();
        assert!(!cursor.receive(""));
        assert!(cursor.publish("L10"));
        assert!(cursor.publish("L20"));
        assert!(!cursor.receive("L10"));
        assert!(!cursor.receive("L20"));
        assert!(cursor.receive("L10"));
        assert!(!cursor.receive("L10"));
    }

    #[test]
    fn coalesced_location_updates_acknowledge_the_earlier_scroll_sections() {
        let mut cursor = LocationCursor::default();
        assert!(cursor.publish("L10"));
        assert!(cursor.publish("L20"));
        assert!(!cursor.receive("L20"));
        assert!(cursor.receive("L10"));
        assert!(!cursor.publish("L10"));
    }
}
