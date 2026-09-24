pub const SPACES_PAGE_URL: &str = "vmux://spaces/";
pub const PROJECTS_PAGE_URL: &str = "vmux://projects/";

#[vmux_api::contract(Copy, Eq)]
pub enum SpaceKey {
    Next,
    Previous,
    Attach,
    Delete,
}

#[vmux_api::contract(Default, Eq)]
pub struct SpacesListEvent {
    pub spaces: Vec<SpaceRow>,
}

#[vmux_api::ui_state_patch]
pub enum SpacesUiStatePatch {
    Snapshot(Box<SpacesListEvent>),
    Key(SpaceKey),
}

#[vmux_api::ui_state(Default, target = "spaces")]
pub struct SpacesUiState {
    pub sequence: u64,
    pub patches: Vec<SpacesUiStatePatch>,
}

#[vmux_api::contract(Default, Eq)]
pub struct SpaceRow {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
    pub tab_count: u32,
    pub startup_dir: String,
}

#[vmux_api::ui_event(Eq, targets = ["spaces", "layout"])]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::message::Message))]
pub enum SpaceRequest {
    OpenPage,
    Attach { space_id: String },
    Delete { space_id: String },
    Rename { space_id: String, name: String },
    Create { name: String },
}

#[vmux_api::ui_event(Eq, targets = ["spaces", "layout", "git"])]
pub struct ProjectActivateRequest {
    pub path: String,
    pub branch: String,
    pub checkout: String,
    pub pane_id: Option<u64>,
}

#[vmux_api::ui_event(Eq, target = "spaces")]
pub struct ProjectForgetRequest {
    pub path: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct ProjectRow {
    pub path: String,
    pub label: String,
    pub display_path: String,
    pub depth: u32,
    pub is_active: bool,
    pub is_worktree: bool,
    pub missing: bool,
    pub branch: String,
    #[serde(default)]
    pub kind: ProjectRowKind,
    #[serde(default)]
    pub expanded: bool,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum ProjectRowKind {
    #[default]
    Project,
    Directory,
    File,
}

impl ProjectRowKind {
    pub fn opens_a_tree(self) -> bool {
        matches!(self, Self::Project | Self::Directory)
    }

    pub fn carries_a_branch(self) -> bool {
        matches!(self, Self::Project)
    }
}

#[vmux_api::ui_event(Default, Eq, target = "layout")]
pub struct ProjectTreeToggle {
    pub path: String,
    #[serde(default)]
    pub pane_id: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct ProjectBranch {
    pub branch: String,
    pub checkout: String,
    pub label: String,
    pub insertions: u32,
    pub deletions: u32,
}

impl ProjectBranch {
    pub fn held(&self) -> bool {
        !self.checkout.is_empty()
    }

    pub fn changed(&self) -> bool {
        self.insertions > 0 || self.deletions > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_row_keeps_profile_and_active_state() {
        let row = SpaceRow {
            id: "work".to_string(),
            name: "Work".to_string(),
            profile: "Personal".to_string(),
            is_active: true,
            tab_count: 3,
            startup_dir: "~/work".to_string(),
        };
        assert_eq!(row.profile, "Personal");
        assert!(row.is_active);
        assert_eq!(row.tab_count, 3);
    }

    #[test]
    fn attach_event_carries_target_space_id() {
        let event = SpaceRequest::Attach {
            space_id: "work".to_string(),
        };
        assert_eq!(
            event,
            SpaceRequest::Attach {
                space_id: "work".to_string()
            }
        );
    }

    #[test]
    fn space_request_rkyv_roundtrip() {
        let original = SpaceRequest::Rename {
            space_id: "work".to_string(),
            name: "Work".to_string(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<SpaceRequest, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn spaces_list_event_rkyv_roundtrip() {
        let original = SpacesListEvent {
            spaces: vec![SpaceRow {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "Personal".to_string(),
                is_active: true,
                tab_count: 2,
                startup_dir: "~/work".to_string(),
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<SpacesListEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
