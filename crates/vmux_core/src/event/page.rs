use serde::{Deserialize, Serialize};

pub const PAGE_CONTEXT_EVENT: &str = "page_context";
pub const TAB_WORKSPACE_EVENT: &str = "tab_workspace";
pub const AGENT_PROMPT_DRAFT_EVENT: &str = "agent_prompt_draft";
pub const SERVICE_UNAVAILABLE_EVENT: &str = "service_unavailable";

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
