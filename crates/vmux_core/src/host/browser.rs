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

#[derive(Resource, Default)]
pub struct NavAwaitingSnapshot(pub HashSet<[u8; 16]>);
