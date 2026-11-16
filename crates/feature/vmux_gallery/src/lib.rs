#[cfg(host)]
pub use host::GalleryPlugin;

#[cfg(host)]
mod host;
#[cfg(ui)]
pub mod ui;

#[vmux_api::ui_state(Default)]
pub struct GalleryUiState {
    pub story: Story,
    pub disabled: bool,
    pub text: String,
    pub checked: bool,
    pub choice: String,
    pub dialog_open: bool,
    pub progress: f64,
    pub interaction_count: u64,
    pub last_event: String,
    pub revision: u64,
}

#[vmux_api::contract(Default, Copy, Eq)]
pub enum Story {
    #[default]
    Button,
    Input,
    Textarea,
    Checkbox,
    Switch,
    Select,
    Dialog,
    Card,
    Avatar,
    Progress,
    Skeleton,
}

impl Story {
    pub const ALL: [Self; 11] = [
        Self::Button,
        Self::Input,
        Self::Textarea,
        Self::Checkbox,
        Self::Switch,
        Self::Select,
        Self::Dialog,
        Self::Card,
        Self::Avatar,
        Self::Progress,
        Self::Skeleton,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Input => "Input",
            Self::Textarea => "Textarea",
            Self::Checkbox => "Checkbox",
            Self::Switch => "Switch",
            Self::Select => "Select",
            Self::Dialog => "Dialog",
            Self::Card => "Card",
            Self::Avatar => "Avatar",
            Self::Progress => "Progress",
            Self::Skeleton => "Skeleton",
        }
    }
}

#[vmux_api::ui_event]
pub struct GallerySelectRequest {
    pub story: Story,
}

#[vmux_api::ui_event]
pub struct GalleryDisabledRequest {
    pub disabled: bool,
}

#[vmux_api::ui_event]
pub struct GalleryTextRequest {
    pub revision: u64,
    pub text: String,
}

#[vmux_api::ui_event]
pub struct GalleryCheckedRequest {
    pub revision: u64,
    pub checked: bool,
}

#[vmux_api::ui_event]
pub struct GalleryChoiceRequest {
    pub revision: u64,
    pub choice: String,
}

#[vmux_api::ui_event]
pub struct GalleryDialogRequest {
    pub revision: u64,
    pub open: bool,
}

#[vmux_api::ui_event]
pub struct GalleryProgressRequest {
    pub revision: u64,
    pub value: f64,
}

#[vmux_api::ui_event]
pub struct GalleryClickRequest {
    pub revision: u64,
    pub label: String,
}

#[vmux_api::ui_event]
pub struct GalleryResetRequest;
