use dioxus::prelude::*;
use vmux_api::command_bar::{
    CommandPaletteComposer, CommandPaletteMenu, CommandPaletteMenuActivateRequest,
    CommandPaletteMenuDismissRequest, CommandPaletteMenuHighlightRequest,
    CommandPaletteMenuToggleRequest, CommandPaletteState, OpenId,
};
use vmux_ui::components::agent_menu::AgentMenu;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{ComposerChip, ComposerMenuKind};
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::permission_menu::PermissionMenu;
use vmux_ui::components::project_picker::{BranchPicker, ProjectPick, ProjectPicker};
use vmux_ui::components::prompt_box::PromptPopupPlacement;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

pub struct ComposerChips {
    pub agent: ComposerChip,
    pub model: Option<ComposerChip>,
    pub permission: Option<ComposerChip>,
    pub project: ComposerChip,
    pub branch: Option<ComposerChip>,
}

impl ComposerChips {
    pub fn build(composer: &CommandPaletteComposer, open_id: OpenId) -> Self {
        if composer.loading {
            return Self {
                agent: ComposerChip::loading(),
                model: Some(ComposerChip::loading()),
                permission: None,
                project: ComposerChip::loading(),
                branch: Some(ComposerChip::loading()),
            };
        }

        let agent = ComposerChip::ready(
            composer.agent_title.clone(),
            translate("composer-choose-agent"),
        )
        .opens(EventHandler::new(move |()| {
            MenuSelection(open_id).toggle(CommandPaletteMenu::Agent);
        }));
        let model = match composer.model_name.is_empty() {
            true => None,
            false => Some(
                ComposerChip::ready(composer.model_name.clone(), translate("agent-change-model"))
                    .opens(EventHandler::new(move |()| {
                        MenuSelection(open_id).toggle(CommandPaletteMenu::Model);
                    })),
            ),
        };
        let project = ComposerChip::ready(
            composer.workspace_label.clone(),
            composer.workspace_title.clone(),
        )
        .opens(EventHandler::new(move |()| {
            MenuSelection(open_id).toggle(CommandPaletteMenu::Project);
        }));
        let branch = match composer.is_git_repo {
            false => None,
            true => Some(
                ComposerChip::ready(composer.branch_label.clone(), composer.branch_title.clone())
                    .opens(EventHandler::new(move |()| {
                        MenuSelection(open_id).toggle(CommandPaletteMenu::Branch);
                    })),
            ),
        };
        let permission = if composer.permission_modes.is_empty() {
            None
        } else {
            let current = composer
                .permission_modes
                .iter()
                .find(|mode| mode.id == composer.permission_current_id);
            let label = current
                .map(|mode| mode.name.clone())
                .unwrap_or_else(|| composer.permission_current_id.clone());
            let title = current
                .and_then(|mode| mode.description.clone())
                .filter(|description| !description.is_empty())
                .unwrap_or_else(|| translate("composer-permission-change"));
            Some(
                ComposerChip::ready(label, title).opens(EventHandler::new(move |()| {
                    MenuSelection(open_id).toggle(CommandPaletteMenu::Permission);
                })),
            )
        };

        Self {
            agent,
            model,
            permission,
            project,
            branch,
        }
    }
}

#[derive(Clone, Copy)]
struct MenuSelection(OpenId);

impl MenuSelection {
    fn toggle(self, menu: CommandPaletteMenu) {
        let _ = send(&CommandPaletteMenuToggleRequest {
            open_id: self.0,
            menu,
        });
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    fn activate(self, index: usize) {
        let _ = send(&CommandPaletteMenuActivateRequest {
            open_id: self.0,
            index: index as u32,
        });
        focus_prompt_end(PROMPT_INPUT_ID);
    }

    fn agent(self, options: &[vmux_api::command_bar::CommandPaletteAgent], url: &str) {
        let Some(index) = options.iter().position(|option| option.url == url) else {
            return;
        };
        self.activate(index);
    }

    fn model(self, models: &[vmux_api::room::ModelOptionEntry], id: &str) {
        let Some(index) = models.iter().position(|model| model.id == id) else {
            return;
        };
        self.activate(index);
    }

    fn permission(self, modes: &[vmux_api::protocol::AcpModeOption], id: &str) {
        let Some(index) = modes.iter().position(|mode| mode.id == id) else {
            return;
        };
        self.activate(index);
    }

    fn project(self, projects: &[vmux_api::space::ProjectRow], pick: &ProjectPick) {
        let mut index = 0;
        for project in projects {
            if project.depth != 0 {
                continue;
            }
            if project.path == pick.project {
                self.activate(index);
                return;
            }
            index += 1;
        }
    }

    fn branch(self, branches: &[vmux_api::space::ProjectBranch], pick: &ProjectPick) {
        let Some(index) = branches
            .iter()
            .position(|branch| branch.branch == pick.branch && branch.checkout == pick.checkout)
        else {
            return;
        };
        self.activate(index);
    }
}

pub struct ComposerMenuView;

impl ComposerMenuView {
    pub const fn kind(menu: Option<CommandPaletteMenu>) -> Option<ComposerMenuKind> {
        match menu {
            Some(CommandPaletteMenu::Agent) => Some(ComposerMenuKind::Agent),
            Some(CommandPaletteMenu::Model) => Some(ComposerMenuKind::Model),
            Some(CommandPaletteMenu::Permission) => Some(ComposerMenuKind::Permission),
            Some(CommandPaletteMenu::Project) => Some(ComposerMenuKind::Project),
            Some(CommandPaletteMenu::Branch) => Some(ComposerMenuKind::Branch),
            None => None,
        }
    }
}

#[component]
pub fn CommandComposerMenus(
    composer: CommandPaletteComposer,
    palette: CommandPaletteState,
    open_id: OpenId,
    opened: Option<CommandPaletteMenu>,
    cursor: usize,
) -> Element {
    let selection = MenuSelection(open_id);
    let agents = composer.agents.clone();
    let models = composer.model_options.clone();
    let modes = composer.permission_modes.clone();
    let projects = composer.projects.clone();
    let branches = palette.branches.clone();
    let project_count = projects.iter().filter(|project| project.depth == 0).count();

    rsx! {
        if opened == Some(CommandPaletteMenu::Agent) {
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
        if opened == Some(CommandPaletteMenu::Model) {
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
                on_select: move |model: vmux_api::room::ModelOptionEntry| {
                    selection.model(&models, &model.id);
                },
                on_dismiss: move |()| {
                    let _ = send(&CommandPaletteMenuDismissRequest { open_id });
                },
            }
        }
        if opened == Some(CommandPaletteMenu::Permission) {
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
        if opened == Some(CommandPaletteMenu::Project) {
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
        if opened == Some(CommandPaletteMenu::Branch) {
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
