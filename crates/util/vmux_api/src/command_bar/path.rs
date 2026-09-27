#[vmux_api::ui_event(Default, Eq)]
pub struct PathCompleteRequest {
    pub request_id: u64,
    pub query: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
    pub project: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct PathCompleteResponse {
    pub request_id: u64,
    pub completions: Vec<PathEntry>,
    pub truncated: bool,
    pub total: u32,
}
