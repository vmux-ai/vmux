use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "page", name = "context_request", target = "git")]
pub struct PageContextRequest {}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "page", name = "context", target = "git")]
pub struct PageContextEvent {
    pub working_directory: String,
    pub page_url: String,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(namespace = "tab", name = "workspace_request", target = "git")]
pub struct TabWorkspaceRequest {
    pub path: String,
    pub branch: String,
    pub checkout: String,
    pub pane_id: String,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(namespace = "tab", name = "workspace", target = "git")]
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
