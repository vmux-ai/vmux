use dioxus::prelude::*;
use std::rc::Rc;

pub use imp::{metrics, restore, to_bottom};

pub type Container = Signal<Option<Rc<MountedData>>>;

mod imp {
    use super::Container;

    pub fn metrics(_container: Container) -> Option<(i32, i32)> {
        None
    }

    pub fn to_bottom(_container: Container) {
        vmux_ui::scroll::ScrollIntoView::element_to("chat-scroll", i32::MAX as f64);
    }

    pub fn restore(_container: Container, _previous_height: i32, _previous_top: i32) {}
}
