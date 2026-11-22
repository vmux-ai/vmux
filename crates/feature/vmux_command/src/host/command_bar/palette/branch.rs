use bevy::prelude::*;
use bevy::prelude::{Changed, Commands};
use bevy_cef::prelude::UiInput;
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, StartBranchesRequest, StartProjectBranches,
};
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::{HostsLauncher, RendersLauncherPanel};

use super::{OpenVersion, PaletteContext, PaletteSnapshot};

pub(super) struct PaletteBranchPlugin;

impl Plugin for PaletteBranchPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(receive_branches)
            .add_observer(apply)
            .add_systems(PreUpdate, attach)
            .add_systems(
                PostUpdate,
                (update, request_branches).chain().before(super::project),
            );
    }
}

#[derive(Component, Default)]
#[require(BranchRevision)]
pub(super) struct PaletteBranch {
    open: OpenVersion,
    desired: String,
    loaded: String,
    inflight: Option<BranchFlight>,
}

#[derive(Component, Default)]
struct BranchRevision(u64);

type NewBranchPages<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
        Without<PaletteBranch>,
    ),
>;

fn attach(pages: NewBranchPages, mut commands: Commands) {
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
            branch.inflight = None;
            branch.desired.clear();
            branch.loaded.clear();
            snapshot.open_id = context.open_id;
            snapshot.branch_project.clear();
            snapshot.branches.clear();
        }
        if branch.desired != context.project {
            branch.inflight = None;
            branch.desired.clone_from(&context.project);
            branch.loaded.clear();
            snapshot.branch_project.clear();
            snapshot.branches.clear();
        }
    }
}

fn receive_branches(trigger: On<UiStateWrite<CommandBarUiState>>, mut commands: Commands) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<StartProjectBranches>>::payload(
            trigger.event().update(),
        )
    else {
        return;
    };
    commands.trigger(BranchesReceived {
        page: trigger.event().webview(),
        response: response.clone(),
    });
}

#[derive(EntityEvent)]
struct BranchesReceived {
    #[event_target]
    page: Entity,
    response: StartProjectBranches,
}

fn apply(
    trigger: On<BranchesReceived>,
    mut palettes: Query<(&mut PaletteBranch, &mut PaletteSnapshot)>,
) {
    let response = &trigger.event().response;
    let Ok((mut branch, mut snapshot)) = palettes.get_mut(trigger.event().page) else {
        return;
    };
    let Some(flight) = branch.inflight.as_ref() else {
        return;
    };
    if flight.open_generation == branch.open.generation()
        && branch.desired == flight.project
        && response.project == flight.project
        && response.request_id == flight.request_id
    {
        let project = flight.project.clone();
        snapshot.branch_project.clone_from(&response.project);
        snapshot.branches.clone_from(&response.branches);
        branch.loaded = project;
        branch.inflight = None;
    }
}

fn request_branches(
    mut palettes: Query<(Entity, &mut PaletteBranch, &mut BranchRevision)>,
    mut commands: Commands,
) {
    for (target, mut branch, mut revision) in &mut palettes {
        if branch.inflight.is_some()
            || branch.desired.trim().is_empty()
            || branch.desired == branch.loaded
        {
            continue;
        }
        let project = branch.desired.clone();
        revision.0 = revision.0.wrapping_add(1).max(1);
        branch.inflight = Some(BranchFlight {
            open_generation: branch.open.generation(),
            project: project.clone(),
            request_id: revision.0,
        });
        commands.trigger(UiInput {
            webview: target,
            payload: StartBranchesRequest {
                project,
                request_id: revision.0,
            },
        });
    }
}

struct BranchFlight {
    open_generation: u64,
    project: String,
    request_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::OpenId;
    use vmux_api::space::ProjectBranch;

    #[derive(Resource, Default)]
    struct Requests(Vec<(Entity, StartBranchesRequest)>);

    impl Requests {
        fn capture(event: On<UiInput<StartBranchesRequest>>, mut requests: ResMut<Self>) {
            requests
                .0
                .push((event.event().webview, event.event().payload.clone()));
        }
    }

    struct PalettePages(App);

    impl PalettePages {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PaletteBranchPlugin))
                .init_resource::<Requests>()
                .add_observer(Requests::capture);
            Self(app)
        }

        fn open(&mut self, page: Entity, open_id: u64) {
            self.open_project(page, open_id, "/project");
        }

        fn open_project(&mut self, page: Entity, open_id: u64, project: &str) {
            self.0.world_mut().entity_mut(page).insert(PaletteContext {
                open_id: OpenId(open_id),
                project: project.to_owned(),
                ..Default::default()
            });
            self.0.update();
        }

        fn requested(&self) -> StartBranchesRequest {
            self.0
                .world()
                .resource::<Requests>()
                .0
                .last()
                .unwrap()
                .1
                .clone()
        }

        fn respond(&mut self, page: Entity, request: &StartBranchesRequest, branch: &str) {
            self.0
                .world_mut()
                .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                    page,
                    &StartProjectBranches {
                        project: request.project.clone(),
                        request_id: request.request_id,
                        branches: vec![ProjectBranch {
                            branch: branch.into(),
                            ..Default::default()
                        }],
                    },
                ));
            self.0.world_mut().flush();
        }
    }

    #[test]
    fn a_project_path_with_significant_space_completes_its_palette_flight() {
        let mut pages = PalettePages::new();
        let page = pages
            .0
            .world_mut()
            .spawn((PaletteBranch::default(), PaletteSnapshot::default()))
            .id();
        pages.open_project(page, 1, "/project ");
        let request = pages.requested();
        assert_eq!(request.project, "/project ");
        pages.respond(page, &request, "current");
        let state = pages.0.world().get::<PaletteSnapshot>(page).unwrap();
        assert_eq!(state.branch_project, "/project ");
        assert_eq!(state.branches[0].branch, "current");
    }

    #[test]
    fn an_old_same_project_response_cannot_consume_a_new_palette_flight() {
        let mut pages = PalettePages::new();
        let page = pages
            .0
            .world_mut()
            .spawn((PaletteBranch::default(), PaletteSnapshot::default()))
            .id();
        pages.open(page, 1);
        let first = pages.requested();
        pages.open(page, 2);
        let second = pages.requested();
        assert!(second.request_id > first.request_id);
        pages.respond(page, &first, "stale");
        assert!(
            pages
                .0
                .world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .branches
                .is_empty()
        );
        pages.respond(page, &second, "current");
        assert_eq!(
            pages
                .0
                .world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .branches[0]
                .branch,
            "current"
        );
    }

    #[test]
    fn recreated_palette_state_keeps_branch_request_identity_monotonic() {
        let mut pages = PalettePages::new();
        let page = pages
            .0
            .world_mut()
            .spawn((PaletteBranch::default(), PaletteSnapshot::default()))
            .id();
        pages.open(page, 1);
        let first = pages.requested();
        pages
            .0
            .world_mut()
            .entity_mut(page)
            .remove::<PaletteBranch>();
        pages
            .0
            .world_mut()
            .entity_mut(page)
            .insert(PaletteBranch::default());
        pages.open(page, 2);
        let second = pages.requested();
        assert!(second.request_id > first.request_id);
        pages.respond(page, &first, "old-document");
        pages.respond(page, &second, "new-document");
        assert_eq!(
            pages
                .0
                .world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .branches[0]
                .branch,
            "new-document"
        );
    }
}
