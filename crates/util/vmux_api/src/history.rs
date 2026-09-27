#[vmux_api::contract]
pub struct HistoryEntry {
    pub url_entity_bits: u64,
    pub url: String,
    pub title: String,
    pub favicon_url: String,
    pub visit_created_at: i64,
    pub visit_count: u32,
    pub last_visited_at: i64,
}

#[vmux_api::ui_event]
pub struct HistoryQueryRequest {
    pub query: String,
}

#[vmux_api::ui_event(Copy, Default)]
pub struct HistoryLoadMoreRequest {
    pub loaded: u32,
}

#[vmux_api::ui_event]
pub struct HistoryDeleteRequest {
    pub url_entity_bits: u64,
}

#[vmux_api::ui_event]
pub struct HistoryClearAllRequest;

#[vmux_api::ui_event]
pub struct HistoryOpenRequest {
    pub url: String,
    pub in_new_stack: bool,
}

#[vmux_api::ui_event]
pub struct HistorySuggestionsRequest {
    pub query: String,
    pub limit: u32,
    pub request_id: u64,
}

#[vmux_api::contract]
pub struct HistorySuggestionsResponse {
    pub request_id: u64,
    pub entries: Vec<HistoryEntry>,
}
