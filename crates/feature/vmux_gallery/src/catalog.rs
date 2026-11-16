#[cfg(ui)]
use crate::GalleryUiState;
#[cfg(ui)]
use dioxus::prelude::*;

#[vmux_macro::story_catalog(
    state = GalleryUiState,
    categories = [
        "gallery-category-primitives", "gallery-category-forms", "gallery-category-composition",
        "gallery-category-navigation", "gallery-category-feedback", "gallery-category-icons"
    ]
)]
#[vmux_api::contract(Default, Copy, Eq)]
pub enum Story {
    #[story(metadata = "../../util/vmux_ui/src/favicon.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    Favicon,
    #[story(metadata = "../../util/vmux_ui/src/file_icon.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    TypeIcon,
    #[story(metadata = "../../util/vmux_ui/src/prompt_ghost.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    PromptGhost,
    #[story(metadata = "../../util/vmux_ui/src/directory.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    DirectoryNavigator,
    #[story(metadata = "../../util/vmux_ui/src/matrix_rain.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    MatrixRain,
    #[story(metadata = "../../util/vmux_ui/src/back.stories.ron", component = crate::ui::previews::AuxiliaryPreview)]
    BackButton,
    #[story(metadata = "../../util/vmux_ui/src/components/address.stories.ron", component = crate::ui::previews::NavigationPreview)]
    Address,
    #[story(metadata = "../../util/vmux_ui/src/components/agent_menu.stories.ron", component = crate::ui::previews::MenuPreview)]
    AgentMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/alert_dialog.stories.ron", component = crate::ui::previews::SurfacePreview)]
    AlertDialog,
    #[story(metadata = "../../util/vmux_ui/src/components/avatar.stories.ron", component = crate::ui::previews::BasicPreview)]
    Avatar,
    #[story(metadata = "../../util/vmux_ui/src/components/badge.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Badge,
    #[story(metadata = "../../util/vmux_ui/src/components/breadcrumb.stories.ron", component = crate::ui::previews::NavigationPreview)]
    Breadcrumbs,
    #[story(metadata = "../../util/vmux_ui/src/components/button.stories.ron", component = crate::ui::previews::BasicPreview)]
    #[default]
    Button,
    #[story(metadata = "../../util/vmux_ui/src/components/card.stories.ron", component = crate::ui::previews::BasicPreview)]
    Card,
    #[story(metadata = "../../util/vmux_ui/src/components/checkbox.stories.ron", component = crate::ui::previews::BasicPreview)]
    Checkbox,
    #[story(metadata = "../../util/vmux_ui/src/components/code.stories.ron", component = crate::ui::previews::SurfacePreview)]
    ShellCode,
    #[story(metadata = "../../util/vmux_ui/src/components/composer.stories.ron", component = crate::ui::previews::ComposerPreview)]
    PromptComposer,
    #[story(metadata = "../../util/vmux_ui/src/components/composer_bar.stories.ron", component = crate::ui::previews::ComposerPreview)]
    ComposerBar,
    #[story(metadata = "../../util/vmux_ui/src/components/context_menu.stories.ron", component = crate::ui::previews::SurfacePreview)]
    ContextMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/dialog.stories.ron", component = crate::ui::previews::BasicPreview)]
    Dialog,
    #[story(metadata = "../../util/vmux_ui/src/components/dropdown.stories.ron", component = crate::ui::previews::NavigationPreview)]
    Dropdown,
    #[story(metadata = "../../util/vmux_ui/src/components/effort_menu.stories.ron", component = crate::ui::previews::MenuPreview)]
    EffortMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/error_state.stories.ron", component = crate::ui::previews::SurfacePreview)]
    ErrorState,
    #[story(metadata = "../../util/vmux_ui/src/components/filter.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Filter,
    #[story(metadata = "../../util/vmux_ui/src/components/icon.stories.ron", component = crate::ui::icons::IconLibrary)]
    Icon,
    #[story(metadata = "../../util/vmux_ui/src/components/inline_edit.stories.ron", component = crate::ui::previews::EditingPreview)]
    InlineEdit,
    #[story(metadata = "../../util/vmux_ui/src/components/input.stories.ron", component = crate::ui::previews::BasicPreview)]
    Input,
    #[story(metadata = "../../util/vmux_ui/src/components/manager.stories.ron", component = crate::ui::previews::ManagementPreview)]
    Manager,
    #[story(metadata = "../../util/vmux_ui/src/components/mcp_menu.stories.ron", component = crate::ui::previews::MenuPreview)]
    McpMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/model_menu.stories.ron", component = crate::ui::previews::MenuPreview)]
    ModelMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/motion.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Motion,
    #[story(metadata = "../../util/vmux_ui/src/components/notice.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Notice,
    #[story(metadata = "../../util/vmux_ui/src/components/page.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Page,
    #[story(metadata = "../../util/vmux_ui/src/components/page_address.stories.ron", component = crate::ui::previews::NavigationPreview)]
    PageAddress,
    #[story(metadata = "../../util/vmux_ui/src/components/permission_menu.stories.ron", component = crate::ui::previews::MenuPreview)]
    PermissionMenu,
    #[story(metadata = "../../util/vmux_ui/src/components/progress.stories.ron", component = crate::ui::previews::BasicPreview)]
    Progress,
    #[story(metadata = "../../util/vmux_ui/src/components/project_picker.stories.ron", component = crate::ui::previews::MenuPreview)]
    ProjectPicker,
    #[story(metadata = "../../util/vmux_ui/src/components/prompt_box.stories.ron", component = crate::ui::previews::ComposerPreview)]
    PromptBox,
    #[story(metadata = "../../util/vmux_ui/src/components/prompt_media_options.stories.ron", component = crate::ui::previews::MenuPreview)]
    PromptMediaOptions,
    #[story(metadata = "../../util/vmux_ui/src/components/segmented_control.stories.ron", component = crate::ui::previews::NavigationPreview)]
    SegmentedControl,
    #[story(metadata = "../../util/vmux_ui/src/components/select.stories.ron", component = crate::ui::previews::BasicPreview)]
    Select,
    #[story(metadata = "../../util/vmux_ui/src/components/sheet.stories.ron", component = crate::ui::previews::NavigationPreview)]
    Sheet,
    #[story(metadata = "../../util/vmux_ui/src/components/skeleton.stories.ron", component = crate::ui::previews::BasicPreview)]
    Skeleton,
    #[story(metadata = "../../util/vmux_ui/src/components/start_hero.stories.ron", component = crate::ui::previews::SurfacePreview)]
    StartHero,
    #[story(metadata = "../../util/vmux_ui/src/components/switch.stories.ron", component = crate::ui::previews::BasicPreview)]
    Switch,
    #[story(metadata = "../../util/vmux_ui/src/components/textarea.stories.ron", component = crate::ui::previews::BasicPreview)]
    Textarea,
    #[story(metadata = "../../util/vmux_ui/src/components/tooltip.stories.ron", component = crate::ui::previews::SurfacePreview)]
    Tooltip,
    #[story(metadata = "../../util/vmux_ui/src/components/tree_row.stories.ron", component = crate::ui::previews::NavigationPreview)]
    FileTree,
}
