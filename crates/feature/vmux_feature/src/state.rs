#[vmux_api::contract(Default, Eq)]
pub struct FlagRow {
    pub key: String,
    pub published: Option<bool>,
    pub proposed: bool,
    pub default: Option<bool>,
    pub source: String,
    pub changed: bool,
    pub configuration: String,
    pub published_configuration: Option<String>,
    pub value_type: String,
    pub default_value: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct ChangeRow {
    pub key: String,
    pub before: Option<String>,
    pub after: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct HistoryRow {
    pub revision: u64,
    pub actor: String,
    pub created_at: u64,
    pub changes: Vec<ChangeRow>,
}

#[vmux_api::contract(Default, Eq)]
pub struct InspectionRow {
    pub key: String,
    pub scope: String,
    pub scope_kind: String,
    pub context_generation: u64,
    pub value_type: String,
    pub class: String,
    pub owner: String,
    pub expiry: String,
    pub lifecycle: String,
    pub default: String,
    pub effective: String,
    pub source: String,
    pub reason: String,
    pub variant: String,
    pub snapshot_revision: u64,
    pub valid_until: String,
    pub snapshot_age: u64,
    pub retained_revision: u64,
    pub local_override: String,
    pub context: Vec<ContextRow>,
}

#[vmux_api::contract(Default, Eq)]
pub struct ContextRow {
    pub attribute: String,
    pub value: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct AssignmentRow {
    pub key: String,
    pub generation: String,
    pub subjects: u64,
    pub active: bool,
}

#[vmux_api::ui_state(Default, Eq, version = 2)]
pub struct FeaturesUiState {
    pub assignments: Vec<AssignmentRow>,
    pub inspection: Vec<InspectionRow>,
    pub can_override: bool,
    pub override_error: String,
    pub environment: String,
    pub environments: Vec<String>,
    pub loaded: bool,
    pub busy: bool,
    pub can_edit: bool,
    pub can_review: bool,
    pub can_publish: bool,
    pub can_rollback: bool,
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

#[vmux_api::ui_event(Default, Eq)]
pub struct FeatureOverrideRequest {
    pub key: String,
    pub environment: String,
    pub scope: String,
    pub context_generation: u64,
    pub value: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct FeatureOverrideClearRequest {
    pub key: String,
    pub environment: String,
    pub scope: String,
    pub context_generation: u64,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct FeatureAssignmentRetireRequest {
    pub key: String,
    pub environment: String,
    pub generation: String,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct DraftConfigurationEditRequest {
    pub key: String,
    pub configuration: String,
}
