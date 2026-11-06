use crate::event::ChatViewport;
use dioxus::prelude::*;
use std::rc::Rc;
use vmux_ui::platform::Platform;

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
