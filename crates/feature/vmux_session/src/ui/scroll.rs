use dioxus::prelude::*;
use std::rc::Rc;

pub use imp::to_bottom;

pub type Container = Signal<Option<Rc<MountedData>>>;

mod imp {
    use super::Container;
    use dioxus::html::ScrollBehavior;
    use dioxus::html::geometry::PixelsVector2D;
    use dioxus::prelude::spawn;

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
}
