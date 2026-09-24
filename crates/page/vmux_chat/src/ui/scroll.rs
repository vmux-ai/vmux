use dioxus::prelude::*;
use std::rc::Rc;

pub type Container = Signal<Option<Rc<MountedData>>>;

mod imp {
    use super::Container;
    use dioxus::html::geometry::PixelsVector2D;
    use dioxus::prelude::*;

    pub fn metrics(_container: Container) -> Option<(i32, i32)> {
        None
    }

    pub fn to_bottom(container: Container) {
        spawn(async move {
            let Some(element) = container.peek().clone() else {
                return;
            };
            let Ok(size) = element.get_scroll_size().await else {
                return;
            };
            let _ = element
                .scroll(
                    PixelsVector2D::new(0.0, size.height),
                    ScrollBehavior::Instant,
                )
                .await;
        });
    }

    pub fn restore(_container: Container, _previous_height: i32, _previous_top: i32) {}
}

pub use imp::{metrics, restore, to_bottom};
