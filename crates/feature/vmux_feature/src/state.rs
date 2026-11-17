#[vmux_api::contract(Default, Eq)]
pub struct FlagRow {
    pub key: String,
    pub published: Option<bool>,
    pub proposed: bool,
    pub default: Option<bool>,
    pub source: String,
    pub changed: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChangeRow {
    pub key: String,
    pub before: Option<bool>,
    pub after: bool,
}

#[vmux_api::contract(Default, Eq)]
pub struct HistoryRow {
    pub revision: u64,
    pub actor: String,
    pub created_at: u64,
    pub changes: Vec<ChangeRow>,
}

#[vmux_api::ui_state(Default, Eq, version = 1)]
pub struct FeaturesUiState {
    pub environment: String,
    pub environments: Vec<String>,
    pub loaded: bool,
    pub busy: bool,
    pub can_edit: bool,
    pub can_review: bool,
    pub can_publish: bool,
    pub revision: u64,
    pub flags: Vec<FlagRow>,
    pub query: String,
    pub selected: String,
    pub tab: String,
    pub draft_id: String,
    pub draft_revision: u64,
    pub base_revision: u64,
    pub reviewed: bool,
    pub unsaved: bool,
    pub history: Vec<HistoryRow>,
    pub error: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct FeaturesOpenRequest {
    pub environment: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct FeaturesFilterRequest {
    pub query: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct FeatureSelectRequest {
    pub key: String,
    pub tab: String,
}

#[vmux_api::ui_event(Eq)]
pub struct DraftEditRequest {
    pub key: String,
    pub enabled: bool,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct DraftSaveRequest {
    pub expected_revision: u64,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct DraftReviewRequest {
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct DraftPublishRequest {
    pub draft_id: String,
    pub expected_revision: u64,
    pub base_revision: u64,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct DraftDiscardRequest {
    pub draft_id: String,
    pub expected_revision: u64,
}

#[vmux_api::ui_event(Eq)]
pub struct FlagRollbackRequest {
    pub revision: u64,
}
