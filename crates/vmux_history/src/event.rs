pub const HISTORY_QUERY_RESPONSE_EVENT: &str = "history-query-response";
pub const HISTORY_SUGGESTIONS_RESPONSE_EVENT: &str = "history-suggestions-response";

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryEntry {
    pub url_entity_bits: u64,
    pub url: String,
    pub title: String,
    pub favicon_url: String,
    pub visit_created_at: i64,
    pub visit_count: u32,
    pub last_visited_at: i64,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryQueryRequest {
    pub query: Option<String>,
    pub offset: u32,
    pub limit: u32,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryQueryResponse {
    pub request_id: u64,
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryDeleteRequest {
    pub url_entity_bits: u64,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryClearAllRequest;

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistoryOpenRequest {
    pub url: String,
    pub in_new_stack: bool,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistorySuggestionsRequest {
    pub query: String,
    pub limit: u32,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct HistorySuggestionsResponse {
    pub request_id: u64,
    pub entries: Vec<HistoryEntry>,
}
