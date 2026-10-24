use dioxus::prelude::*;
use std::rc::Rc;
use vmux_ui::platform::Platform;

pub use imp::{settle_bottom, to_bottom};

pub type Container = Signal<Option<Rc<MountedData>>>;

mod imp {
    use super::Container;
    use super::Platform;
    use dioxus::core::Task;
    use dioxus::html::ScrollBehavior;
    use dioxus::html::geometry::PixelsVector2D;
    use dioxus::prelude::{ReadableExt, Signal, WritableExt, spawn};

    pub fn to_bottom(container: Container) {
        let Some(element) = container() else {
            return;
        };
        spawn(async move {
            for _ in 0..2 {
                let Ok(size) = element.get_scroll_size().await else {
                    return;
                };
                let _ = element
                    .scroll(
                        PixelsVector2D::new(0.0, size.height),
                        ScrollBehavior::Instant,
                    )
                    .await;
            }
        });
    }

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
            following.set(false);
            pending.set(None);
        })));
    }
}
