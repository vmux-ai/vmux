#[vmux_api::ui_event(Default, Eq, target = "git")]
pub struct PageContextRequest {}

#[vmux_api::host_event(Default, Eq, target = "git")]
pub struct PageContextEvent {
    pub working_directory: String,
    pub page_url: String,
}

#[vmux_api::ui_event(Default, Eq, target = "git")]
pub struct TabWorkspaceRequest {
    pub path: String,
    pub branch: String,
    pub checkout: String,
    pub pane_id: String,
}

#[vmux_api::host_event(Default, Eq, target = "git")]
pub struct TabWorkspaceEvent {
    pub path: String,
    pub branch: String,
    pub error: String,
}

#[cfg(host)]
#[derive(bevy::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct RecordVisitRequest {
    pub url: String,
    pub title: String,
}
