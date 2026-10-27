use crate::hooks::send;
use dioxus::prelude::{ModifiersInteraction, MouseEvent};
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;

pub struct PageNavigation;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct NavigationGesture {
    meta: bool,
    shift: bool,
}

impl NavigationGesture {
    pub fn from_mouse(event: &MouseEvent) -> Self {
        let modifiers = event.modifiers();
        Self {
            meta: modifiers.meta(),
            shift: modifiers.shift(),
        }
    }

    pub fn open_target(self) -> OpenTarget {
        if self.meta && self.shift {
            OpenTarget::InNewTab
        } else if self.meta {
            OpenTarget::InNewStack
        } else {
            OpenTarget::InPlace
        }
    }
}

impl PageNavigation {
    pub fn click(url: impl Into<String>, event: &MouseEvent) {
        Self::gesture(url, NavigationGesture::from_mouse(event));
    }

    pub fn gesture(url: impl Into<String>, gesture: NavigationGesture) {
        Self::open(url, gesture.open_target());
    }

    pub fn replace(url: impl Into<String>) {
        Self::open(url, OpenTarget::InPlace);
    }

    pub fn stack(url: impl Into<String>) {
        Self::open(url, OpenTarget::InNewStack);
    }

    pub fn tab(url: impl Into<String>) {
        Self::open(url, OpenTarget::InNewTab);
    }

    fn open(url: impl Into<String>, open: OpenTarget) {
        let _ = send(&OpenRequest {
            value: url.into(),
            open: Some(open),
        });
    }
}
