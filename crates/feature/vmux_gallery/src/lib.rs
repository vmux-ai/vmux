#[cfg(host)]
pub use host::GalleryPlugin;

#[cfg(host)]
mod host;
#[cfg(ui)]
pub mod ui;

#[vmux_api::ui_state(Default)]
pub struct GalleryUiState {
    pub address: vmux_api::address::PageAddress,
    pub examples: Examples,
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
    pub menu: String,
    pub cursor: usize,
    pub agent: String,
    pub model: String,
    pub effort: String,
    pub permission: String,
    pub project: String,
    pub branch: String,
    pub running: bool,
    pub has_attachment: bool,
    pub status: String,
    pub submitted: String,
}

#[vmux_api::contract(Default)]
pub struct Examples {
    pub files: Vec<vmux_ecs::event::FileDirEntry>,
    pub agents: Vec<vmux_api::command_bar::CommandPaletteAgent>,
    pub models: Vec<vmux_api::session::ModelOptionEntry>,
    pub modes: Vec<vmux_api::protocol::AcpModeOption>,
    pub projects: Vec<vmux_api::space::ProjectRow>,
    pub branches: Vec<vmux_api::space::ProjectBranch>,
    pub media: Vec<vmux_api::prompt_media::PromptMediaOption>,
    pub attachments: Vec<vmux_api::prompt_media::PromptComposerAttachment>,
    pub mcp: vmux_api::mcp::McpServersUiState,
}

#[vmux_api::contract(Default, Copy, Eq)]
pub enum Story {
    Favicon,
    TypeIcon,
    PromptGhost,
    DirectoryNavigator,
    MatrixRain,
    BackButton,
    Address,
    AgentMenu,
    AlertDialog,
    Avatar,
    Badge,
    Breadcrumbs,
    #[default]
    Button,
    Card,
    Checkbox,
    ShellCode,
    PromptComposer,
    ComposerBar,
    ContextMenu,
    Dialog,
    Dropdown,
    EffortMenu,
    ErrorState,
    Filter,
    Icon,
    InlineEdit,
    Input,
    Manager,
    McpMenu,
    ModelMenu,
    Motion,
    Notice,
    Page,
    PageAddress,
    PermissionMenu,
    Progress,
    ProjectPicker,
    PromptBox,
    PromptMediaOptions,
    SegmentedControl,
    Select,
    Sheet,
    Skeleton,
    StartHero,
    Switch,
    Textarea,
    Tooltip,
    FileTree,
}

impl Story {
    pub const ALL: [Self; 48] = [
        Self::Favicon,
        Self::TypeIcon,
        Self::PromptGhost,
        Self::DirectoryNavigator,
        Self::MatrixRain,
        Self::BackButton,
        Self::Address,
        Self::AgentMenu,
        Self::AlertDialog,
        Self::Avatar,
        Self::Badge,
        Self::Breadcrumbs,
        Self::Button,
        Self::Card,
        Self::Checkbox,
        Self::ShellCode,
        Self::PromptComposer,
        Self::ComposerBar,
        Self::ContextMenu,
        Self::Dialog,
        Self::Dropdown,
        Self::EffortMenu,
        Self::ErrorState,
        Self::Filter,
        Self::Icon,
        Self::InlineEdit,
        Self::Input,
        Self::Manager,
        Self::McpMenu,
        Self::ModelMenu,
        Self::Motion,
        Self::Notice,
        Self::Page,
        Self::PageAddress,
        Self::PermissionMenu,
        Self::Progress,
        Self::ProjectPicker,
        Self::PromptBox,
        Self::PromptMediaOptions,
        Self::SegmentedControl,
        Self::Select,
        Self::Sheet,
        Self::Skeleton,
        Self::StartHero,
        Self::Switch,
        Self::Textarea,
        Self::Tooltip,
        Self::FileTree,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Favicon => "Favicon",
            Self::TypeIcon => "TypeIcon",
            Self::PromptGhost => "PromptGhost",
            Self::DirectoryNavigator => "DirectoryNavigator",
            Self::MatrixRain => "MatrixRain",
            Self::BackButton => "BackButton",
            Self::Address => "Address",
            Self::AgentMenu => "AgentMenu",
            Self::AlertDialog => "AlertDialog",
            Self::Avatar => "Avatar",
            Self::Badge => "Badge",
            Self::Breadcrumbs => "Breadcrumbs",
            Self::Button => "Button",
            Self::Card => "Card",
            Self::Checkbox => "Checkbox",
            Self::ShellCode => "ShellCode",
            Self::PromptComposer => "PromptComposer",
            Self::ComposerBar => "ComposerBar",
            Self::ContextMenu => "ContextMenu",
            Self::Dialog => "Dialog",
            Self::Dropdown => "Dropdown",
            Self::EffortMenu => "EffortMenu",
            Self::ErrorState => "ErrorState",
            Self::Filter => "Filter",
            Self::Icon => "Icon",
            Self::InlineEdit => "InlineEdit",
            Self::Input => "Input",
            Self::Manager => "Manager",
            Self::McpMenu => "McpMenu",
            Self::ModelMenu => "ModelMenu",
            Self::Motion => "Motion",
            Self::Notice => "Notice",
            Self::Page => "Page",
            Self::PageAddress => "PageAddress",
            Self::PermissionMenu => "PermissionMenu",
            Self::Progress => "Progress",
            Self::ProjectPicker => "ProjectPicker",
            Self::PromptBox => "PromptBox",
            Self::PromptMediaOptions => "PromptMediaOptions",
            Self::SegmentedControl => "SegmentedControl",
            Self::Select => "Select",
            Self::Sheet => "Sheet",
            Self::Skeleton => "Skeleton",
            Self::StartHero => "StartHero",
            Self::Switch => "Switch",
            Self::Textarea => "Textarea",
            Self::Tooltip => "Tooltip",
            Self::FileTree => "FileTree",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Favicon => "favicon",
            Self::TypeIcon => "type-icon",
            Self::PromptGhost => "prompt-ghost",
            Self::DirectoryNavigator => "directory",
            Self::MatrixRain => "matrix-rain",
            Self::BackButton => "back-button",
            Self::Address => "address",
            Self::AgentMenu => "agent-menu",
            Self::AlertDialog => "alert-dialog",
            Self::Avatar => "avatar",
            Self::Badge => "badge",
            Self::Breadcrumbs => "breadcrumbs",
            Self::Button => "button",
            Self::Card => "card",
            Self::Checkbox => "checkbox",
            Self::ShellCode => "shell-code",
            Self::PromptComposer => "composer",
            Self::ComposerBar => "composer-bar",
            Self::ContextMenu => "context-menu",
            Self::Dialog => "dialog",
            Self::Dropdown => "dropdown",
            Self::EffortMenu => "effort-menu",
            Self::ErrorState => "error-state",
            Self::Filter => "filter",
            Self::Icon => "icon",
            Self::InlineEdit => "inline-edit",
            Self::Input => "input",
            Self::Manager => "manager",
            Self::McpMenu => "mcp-menu",
            Self::ModelMenu => "model-menu",
            Self::Motion => "motion",
            Self::Notice => "notice",
            Self::Page => "page",
            Self::PageAddress => "page-address",
            Self::PermissionMenu => "permission-menu",
            Self::Progress => "progress",
            Self::ProjectPicker => "project-picker",
            Self::PromptBox => "prompt-box",
            Self::PromptMediaOptions => "prompt-media-options",
            Self::SegmentedControl => "segmented-control",
            Self::Select => "select",
            Self::Sheet => "sheet",
            Self::Skeleton => "skeleton",
            Self::StartHero => "start-hero",
            Self::Switch => "switch",
            Self::Textarea => "textarea",
            Self::Tooltip => "tooltip",
            Self::FileTree => "file-tree",
        }
    }

    pub fn component_names(self) -> &'static str {
        match self {
            Self::Favicon => "Favicon · GlobeIcon",
            Self::TypeIcon => "TypeIcon",
            Self::PromptGhost => "PromptGhost",
            Self::DirectoryNavigator => "DirectoryNavigator · DirectoryEntryVisual",
            Self::MatrixRain => "MatrixRain · MatrixLoader",
            Self::BackButton => "BackButton",
            Self::Address => "AddressTrail · AddressControls",
            Self::AgentMenu => "AgentMenu",
            Self::AlertDialog => {
                "AlertDialogRoot · AlertDialogContent · AlertDialogTitle · AlertDialogDescription · AlertDialogActions · AlertDialogCancel · AlertDialogAction"
            }
            Self::Avatar => "Avatar · AgentThumbnail · Identicon · IdentityOrb",
            Self::Badge => "Badge",
            Self::Breadcrumbs => {
                "Breadcrumbs · BreadcrumbItem · BreadcrumbSeparator · BreadcrumbDropdown · BreadcrumbSelector · BreadcrumbOptionIcon"
            }
            Self::Button => "Button · IconButton",
            Self::Card => "Card · CardHeader · CardTitle · CardDescription · CardContent",
            Self::Checkbox => "Checkbox",
            Self::ShellCode => "ShellCode",
            Self::PromptComposer => "PromptComposer · ComposerFrame",
            Self::ComposerBar => {
                "StatusDot · ComposerStatus · WorkspaceBadges · ComposerBar · ComposerMenus · ComposerChipIcon"
            }
            Self::ContextMenu => {
                "ContextMenu · ContextMenuTrigger · ContextMenuContent · ContextMenuItem"
            }
            Self::Dialog => "DialogRoot · DialogContent · DialogTitle · DialogDescription",
            Self::Dropdown => {
                "DropdownToggleIndicator · DropdownTrigger · DropdownOption · DropdownSelector · Dropdown · DropdownPopup"
            }
            Self::EffortMenu => "EffortMenu",
            Self::ErrorState => "ErrorState",
            Self::Filter => "FilterTrigger · FilterPill · FilterMenu · FilterOption",
            Self::Icon => {
                "Icon · LineIconView · BuiltinIconView · GitIconView · PageIconView · GitSymbolView"
            }
            Self::InlineEdit => "EditableText · InlineEdit · InlineTextarea",
            Self::Input => "Input · ActionInput",
            Self::Manager => {
                "ManagerPage · WorkspaceLayout · ManagerSearchRow · ManagerSidebarSection · ManagerSidebarItem · ManagerTabs · ManagerHeader · ManagerToolbar · ManagerControls · ManagerSearch · ManagerIconButton · ManagerAddButton · ManagerExpandableSearch · ManagerSearchAction · ManagerViewToggle · ManagerList · ManagerSection · ManagerCollection · ManagerRow · ManagerCard · ManagerDetails · ManagerThumbnail · ManagerBadge · ManagerButton · ManagerSelect · ManagerSpinner · ManagerEmpty · ManagerSkeleton"
            }
            Self::McpMenu => "McpMenu",
            Self::ModelMenu => "ModelMenu",
            Self::Motion => "ContentTransition · Collapse · Collapsible",
            Self::Notice => "Notice",
            Self::Page => {
                "PageShell · PageToolbarRow · PageSidebarToggle · PageHeader · PageContent"
            }
            Self::PageAddress => "PageBreadcrumbs · PageToolbar · AddressSurface",
            Self::PermissionMenu => "PermissionMenu",
            Self::Progress => "LoadingIcon · Progress · ProgressIndicator",
            Self::ProjectPicker => {
                "ProjectSelectTrigger · ProjectSelectPopup · ProjectPicker · BranchPicker"
            }
            Self::PromptBox => "PromptBox · PromptPopup",
            Self::PromptMediaOptions => "PromptMediaOptions",
            Self::SegmentedControl => "SegmentedControl",
            Self::Select => {
                "Select · SelectTrigger · SelectValue · SelectList · SelectGroup · SelectOption · SelectItemIndicator"
            }
            Self::Sheet => {
                "SheetHeaderAction · SheetHeaderChevron · SheetToggleRow · SheetSelectorRow"
            }
            Self::Skeleton => "Skeleton",
            Self::StartHero => "StartBackdrop · StartHero",
            Self::Switch => "Switch · SwitchThumb",
            Self::Textarea => "Textarea",
            Self::Tooltip => "Tooltip",
            Self::FileTree => {
                "FileTree · FileTreeSkeleton · FileTreeHeader · FileTreeScroller · FileTreeRowGroup · FileTreeChildren · FileTreeRow"
            }
        }
    }

    pub fn url(self) -> String {
        format!("vmux://ui/{}", self.slug())
    }

    pub fn from_url(url: &str) -> Option<Self> {
        let path = url.split(['?', '#']).next()?.strip_prefix("vmux://ui")?;
        if path.is_empty() || path == "/" {
            return Some(Self::Button);
        }
        let slug = path.strip_prefix('/')?.trim_end_matches('/');
        Self::ALL.into_iter().find(|story| slug == story.slug())
    }
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

#[vmux_api::ui_event]
pub struct GalleryMenuRequest {
    pub revision: u64,
    pub menu: String,
}

#[vmux_api::ui_event]
pub struct GalleryCursorRequest {
    pub revision: u64,
    pub cursor: usize,
}

#[vmux_api::ui_event]
pub struct GalleryAgentRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryModelRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryEffortRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryPermissionRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryProjectRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GalleryBranchRequest {
    pub revision: u64,
    pub value: String,
}

#[vmux_api::ui_event]
pub struct GallerySubmitRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryStopRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryAttachRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryRemoveAttachmentRequest {
    pub revision: u64,
    pub index: usize,
}

#[vmux_api::ui_event]
pub struct GalleryPasteRequest {
    pub revision: u64,
}

#[vmux_api::ui_event]
pub struct GalleryStatusRequest {
    pub revision: u64,
    pub status: String,
}
