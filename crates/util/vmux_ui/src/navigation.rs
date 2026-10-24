use crate::hooks::send;
use vmux_api::command_bar::OpenRequest;
use vmux_api::open_target::OpenTarget;

pub struct PageNavigation;

impl PageNavigation {
    pub fn replace(url: impl Into<String>) {
        let _ = send(&OpenRequest {
            value: url.into(),
            open: Some(OpenTarget::InPlace),
        });
    }

    pub fn stack(url: impl Into<String>) {
        let _ = send(&OpenRequest {
            value: url.into(),
            open: Some(OpenTarget::InNewStack),
        });
    }
}
