use crate::event::{
    StartBranchesRequest, StartGoToBranch, StartSelectMode, StartSelectModel, StartSelectWorkspace,
};
use crate::page::signals::PaletteSignals;
use dioxus::prelude::*;
use vmux_ui::components::composer::{PROMPT_INPUT_ID, focus_prompt_end};
use vmux_ui::components::composer_bar::{
    AgentMenuData, BranchMenuData, ComposerChip, ComposerMenu, ComposerMenuKind, ModelMenuData,
    PermissionMenuData, ProjectMenuData,
};
use vmux_ui::components::project_picker::ProjectPick;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::launcher::palette::ComposerState;
use vmux_ui::prompt_recall::{PromptHistoryDirection, move_prompt_history};
use vmux_wire::chat::PromptHistoryRequest;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::ProjectBranch;

pub struct ComposerChips {
    pub agent: ComposerChip,
    pub model: Option<ComposerChip>,
    pub permission: Option<ComposerChip>,
    pub project: ComposerChip,
    pub branch: Option<ComposerChip>,
}

impl ComposerChips {
    pub fn of(composer: &ComposerState, menu: ComposerMenu, mut picking: ProjectPicking) -> Self {
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
            menu.toggle(ComposerMenuKind::Agent);
            focus_prompt_end(PROMPT_INPUT_ID);
        }));
        let model = match composer.model_name.is_empty() {
            true => None,
            false => Some(
                ComposerChip::ready(composer.model_name.clone(), translate("agent-change-model"))
                    .opens(EventHandler::new(move |()| {
                        menu.toggle(ComposerMenuKind::Model);
                    })),
            ),
        };
        let project_cursor = composer
            .projects
            .iter()
            .filter(|project| project.depth == 0)
            .position(|project| project.is_active)
            .unwrap_or(0);
        let project = ComposerChip::ready(
            composer.workspace_label.clone(),
            composer.workspace_title.clone(),
        )
        .opens(EventHandler::new(move |()| {
            menu.toggle_at(ComposerMenuKind::Project, project_cursor);
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
                        if menu.toggle(ComposerMenuKind::Branch) {
                            picking.read_ahead(&owner);
                        }
                    })),
                )
            }
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
            let selected = composer
                .permission_modes
                .iter()
                .position(|mode| mode.id == composer.permission_current_id)
                .unwrap_or(0);
            Some(
                ComposerChip::ready(label, title).opens(EventHandler::new(move |()| {
                    menu.toggle_at(ComposerMenuKind::Permission, selected);
                    focus_prompt_end(PROMPT_INPUT_ID);
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
pub struct PromptRecall {
    history: Signal<Vec<String>>,
    cursor: Signal<Option<usize>>,
    scratch: Signal<String>,
    handed: Signal<String>,
    asked_for: Signal<String>,
}

pub fn use_prompt_recall() -> PromptRecall {
    PromptRecall {
        history: use_signal(Vec::<String>::new),
        cursor: use_signal(|| None),
        scratch: use_signal(String::new),
        handed: use_signal(String::new),
        asked_for: use_signal(String::new),
    }
}

impl PromptRecall {
    pub fn remember(&mut self, prompts: Vec<String>) {
        self.history.set(prompts);
    }

    pub fn read_ahead(&mut self, agent: &str, cwd: &str) {
        if agent.is_empty() || cwd.is_empty() {
            return;
        }
        let asked = format!("{agent}\u{0}{cwd}");
        if *self.asked_for.peek() == asked {
            return;
        }
        self.asked_for.set(asked);
        let _ = send(&PromptHistoryRequest {
            agent: agent.to_string(),
            cwd: cwd.to_string(),
        });
    }

    pub fn recalling(&self, current: &str) -> bool {
        self.place_in(current).is_some()
    }

    fn place_in(&self, current: &str) -> Option<usize> {
        let cursor = (*self.cursor.peek())?;
        (self.handed.peek().as_str() == current).then_some(cursor)
    }

    pub fn walk(&mut self, direction: PromptHistoryDirection, current: &str) -> Option<String> {
        let history = self.history.peek().clone();
        if history.is_empty() {
            return None;
        }
        let (value, next, scratch) = move_prompt_history(
            &history,
            self.place_in(current),
            &self.scratch.peek().clone(),
            current,
            direction,
        );
        self.cursor.set(next);
        self.scratch.set(scratch);
        self.handed.set(value.clone());
        Some(value)
    }
}

#[derive(Clone, Copy)]
pub struct ProjectPicking {
    pub branches: Signal<Vec<ProjectBranch>>,
    pub branches_for: Signal<String>,
    asked_for: Signal<String>,
}

pub fn use_project_picking() -> ProjectPicking {
    ProjectPicking {
        branches: use_signal(Vec::<ProjectBranch>::new),
        branches_for: use_signal(String::new),
        asked_for: use_signal(String::new),
    }
}

impl ProjectPicking {
    pub fn remember(&mut self, project: String, branches: Vec<ProjectBranch>) {
        self.branches.set(branches);
        self.branches_for.set(project);
    }

    pub fn read_ahead(&mut self, project: &str) {
        if project.is_empty() || *self.asked_for.peek() == project {
            return;
        }
        self.asked_for.set(project.to_string());
        let _ = send(&StartBranchesRequest {
            project: project.to_string(),
        });
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

#[derive(Clone)]
pub struct ComposerMenuSet {
    pub agent: AgentMenuData,
    pub model: ModelMenuData,
    pub permission: PermissionMenuData,
    pub project: ProjectMenuData,
    pub branch: BranchMenuData,
}

impl ComposerMenuSet {
    pub fn of(
        composer: &ComposerState,
        mut signals: PaletteSignals,
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
            on_select: EventHandler::new(move |model: ModelOptionEntry| {
                let _ = send(&StartSelectModel {
                    agent_key: agent_key.clone(),
                    model_id: model.id,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let cwd = composer.cwd.clone();
        let permission_agent_key = composer.permission_agent_key.clone();
        let permission = PermissionMenuData {
            modes: composer.permission_modes.clone(),
            current_mode_id: composer.permission_current_id.clone(),
            on_select: EventHandler::new(move |mode: vmux_wire::protocol::AcpModeOption| {
                let _ = send(&StartSelectMode {
                    agent_key: permission_agent_key.clone(),
                    mode_id: mode.id,
                });
                focus_prompt_end(PROMPT_INPUT_ID);
            }),
        };
        let project = ProjectMenuData {
            projects: composer.projects.clone(),
            loaded: !composer.projects.is_empty(),
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
            permission,
            project,
            branch,
        }
    }

    pub fn rows(&self, kind: ComposerMenuKind) -> usize {
        match kind {
            ComposerMenuKind::Agent => self.agent.options.len(),
            ComposerMenuKind::Model => self.model.models.len(),
            ComposerMenuKind::Effort => 0,
            ComposerMenuKind::Permission => self.permission.modes.len(),
            ComposerMenuKind::Project => self.roots().len() + 1,
            ComposerMenuKind::Branch => self.branch.branches.len(),
        }
    }

    pub fn choose(&self, kind: ComposerMenuKind, index: usize) -> bool {
        match kind {
            ComposerMenuKind::Agent => {
                let Some(option) = self.agent.options.get(index) else {
                    return false;
                };
                self.agent.on_select.call(option.url.clone());
            }
            ComposerMenuKind::Model => {
                let Some(model) = self.model.models.get(index) else {
                    return false;
                };
                self.model.on_select.call(model.clone());
            }
            ComposerMenuKind::Effort => return false,
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
}
