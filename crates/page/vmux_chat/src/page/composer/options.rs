use crate::event::{ChatGoToBranch, ChatSelectWorkspace, ModelOptionEntry, SetAgentEffort};
use crate::page::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{ComposerMenus, EffortMenuData, ProjectMenuData};
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;

#[component]
pub(super) fn ChatComposerMenus(chat: Chat) -> Element {
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
    let project = ProjectMenuData {
        projects: context.projects.clone(),
        expanded: (chat.projects.expanded)(),
        branches: (chat.projects.branches)(),
        branches_for: (chat.projects.branches_for)(),
        on_expand: EventHandler::new(move |path: String| chat.projects.expand(&path)),
        on_pick: EventHandler::new(move |pick: ProjectPick| {
            let _ = send(&ChatGoToBranch {
                project: pick.project,
                branch: pick.branch,
                checkout: pick.checkout,
            });
            focus_prompt_end(PROMPT_INPUT_ID);
        }),
        on_choose_another: EventHandler::new(move |()| {
            let _ = send(&ChatSelectWorkspace);
            focus_prompt_end(PROMPT_INPUT_ID);
        }),
    };
    rsx! {
        ComposerMenus {
            menu: chat.menu,
            effort: Some(effort),
            project: Some(project),
        }
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
