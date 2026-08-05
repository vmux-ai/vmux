//! The request and response vocabulary for driving an embedded page: snapshot its DOM and
//! scroll it. Lives here so the agent can send these without depending on the browser, and
//! the browser can serve them without depending on the agent.

use std::collections::HashSet;

use bevy::prelude::*;

#[derive(Message, Clone)]
pub struct BrowserSnapshotRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
    pub webview: Option<Entity>,
}

#[derive(Message, Clone)]
pub struct BrowserSnapshotResponse {
    pub request_id: [u8; 16],
    pub result: Result<String, String>,
}

#[derive(Message, Clone)]
pub struct BrowserScrollRequest {
    pub request_id: [u8; 16],
    pub pane: Option<String>,
    pub to: Option<String>,
    pub delta: Option<i32>,
}

/// Request ids whose [`BrowserSnapshotResponse`] must be returned as an agent *command*
/// result (a navigation that returns its page snapshot inline) rather than the default
/// *query* result. Populated when a deferred navigation settles.
#[derive(Resource, Default)]
pub struct NavAwaitingSnapshot(pub HashSet<[u8; 16]>);
