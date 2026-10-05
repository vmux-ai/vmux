use dioxus::prelude::*;
use std::rc::Rc;

pub use imp::to_bottom;

pub type Container = Signal<Option<Rc<MountedData>>>;

mod imp {
    use super::Container;

    pub fn to_bottom(_container: Container) {
        vmux_ui::scroll::ScrollIntoView::element_to("chat-scroll", i32::MAX as f64);
    }
}
