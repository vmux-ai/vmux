use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteBranchesRequest, StartBranchesRequest,
    StartProjectBranches,
};
use vmux_core::host::UiStateWrite;
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};

use super::{OpenVersion, PaletteSnapshot};

pub(super) struct PaletteBranchPlugin;

impl Plugin for PaletteBranchPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(request_palette_branches)
            .add_observer(receive_palette_branches)
            .add_systems(PreUpdate, attach_palette_branch);
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteBranch {
    open: OpenVersion,
    desired: String,
    inflight: Option<BranchFlight>,
}

fn attach_palette_branch(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteBranch>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert(PaletteBranch::default());
    }
}

fn request_palette_branches(
    trigger: On<UiInput<CommandPaletteBranchesRequest>>,
    mut palettes: Query<(&mut PaletteBranch, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut branch, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = branch.open.accept(request.open_id) else {
        return;
    };
    if opened {
        branch.desired.clear();
        snapshot.0.open_id = request.open_id;
        snapshot.0.branch_project.clear();
        snapshot.0.branches.clear();
    }
    if branch.desired == request.project {
        return;
    }
    branch.desired.clone_from(&request.project);
    snapshot.0.branch_project.clear();
    snapshot.0.branches.clear();
    branch.request_branches(target, &mut commands);
}

fn receive_palette_branches(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PaletteBranch, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<StartProjectBranches>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let target = trigger.event().webview();
    let Ok((mut branch, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(flight) = branch.inflight.take() else {
        return;
    };
    if flight.open_generation == branch.open.generation()
        && branch.desired == flight.project
        && response.project == flight.project
    {
        snapshot.0.branch_project.clone_from(&response.project);
        snapshot.0.branches.clone_from(&response.branches);
    }
    branch.request_branches(target, &mut commands);
}

impl PaletteBranch {
    fn request_branches(&mut self, target: Entity, commands: &mut Commands) {
        if self.inflight.is_some() || self.desired.trim().is_empty() {
            return;
        }
        let project = self.desired.clone();
        self.inflight = Some(BranchFlight {
            open_generation: self.open.generation(),
            project: project.clone(),
        });
        commands.trigger(UiInput {
            webview: target,
            payload: StartBranchesRequest { project },
        });
    }
}

struct BranchFlight {
    open_generation: u64,
    project: String,
}
