use crate::event::{ChatGoToBranch, ChatSelectWorkspace, ModelOptionEntry, SetAgentEffort};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{
    BranchMenuData, ComposerMenuKind, ComposerMenus, EffortMenuData, PermissionMenuData,
    ProjectMenuData,
};
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;

#[component]
pub(super) fn ChatComposerMenus(chat: Chat) -> Element {
    let menus = ChatMenuSet::of(chat);
    rsx! {
        ComposerMenus {
            menu: chat.menu,
            effort: Some(menus.effort),
            permission: Some(menus.permission),
            project: Some(menus.project),
            branch: Some(menus.branch),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ChatMenuSet {
    effort: EffortMenuData,
    permission: PermissionMenuData,
    project: ProjectMenuData,
    branch: BranchMenuData,
}

impl ChatMenuSet {
    pub(crate) fn of(chat: Chat) -> Self {
        let context = (chat.slash.composer_context)();
        let agent_key = (chat.effort.agent_key)();
        let mut current = chat.effort.current;
        let effort = EffortMenuData {
            levels: (chat.effort.levels)(),
            selected: current(),
            on_select: EventHandler::new(move |level: String| {
                current.set(level.clone());
                let _ = send(&SetAgentEffort {
                    agent_key: agent_key.clone(),
                    level,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let permission = PermissionMenuData {
            modes: (chat.permissions.modes)(),
            current_mode_id: (chat.permissions.current_mode_id)(),
            on_select: EventHandler::new(move |mode: vmux_wire::protocol::AcpModeOption| {
                chat.select_mode(mode.id)
            }),
        };
        let project = ProjectMenuData {
            projects: context.projects.clone(),
            loaded: (chat.projects.loaded)(),
            on_pick: EventHandler::new(Self::go_to),
            on_choose_another: EventHandler::new(move |()| {
                let _ = send(&ChatSelectWorkspace);
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let branch = BranchMenuData {
            project: context.cwd.clone(),
            branches: (chat.projects.branches)(),
            loaded: (chat.projects.branches_for)() == context.cwd,
            on_pick: EventHandler::new(Self::go_to),
        };

        Self {
            effort,
            permission,
            project,
            branch,
        }
    }

    pub(crate) fn rows(&self, kind: ComposerMenuKind) -> usize {
        match kind {
            ComposerMenuKind::Agent | ComposerMenuKind::Model => 0,
            ComposerMenuKind::Effort => self.effort.levels.len() + 1,
            ComposerMenuKind::Permission => self.permission.modes.len(),
            ComposerMenuKind::Project => self.roots().len() + 1,
            ComposerMenuKind::Branch => self.branch.branches.len(),
        }
    }

    pub(crate) fn choose(&self, kind: ComposerMenuKind, index: usize) -> bool {
        match kind {
            ComposerMenuKind::Agent | ComposerMenuKind::Model => return false,
            ComposerMenuKind::Effort => {
                if index == 0 {
                    self.effort.on_select.call(String::new());
                    return true;
                }
                let Some(level) = self.effort.levels.get(index - 1) else {
                    return false;
                };
                self.effort.on_select.call(level.clone());
            }
            ComposerMenuKind::Permission => {
                let Some(mode) = self.permission.modes.get(index) else {
                    return false;
                };
                self.permission.on_select.call(mode.clone());
            }
            ComposerMenuKind::Project => {
                let roots = self.roots();
                if index == roots.len() {
                    self.project.on_choose_another.call(());
                    return true;
                }
                let Some(project) = roots.get(index) else {
                    return false;
                };
                self.project.on_pick.call(ProjectPick {
                    project: project.path.clone(),
                    branch: String::new(),
                    checkout: String::new(),
                });
            }
            ComposerMenuKind::Branch => {
                let Some(branch) = self.branch.branches.get(index) else {
                    return false;
                };
                self.branch.on_pick.call(ProjectPick {
                    project: self.branch.project.clone(),
                    branch: branch.branch.clone(),
                    checkout: branch.checkout.clone(),
                });
            }
        }
        true
    }

    fn roots(&self) -> Vec<&vmux_wire::space::ProjectRow> {
        let mut roots = Vec::new();
        for project in &self.project.projects {
            if project.depth == 0 {
                roots.push(project);
            }
        }
        roots
    }

    fn go_to(pick: ProjectPick) {
        let _ = send(&ChatGoToBranch {
            project: pick.project,
            branch: pick.branch,
            checkout: pick.checkout,
        });
        focus_prompt_end(PROMPT_INPUT_ID);
    }
}

#[component]
pub(super) fn ChatModelMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        ModelMenu {
            models: chat.filtered_models(),
            current_model_id: (chat.models.current_model_id)(),
            selected: menu_sel(),
            on_hover: move |index| menu_sel.set(index),
            on_select: move |model: ModelOptionEntry| chat.select_model(&model),
            on_dismiss: move |()| chat.dismiss_selector(),
        }
    }
}
