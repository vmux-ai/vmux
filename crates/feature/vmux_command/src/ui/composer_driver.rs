use dioxus::prelude::*;
use vmux_api::command_bar::{
    CommandPaletteAgentMenuToggleRequest, CommandPaletteBranchMenuToggleRequest,
    CommandPaletteComposer, CommandPaletteMenuActivateRequest, CommandPaletteMenus,
    CommandPaletteModelMenuToggleRequest, CommandPalettePermissionMenuToggleRequest,
    CommandPaletteProjectMenuToggleRequest, OpenId,
};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptFocus};
use vmux_ui::components::composer_bar::{ComposerChip, ComposerMenuKind};
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

pub(super) struct ComposerChips {
    pub(super) agent: ComposerChip,
    pub(super) model: Option<ComposerChip>,
    pub(super) permission: Option<ComposerChip>,
    pub(super) project: ComposerChip,
    pub(super) branch: Option<ComposerChip>,
}

impl ComposerChips {
    pub(super) fn build(composer: &CommandPaletteComposer, open_id: OpenId) -> Self {
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
            let _ = send(&CommandPaletteAgentMenuToggleRequest { open_id });
            PromptFocus::end(PROMPT_INPUT_ID);
        }));
        let model = match composer.model_name.is_empty() {
            true => None,
            false => Some(
                ComposerChip::ready(composer.model_name.clone(), translate("agent-change-model"))
                    .opens(EventHandler::new(move |()| {
                        let _ = send(&CommandPaletteModelMenuToggleRequest { open_id });
                        PromptFocus::end(PROMPT_INPUT_ID);
                    })),
            ),
        };
        let project = ComposerChip::ready(
            composer.workspace_label.clone(),
            composer.workspace_title.clone(),
        )
        .opens(EventHandler::new(move |()| {
            let _ = send(&CommandPaletteProjectMenuToggleRequest { open_id });
            PromptFocus::end(PROMPT_INPUT_ID);
        }));
        let branch = match composer.is_git_repo {
            false => None,
            true => Some(
                ComposerChip::ready(composer.branch_label.clone(), composer.branch_title.clone())
                    .opens(EventHandler::new(move |()| {
                        let _ = send(&CommandPaletteBranchMenuToggleRequest { open_id });
                        PromptFocus::end(PROMPT_INPUT_ID);
                    })),
            ),
        };
        let permission = if composer.permission_modes.is_empty() {
            None
        } else {
            Some(
                ComposerChip::ready(
                    composer.permission_name.clone(),
                    composer.permission_title.clone(),
                )
                .opens(EventHandler::new(move |()| {
                    let _ = send(&CommandPalettePermissionMenuToggleRequest { open_id });
                    PromptFocus::end(PROMPT_INPUT_ID);
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
pub(super) struct MenuSelection(OpenId);

impl MenuSelection {
    pub(super) fn new(open_id: OpenId) -> Self {
        Self(open_id)
    }

    pub(super) fn activate(self, index: usize) {
        let _ = send(&CommandPaletteMenuActivateRequest {
            open_id: self.0,
            index: index as u32,
        });
        PromptFocus::end(PROMPT_INPUT_ID);
    }

    pub(super) fn agent(self, options: &[vmux_api::command_bar::CommandPaletteAgent], url: &str) {
        let Some(index) = options.iter().position(|option| option.url == url) else {
            return;
        };
        self.activate(index);
    }

    pub(super) fn model(self, models: &[vmux_api::conversation::ModelOptionEntry], id: &str) {
        let Some(index) = models.iter().position(|model| model.id == id) else {
            return;
        };
        self.activate(index);
    }

    pub(super) fn permission(self, modes: &[vmux_api::protocol::AcpModeOption], id: &str) {
        let Some(index) = modes.iter().position(|mode| mode.id == id) else {
            return;
        };
        self.activate(index);
    }

    pub(super) fn project(self, projects: &[vmux_api::space::ProjectRow], pick: &ProjectPick) {
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

    pub(super) fn branch(self, branches: &[vmux_api::space::ProjectBranch], pick: &ProjectPick) {
        let Some(index) = branches
            .iter()
            .position(|branch| branch.branch == pick.branch && branch.checkout == pick.checkout)
        else {
            return;
        };
        self.activate(index);
    }
}

pub(super) struct ComposerMenuView;

impl ComposerMenuView {
    pub(super) const fn kind(menus: CommandPaletteMenus) -> Option<ComposerMenuKind> {
        if menus.agent {
            return Some(ComposerMenuKind::Agent);
        }
        if menus.model {
            return Some(ComposerMenuKind::Model);
        }
        if menus.permission {
            return Some(ComposerMenuKind::Permission);
        }
        if menus.project {
            return Some(ComposerMenuKind::Project);
        }
        if menus.branch {
            return Some(ComposerMenuKind::Branch);
        }
        None
    }
}
