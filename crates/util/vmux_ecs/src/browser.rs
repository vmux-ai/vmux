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
pub struct BrowserNavigationSnapshotResponse {
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

#[derive(Message, Clone)]
pub struct BrowserScrollResponse {
    pub request_id: [u8; 16],
    pub result: Result<String, String>,
}
#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct BrowserOpenRequest {
    pub source: bevy_ecs::prelude::Entity,
    pub url: String,
}
