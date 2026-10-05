use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, StartBranchesRequest, StartProjectBranches,
};
use vmux_ecs::UiStateWrite;

use super::{NewPalette, OpenVersion, PaletteContext, PaletteSnapshot};

pub(super) struct PaletteBranchPlugin;

impl Plugin for PaletteBranchPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(receive_branches)
            .add_systems(PreUpdate, attach)
            .add_systems(
                PostUpdate,
                (update, request_branches).chain().before(super::project),
            );
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteBranch {
    open: OpenVersion,
    desired: String,
    loaded: String,
    inflight: Option<BranchFlight>,
}

fn attach(pages: Query<Entity, NewPalette<PaletteBranch>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert(PaletteBranch::default());
    }
}

fn update(
    mut palettes: Query<
        (
            Entity,
            &PaletteContext,
            &mut PaletteBranch,
            &mut PaletteSnapshot,
        ),
        Changed<PaletteContext>,
    >,
) {
    for (_, context, mut branch, mut snapshot) in &mut palettes {
        let Some(opened) = branch.open.accept(context.open_id) else {
            continue;
        };
        if opened {
            branch.desired.clear();
            branch.loaded.clear();
            snapshot.0.open_id = context.open_id;
            snapshot.0.branch_project.clear();
            snapshot.0.branches.clear();
        }
        if branch.desired != context.project {
            branch.desired.clone_from(&context.project);
            branch.loaded.clear();
            snapshot.0.branch_project.clear();
            snapshot.0.branches.clear();
        }
    }
}

fn receive_branches(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PaletteBranch, &mut PaletteSnapshot)>,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<StartProjectBranches>>::payload(
            trigger.event().update(),
        )
    else {
        return;
    };
    let Ok((mut branch, mut snapshot)) = palettes.get_mut(trigger.event().webview()) else {
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
        branch.loaded = flight.project;
    }
}

fn request_branches(mut palettes: Query<(Entity, &mut PaletteBranch)>, mut commands: Commands) {
    for (target, mut branch) in &mut palettes {
        if branch.inflight.is_some()
            || branch.desired.trim().is_empty()
            || branch.desired == branch.loaded
        {
            continue;
        }
        let project = branch.desired.clone();
        branch.inflight = Some(BranchFlight {
            open_generation: branch.open.generation(),
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
