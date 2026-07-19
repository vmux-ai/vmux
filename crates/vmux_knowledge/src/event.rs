pub const NOTES_PAGE_URL: &str = "vmux://notes/";
pub const NOTES_QUERY_RESPONSE_EVENT: &str = "notes-query-response";
pub const NOTE_READ_RESPONSE_EVENT: &str = "note-read-response";
pub const NOTE_CREATED_EVENT: &str = "note-created";
pub const NOTE_WRITTEN_EVENT: &str = "note-written";
pub const NOTE_ERROR_EVENT: &str = "note-error";

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum NoteOperation {
    #[default]
    Query,
    Read,
    Create,
    Write,
    Open,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteSummary {
    pub path: String,
    pub relative_path: String,
    pub title: String,
    pub excerpt: String,
    pub modified_at: i64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NotesQueryRequest {
    pub query: String,
    pub request_id: u64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NotesQueryResponse {
    pub request_id: u64,
    pub offset: u32,
    pub vault_path: String,
    pub notes: Vec<NoteSummary>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteReadRequest {
    pub path: String,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteReadResponse {
    pub request_id: u64,
    pub path: String,
    pub relative_path: String,
    pub title: String,
    pub source: String,
    pub html: String,
    pub modified_at: i64,
    pub word_count: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteWriteRequest {
    pub path: String,
    pub source: String,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteWrittenEvent {
    pub request_id: u64,
    pub note: NoteReadResponse,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteCreateRequest {
    pub title: String,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteCreatedEvent {
    pub request_id: u64,
    pub note: NoteSummary,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteOpenRequest {
    pub path: String,
    pub request_id: u64,
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct NoteErrorEvent {
    pub operation: NoteOperation,
    pub request_id: u64,
    pub path: String,
    pub message: String,
}
