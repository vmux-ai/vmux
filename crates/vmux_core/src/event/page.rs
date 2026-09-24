#[vmux_api::ui_event(Default, Eq, target = "git")]
pub struct PageContextRequest {}

#[vmux_api::ui_event(Default, Eq, target = "git")]
pub struct TabWorkspaceRequest {
    pub path: String,
    pub branch: String,
    pub checkout: String,
    pub pane_id: String,
}

#[cfg(host)]
#[derive(bevy::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct RecordVisitRequest {
    pub url: String,
    pub title: String,
}
