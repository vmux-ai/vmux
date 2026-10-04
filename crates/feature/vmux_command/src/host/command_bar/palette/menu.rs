use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandPaletteAgentMenuToggleRequest, CommandPaletteBranchMenuToggleRequest,
    CommandPaletteMenuActivateRequest, CommandPaletteMenuDismissRequest,
    CommandPaletteMenuHighlightRequest, CommandPaletteMenuMoveRequest,
    CommandPaletteModelMenuToggleRequest, CommandPalettePermissionMenuToggleRequest,
    CommandPaletteProjectMenuToggleRequest, StartGoToBranch, StartSelectMode, StartSelectModel,
    StartSelectWorkspace,
};

use crate::{BindCommands, CommandDispatch, CommandRegistry};

use super::{PaletteDraftInput, PaletteOpen, PaletteSnapshot};

pub(super) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            CommandPaletteAgentMenuToggleRequest,
            CommandPaletteModelMenuToggleRequest,
            CommandPalettePermissionMenuToggleRequest,
            CommandPaletteProjectMenuToggleRequest,
            CommandPaletteBranchMenuToggleRequest,
            CommandPaletteMenuMoveRequest,
            CommandPaletteMenuHighlightRequest,
            CommandPaletteMenuActivateRequest,
            CommandPaletteMenuDismissRequest,
        )>::default())
            .add_observer(toggle_agent)
            .add_observer(toggle_model)
            .add_observer(toggle_permission)
            .add_observer(toggle_project)
            .add_observer(toggle_branch)
            .add_observer(navigate)
            .add_observer(highlight)
            .add_observer(activate)
            .add_observer(dismiss_input)
            .add_observer(move_cursor)
            .add_observer(choose)
            .add_observer(dismiss)
            .add_systems(Startup, bind.in_set(BindCommands));
    }
}

#[derive(Component)]
pub(super) struct AgentMenuOpen;

#[derive(Component)]
pub(super) struct ModelMenuOpen;

#[derive(Component)]
pub(super) struct PermissionMenuOpen;

#[derive(Component)]
pub(super) struct ProjectMenuOpen;

#[derive(Component)]
pub(super) struct BranchMenuOpen;

#[derive(Component, Default)]
pub(super) struct PaletteMenuCursor(pub(super) usize);

impl PaletteMenuCursor {
    fn step(&mut self, next: bool, rows: usize) {
        if rows == 0 {
            return;
        }
        let at = self.0.min(rows - 1);
        self.0 = if next {
            (at + 1).min(rows - 1)
        } else {
            at.saturating_sub(1)
        };
    }

    fn highlight(&mut self, index: usize, rows: usize) {
        if index < rows {
            self.0 = index;
        }
    }
}

pub(super) type OpenMenu = (
    AgentMenuOpen,
    ModelMenuOpen,
    PermissionMenuOpen,
    ProjectMenuOpen,
    BranchMenuOpen,
    PaletteMenuCursor,
);

type MenuCursorRow = (
    &'static PaletteOpen,
    &'static PaletteSnapshot,
    &'static mut PaletteMenuCursor,
    Has<AgentMenuOpen>,
    Has<ModelMenuOpen>,
    Has<PermissionMenuOpen>,
    Has<ProjectMenuOpen>,
    Has<BranchMenuOpen>,
);

type MenuActivationRow = (
    &'static PaletteOpen,
    &'static mut PaletteDraftInput,
    &'static PaletteSnapshot,
    Has<AgentMenuOpen>,
    Has<ModelMenuOpen>,
    Has<PermissionMenuOpen>,
    Has<ProjectMenuOpen>,
    Has<BranchMenuOpen>,
);

#[vmux_command::command]
struct CommandBarMenuNextBinding;

#[vmux_command::command]
struct CommandBarMenuPreviousBinding;

#[vmux_command::command]
struct CommandBarMenuChooseBinding;

#[vmux_command::command]
struct CommandBarMenuDismissBinding;

fn bind(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<CommandBarMenuNextBinding>(&mut commands);
    registry.bind::<CommandBarMenuPreviousBinding>(&mut commands);
    registry.bind::<CommandBarMenuChooseBinding>(&mut commands);
    registry.bind::<CommandBarMenuDismissBinding>(&mut commands);
}

fn toggle_agent(
    trigger: On<UiInput<CommandPaletteAgentMenuToggleRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteSnapshot, Has<AgentMenuOpen>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, snapshot, active)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let mut entity = commands.entity(target);
    entity.remove::<OpenMenu>();
    if !active {
        let composer = &snapshot.0.projection.composer;
        let cursor = composer
            .agents
            .iter()
            .position(|agent| agent.url == composer.agent_url)
            .unwrap_or(0);
        entity.insert((AgentMenuOpen, PaletteMenuCursor(cursor)));
    }
}

fn toggle_model(
    trigger: On<UiInput<CommandPaletteModelMenuToggleRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteSnapshot, Has<ModelMenuOpen>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, snapshot, active)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let mut entity = commands.entity(target);
    entity.remove::<OpenMenu>();
    if !active {
        let composer = &snapshot.0.projection.composer;
        let cursor = composer
            .model_options
            .iter()
            .position(|model| model.id == composer.model_current_id)
            .unwrap_or(0);
        entity.insert((ModelMenuOpen, PaletteMenuCursor(cursor)));
    }
}

fn toggle_permission(
    trigger: On<UiInput<CommandPalettePermissionMenuToggleRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteSnapshot, Has<PermissionMenuOpen>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, snapshot, active)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let mut entity = commands.entity(target);
    entity.remove::<OpenMenu>();
    if !active {
        let composer = &snapshot.0.projection.composer;
        let cursor = composer
            .permission_modes
            .iter()
            .position(|mode| mode.id == composer.permission_current_id)
            .unwrap_or(0);
        entity.insert((PermissionMenuOpen, PaletteMenuCursor(cursor)));
    }
}

fn toggle_project(
    trigger: On<UiInput<CommandPaletteProjectMenuToggleRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteSnapshot, Has<ProjectMenuOpen>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, snapshot, active)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let mut entity = commands.entity(target);
    entity.remove::<OpenMenu>();
    if !active {
        let cursor = snapshot
            .0
            .projection
            .composer
            .projects
            .iter()
            .filter(|project| project.depth == 0)
            .position(|project| project.is_active)
            .unwrap_or(0);
        entity.insert((ProjectMenuOpen, PaletteMenuCursor(cursor)));
    }
}

fn toggle_branch(
    trigger: On<UiInput<CommandPaletteBranchMenuToggleRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteSnapshot, Has<BranchMenuOpen>)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, snapshot, active)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let mut entity = commands.entity(target);
    entity.remove::<OpenMenu>();
    if !active {
        let composer = &snapshot.0.projection.composer;
        let cursor = snapshot
            .0
            .branches
            .iter()
            .position(|branch| branch.branch == composer.branch_label)
            .unwrap_or(0);
        entity.insert((BranchMenuOpen, PaletteMenuCursor(cursor)));
    }
}

fn navigate(
    trigger: On<UiInput<CommandPaletteMenuMoveRequest>>,
    mut palettes: Query<MenuCursorRow>,
) {
    let Ok((opened, snapshot, mut cursor, agent, model, permission, project, branch)) =
        palettes.get_mut(trigger.event().webview)
    else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let composer = &snapshot.0.projection.composer;
    let rows = if agent {
        composer.agents.len()
    } else if model {
        composer.model_options.len()
    } else if permission {
        composer.permission_modes.len()
    } else if project {
        composer
            .projects
            .iter()
            .filter(|project| project.depth == 0)
            .count()
            + 1
    } else if branch {
        snapshot.0.branches.len()
    } else {
        return;
    };
    cursor.step(request.next, rows);
}

fn highlight(
    trigger: On<UiInput<CommandPaletteMenuHighlightRequest>>,
    mut palettes: Query<MenuCursorRow>,
) {
    let Ok((opened, snapshot, mut cursor, agent, model, permission, project, branch)) =
        palettes.get_mut(trigger.event().webview)
    else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let composer = &snapshot.0.projection.composer;
    let rows = if agent {
        composer.agents.len()
    } else if model {
        composer.model_options.len()
    } else if permission {
        composer.permission_modes.len()
    } else if project {
        composer
            .projects
            .iter()
            .filter(|project| project.depth == 0)
            .count()
            + 1
    } else if branch {
        snapshot.0.branches.len()
    } else {
        return;
    };
    cursor.highlight(request.index as usize, rows);
}

fn activate(
    trigger: On<UiInput<CommandPaletteMenuActivateRequest>>,
    mut palettes: Query<MenuActivationRow>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, snapshot, agent, model, permission, project, branch)) =
        palettes.get_mut(target)
    else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let index = request.index as usize;
    let composer = &snapshot.0.projection.composer;
    let handled = if agent {
        let Some(agent) = composer.agents.get(index) else {
            return;
        };
        input.target_url.clone_from(&agent.url);
        input.selected = 0;
        input.navigating = false;
        input.changed();
        true
    } else if model {
        let Some(model) = composer.model_options.get(index) else {
            return;
        };
        commands.trigger(UiInput {
            webview: target,
            payload: StartSelectModel {
                agent_key: composer.model_agent_key.clone(),
                model_id: model.id.clone(),
            },
        });
        true
    } else if permission {
        let Some(mode) = composer.permission_modes.get(index) else {
            return;
        };
        commands.trigger(UiInput {
            webview: target,
            payload: StartSelectMode {
                agent_key: composer.permission_agent_key.clone(),
                mode_id: mode.id.clone(),
            },
        });
        true
    } else if project {
        let projects = composer
            .projects
            .iter()
            .filter(|project| project.depth == 0);
        let count = projects.clone().count();
        if index == count {
            commands.trigger(UiInput {
                webview: target,
                payload: StartSelectWorkspace {
                    current_dir: composer.cwd.clone(),
                },
            });
            true
        } else {
            let Some(project) = projects.into_iter().nth(index) else {
                return;
            };
            commands.trigger(UiInput {
                webview: target,
                payload: StartGoToBranch {
                    project: project.path.clone(),
                    branch: String::new(),
                    checkout: String::new(),
                },
            });
            true
        }
    } else if branch {
        let Some(branch) = snapshot.0.branches.get(index) else {
            return;
        };
        commands.trigger(UiInput {
            webview: target,
            payload: StartGoToBranch {
                project: composer.project.clone(),
                branch: branch.branch.clone(),
                checkout: branch.checkout.clone(),
            },
        });
        true
    } else {
        false
    };
    if handled {
        commands.entity(target).remove::<OpenMenu>();
    }
}

fn dismiss_input(
    trigger: On<UiInput<CommandPaletteMenuDismissRequest>>,
    palettes: Query<&PaletteOpen>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok(opened) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    commands.entity(target).remove::<OpenMenu>();
}

fn move_cursor(
    trigger: On<CommandDispatch>,
    next: Query<(), With<CommandBarMenuNextBinding>>,
    previous: Query<(), With<CommandBarMenuPreviousBinding>>,
    palettes: Query<&PaletteOpen>,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let next = if next.contains(command) {
        true
    } else if previous.contains(command) {
        false
    } else {
        return;
    };
    let target = trigger.event().invocation().caller;
    let Ok(opened) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMenuMoveRequest {
            open_id: opened.0.open_id,
            next,
        },
    });
}

fn choose(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarMenuChooseBinding>>,
    palettes: Query<(&PaletteOpen, &PaletteMenuCursor)>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok((opened, menu)) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMenuActivateRequest {
            open_id: opened.0.open_id,
            index: menu.0 as u32,
        },
    });
}

fn dismiss(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarMenuDismissBinding>>,
    palettes: Query<&PaletteOpen>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(opened) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMenuDismissRequest {
            open_id: opened.0.open_id,
        },
    });
}
