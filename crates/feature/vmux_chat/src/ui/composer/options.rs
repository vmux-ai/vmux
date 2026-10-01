use crate::event::{ChatGoToBranch, ChatSelectWorkspace, ModelOptionEntry, SetAgentEffort};
use crate::ui::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{
    BranchMenuData, ComposerMenus, EffortMenuData, PermissionMenuData, ProjectMenuData,
};
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;

#[component]
pub(super) fn ChatComposerMenus(chat: Chat) -> Element {
    let menus = ChatMenuSet::from(chat);
    let menu = chat.menu;
    rsx! {
        ComposerMenus {
            opened: menu.opened(),
            cursor: menu.cursor(),
            on_hover: move |index| menu.point_at(index),
            on_dismiss: move |()| menu.close(),
            on_selected: move |()| menu.close(),
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

impl From<Chat> for ChatMenuSet {
    fn from(chat: Chat) -> Self {
        let context = chat.slash.context();
        let effort_state = chat.effort.current();
        let agent_key = effort_state.agent_key.clone();
        let effort = EffortMenuData {
            levels: effort_state.effort_levels,
            selected: effort_state.effort_current,
            on_select: EventHandler::new(move |level: String| {
                let _ = send(&SetAgentEffort {
                    agent_key: agent_key.clone(),
                    level,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let permission_state = chat.permissions.current();
        let permission = PermissionMenuData {
            modes: permission_state.modes,
            current_mode_id: permission_state.current_mode_id,
            on_select: EventHandler::new(move |mode: vmux_api::protocol::AcpModeOption| {
                chat.select_mode(mode.id)
            }),
        };
        let project = ProjectMenuData {
            projects: context.projects.clone(),
            loaded: chat.projects.context_ready(),
            on_pick: EventHandler::new(Self::go_to),
            on_choose_another: EventHandler::new(move |()| {
                let _ = send(&ChatSelectWorkspace);
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let branches = chat.projects.branches();
        let branch = BranchMenuData {
            project: context.cwd.clone(),
            branches: branches.branches,
            loaded: branches.project == context.cwd && !branches.loading,
            on_pick: EventHandler::new(Self::go_to),
        };

        Self {
            effort,
            permission,
            project,
            branch,
        }
    }
}

impl ChatMenuSet {
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
    let menu_sel = chat.slash.menu_sel;
    rsx! {
        ModelMenu {
            models: chat.filtered_models(),
            current_model_id: chat.models.current().current_model_id,
            selected: menu_sel(),
            on_hover: move |index| chat.point_at_list(index),
            on_select: move |(index, _model): (usize, ModelOptionEntry)| chat.choose_list(index),
            on_dismiss: move |()| chat.dismiss_selector(),
        }
    }
}
