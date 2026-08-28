use crate::event::{StartBranchesRequest, StartGoToBranch, StartSelectModel, StartSelectWorkspace};
use crate::page::signals::PaletteSignals;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{
    AgentMenuData, BranchMenuData, ComposerChip, ComposerMenu, ComposerMenuKind, ModelMenuData,
    ProjectMenuData,
};
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::launcher::palette::ComposerState;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::ProjectBranch;

pub struct ComposerChips {
    pub agent: ComposerChip,
    pub model: Option<ComposerChip>,
    pub project: ComposerChip,
    pub branch: Option<ComposerChip>,
}

impl ComposerChips {
    pub fn of(
        composer: &ComposerState,
        menu: ComposerMenu,
        mut model_menu_sel: Signal<usize>,
    ) -> Self {
        if composer.loading {
            return Self {
                agent: ComposerChip::loading(),
                model: Some(ComposerChip::loading()),
                project: ComposerChip::loading(),
                branch: Some(ComposerChip::loading()),
            };
        }

        let agent = ComposerChip::ready(
            composer.agent_title.clone(),
            translate("composer-choose-agent"),
        )
        .opens(EventHandler::new(move |()| {
            menu.toggle(ComposerMenuKind::Agent);
            focus_prompt_end(PROMPT_INPUT_ID);
        }));
        let model = match composer.model_name.is_empty() {
            true => None,
            false => Some(
                ComposerChip::ready(composer.model_name.clone(), translate("agent-change-model"))
                    .opens(EventHandler::new(move |()| {
                        model_menu_sel.set(0);
                        menu.toggle(ComposerMenuKind::Model);
                    })),
            ),
        };
        let project = ComposerChip::ready(
            composer.workspace_label.clone(),
            composer.workspace_title.clone(),
        )
        .opens(EventHandler::new(move |()| {
            menu.toggle(ComposerMenuKind::Project);
        }));
        let branch = match composer.is_git_repo {
            false => None,
            true => {
                let owner = composer.project.clone();
                Some(
                    ComposerChip::ready(
                        composer.branch_label.clone(),
                        composer.branch_title.clone(),
                    )
                    .opens(EventHandler::new(move |()| {
                        if menu.toggle(ComposerMenuKind::Branch) && !owner.is_empty() {
                            let _ = send(&StartBranchesRequest {
                                project: owner.clone(),
                            });
                        }
                    })),
                )
            }
        };

        Self {
            agent,
            model,
            project,
            branch,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ProjectPicking {
    pub expanded: Signal<String>,
    pub branches: Signal<Vec<ProjectBranch>>,
    pub branches_for: Signal<String>,
}

pub fn use_project_picking() -> ProjectPicking {
    ProjectPicking {
        expanded: use_signal(String::new),
        branches: use_signal(Vec::<ProjectBranch>::new),
        branches_for: use_signal(String::new),
    }
}

impl ProjectPicking {
    pub fn remember(&mut self, project: String, branches: Vec<ProjectBranch>) {
        self.branches.set(branches);
        self.branches_for.set(project);
    }

    fn expand(&mut self, path: String) {
        if *self.expanded.peek() == path {
            self.expanded.set(String::new());
            return;
        }
        self.expanded.set(path.clone());
        if *self.branches_for.peek() != path {
            self.branches.set(Vec::new());
        }
        let _ = send(&StartBranchesRequest { project: path });
    }

    fn go_to(pick: ProjectPick) {
        let _ = send(&StartGoToBranch {
            project: pick.project,
            branch: pick.branch,
            checkout: pick.checkout,
        });
        focus_prompt_end(PROMPT_INPUT_ID);
    }
}

pub struct ComposerMenuSet {
    pub agent: AgentMenuData,
    pub model: ModelMenuData,
    pub project: ProjectMenuData,
    pub branch: BranchMenuData,
}

impl ComposerMenuSet {
    pub fn of(
        composer: &ComposerState,
        mut signals: PaletteSignals,
        mut model_menu_sel: Signal<usize>,
        picking: ProjectPicking,
    ) -> Self {
        let agent = AgentMenuData {
            options: composer.agents.clone(),
            selected_url: composer.agent_url.clone(),
            on_select: EventHandler::new(move |url: String| {
                signals.retarget(url);
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let agent_key = composer.model_agent_key.clone();
        let model = ModelMenuData {
            models: composer.model_options.clone(),
            current_model_id: composer.model_current_id.clone(),
            selected: model_menu_sel(),
            on_hover: EventHandler::new(move |index| model_menu_sel.set(index)),
            on_select: EventHandler::new(move |model: ModelOptionEntry| {
                let _ = send(&StartSelectModel {
                    agent_key: agent_key.clone(),
                    model_id: model.id,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let cwd = composer.cwd.clone();
        let mut expanding = picking;
        let project = ProjectMenuData {
            projects: composer.projects.clone(),
            expanded: (picking.expanded)(),
            branches: (picking.branches)(),
            branches_for: (picking.branches_for)(),
            on_expand: EventHandler::new(move |path: String| expanding.expand(path)),
            on_pick: EventHandler::new(ProjectPicking::go_to),
            on_choose_another: EventHandler::new(move |()| {
                let _ = send(&StartSelectWorkspace {
                    current_dir: cwd.clone(),
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let branch = BranchMenuData {
            project: composer.project.clone(),
            branches: (picking.branches)(),
            loaded: (picking.branches_for)() == composer.project,
            on_pick: EventHandler::new(ProjectPicking::go_to),
        };

        Self {
            agent,
            model,
            project,
            branch,
        }
    }
}
