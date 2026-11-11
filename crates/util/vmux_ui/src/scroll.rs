use dioxus::html::ScrollBehavior;
use dioxus::prelude::{ReadableExt, ScrollData, Signal, WritableExt, use_signal};

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
        let _ = crate::hooks::send(&vmux_api::page_location::PageLocationRequest { fragment });
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

pub struct StickySections;

impl StickySections {
    pub fn install() {
        crate::transport::Host::run_script(Self::runtime().to_string());
    }

    pub fn runtime() -> &'static str {
        include_str!("sticky_sections.js")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct StickyProbe;

    impl StickyProbe {
        fn run() {
            let mut child = Command::new("node")
                .args(["-e", include_str!("sticky_sections_test.js")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for sticky header behavior tests");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(StickySections::runtime().as_bytes())
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn sticky_headers_follow_section_boundaries_and_release_detached_observers() {
        StickyProbe::run();
    }

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
