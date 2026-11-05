use crate::event::ChatViewport;
use dioxus::prelude::*;
use std::rc::Rc;
use vmux_ui::platform::Platform;

pub use imp::settle_bottom;

pub type Container = Signal<Option<Rc<MountedData>>>;

impl ChatViewport {
    pub(super) fn restore(
        self,
        container: Container,
        mut pending: Signal<Option<dioxus::core::Task>>,
    ) {
        if pending.peek().is_some() {
            return;
        }
        pending.set(Some(spawn(async move {
            let mut previous_height = None;
            for _ in 0..6 {
                Platform::sleep(16).await;
                let Some(element) = container.peek().clone() else {
                    break;
                };
                let Ok(size) = element.get_scroll_size().await else {
                    break;
                };
                let Ok(rect) = element.get_client_rect().await else {
                    break;
                };
                let top = (self.top as f64).min((size.height - rect.size.height).max(0.0));
                let _ = element
                    .scroll(
                        dioxus::html::geometry::PixelsVector2D::new(0.0, top),
                        dioxus::html::ScrollBehavior::Instant,
                    )
                    .await;
                if previous_height.is_some_and(|height: f64| (height - size.height).abs() <= 1.0) {
                    break;
                }
                previous_height = Some(size.height);
            }
            pending.set(None);
        })));
    }
}

pub(super) struct Observation {
    pub top: i32,
    pub previous: i32,
    pub distance: i32,
    pub initial: bool,
    pub settling: bool,
}

impl Observation {
    pub fn pauses_following(&self) -> bool {
        !self.initial && !self.settling && self.distance > 48 && self.top < self.previous
    }
}

mod imp {
    use super::Container;
    use super::Platform;
    use dioxus::core::Task;
    use dioxus::html::ScrollBehavior;
    use dioxus::html::geometry::PixelsVector2D;
    use dioxus::prelude::{ReadableExt, Signal, WritableExt, spawn};

    pub fn settle_bottom(
        container: Container,
        mut following: Signal<bool>,
        mut pending: Signal<Option<Task>>,
    ) {
        if pending.peek().is_some() {
            return;
        }
        pending.set(Some(spawn(async move {
            let mut stable_height = None;
            for _ in 0..6 {
                Platform::sleep(16).await;
                let Some(element) = container.peek().clone() else {
                    pending.set(None);
                    return;
                };
                let Ok(size) = element.get_scroll_size().await else {
                    pending.set(None);
                    return;
                };
                let Ok(rect) = element.get_client_rect().await else {
                    pending.set(None);
                    return;
                };
                let _ = element
                    .scroll(
                        PixelsVector2D::new(0.0, size.height),
                        ScrollBehavior::Instant,
                    )
                    .await;
                Platform::sleep(16).await;
                let Ok(offset) = element.get_scroll_offset().await else {
                    pending.set(None);
                    return;
                };
                let Ok(next_size) = element.get_scroll_size().await else {
                    pending.set(None);
                    return;
                };
                let distance = next_size.height - offset.y - rect.size.height;
                let stable = stable_height
                    .is_some_and(|height: f64| (height - next_size.height).abs() <= 1.0);
                if distance <= 2.0 && stable {
                    following.set(false);
                    pending.set(None);
                    return;
                }
                stable_height = Some(next_size.height);
            }
            pending.set(None);
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinking_content_at_the_bottom_does_not_cancel_following() {
        for (initial, settling) in [(true, true), (true, false), (false, true), (false, false)] {
            let observed = Observation {
                top: 8020,
                previous: 8193,
                distance: 0,
                initial,
                settling,
            };
            assert!(!observed.pauses_following());
        }
    }

    #[test]
    fn an_initial_or_pending_scroll_does_not_become_a_manual_upward_scroll() {
        for (initial, settling) in [(true, true), (true, false), (false, true)] {
            let observed = Observation {
                top: 8000,
                previous: 8193,
                distance: 400,
                initial,
                settling,
            };
            assert!(!observed.pauses_following());
        }
        let manual = Observation {
            top: 8000,
            previous: 8193,
            distance: 400,
            initial: false,
            settling: false,
        };
        assert!(manual.pauses_following());
    }
}
