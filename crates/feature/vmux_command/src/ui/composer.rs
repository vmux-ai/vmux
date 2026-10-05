use dioxus::prelude::*;
use vmux_api::command_bar::{
    CommandPaletteComposer, CommandPaletteMenuDismissRequest, CommandPaletteMenuHighlightRequest,
    CommandPaletteMenus, CommandPaletteUiState, OpenId,
};
use vmux_ui::components::agent_menu::AgentMenu;
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::permission_menu::PermissionMenu;
use vmux_ui::components::project_picker::{BranchPicker, ProjectPicker};
use vmux_ui::components::prompt_box::PromptPopupPlacement;
use vmux_ui::hooks::send;

use super::composer_driver::MenuSelection;

#[component]
pub fn CommandComposerMenus(
    composer: CommandPaletteComposer,
    palette: CommandPaletteUiState,
    open_id: OpenId,
    menus: CommandPaletteMenus,
    cursor: usize,
) -> Element {
    let selection = MenuSelection::new(open_id);
    let agents = composer.agents.clone();
    let models = composer.model_options.clone();
    let modes = composer.permission_modes.clone();
    let projects = composer.projects.clone();
    let branches = palette.branches.clone();
    let project_count = projects.iter().filter(|project| project.depth == 0).count();

    rsx! {
        if menus.agent {
            AgentMenu {
                placement: PromptPopupPlacement::Downward,
                options: agents.clone(),
                selected_url: composer.agent_url.clone(),
                cursor,
                on_hover: move |index| {
                    let _ = send(&CommandPaletteMenuHighlightRequest {
                        open_id,
                        index: index as u32,
                    });
                },
                on_select: move |url: String| selection.agent(&agents, &url),
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
        if menus.model {
            ModelMenu {
                placement: PromptPopupPlacement::Downward,
                models: models.clone(),
                current_model_id: composer.model_current_id.clone(),
                selected: cursor,
                on_hover: move |index| {
                    let _ = send(&CommandPaletteMenuHighlightRequest {
                        open_id,
                        index: index as u32,
                    });
                },
                on_select: move |(_index, model): (usize, vmux_api::room::ModelOptionEntry)| {
                    selection.model(&models, &model.id);
                },
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
        if menus.permission {
            PermissionMenu {
                placement: PromptPopupPlacement::Downward,
                modes: modes.clone(),
                current_mode_id: composer.permission_current_id.clone(),
                selected: cursor,
                on_hover: move |index| {
                    let _ = send(&CommandPaletteMenuHighlightRequest {
                        open_id,
                        index: index as u32,
                    });
                },
                on_select: move |mode: vmux_api::protocol::AcpModeOption| {
                    selection.permission(&modes, &mode.id);
                },
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
        if menus.project {
            ProjectPicker {
                placement: PromptPopupPlacement::Downward,
                projects: projects.clone(),
                loaded: !composer.projects.is_empty(),
                cursor,
                on_hover: move |index| {
                    let _ = send(&CommandPaletteMenuHighlightRequest {
                        open_id,
                        index: index as u32,
                    });
                },
                on_pick: move |pick| selection.project(&projects, &pick),
                on_choose_another: move |()| selection.activate(project_count),
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
        if menus.branch {
            BranchPicker {
                placement: PromptPopupPlacement::Downward,
                project: composer.project.clone(),
                branches: branches.clone(),
                loaded: palette.branch_project == composer.project,
                cursor,
                on_hover: move |index| {
                    let _ = send(&CommandPaletteMenuHighlightRequest {
                        open_id,
                        index: index as u32,
                    });
                },
                on_pick: move |pick| selection.branch(&branches, &pick),
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
    }
}
