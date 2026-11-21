use std::collections::BTreeSet;

use bevy::prelude::*;
use vmux_ecs::{EntityTarget, JobRequest, JobResult, OperationState};

use crate::io::Execute;
use crate::observation::{
    Accepted, Finished, Observation, ObserveProviderRequest, ObservedPackage, ObservedTool,
};
use crate::state::ToolStatus;
use crate::{ToolProviderId, ToolTaskSet};

pub(crate) struct HomebrewInventoryPlugin;

impl Plugin for HomebrewInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(observe).add_systems(
            Update,
            (list, outdated, publish).chain().in_set(ToolTaskSet::Start),
        );
    }
}

#[derive(Component)]
struct Listing;

#[derive(Component)]
struct Refresh;

#[derive(Component)]
struct Listed(String);

#[derive(Component)]
struct Outdated;

#[derive(Component)]
struct OutdatedNames(String);

#[derive(Component)]
struct Ready;

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<&ToolProviderId>,
    observations: Query<&OperationState, With<Observation>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok(provider) = providers.get(request.entity) else {
        return;
    };
    if !matches!(provider.0.id(), "homebrew-formula" | "homebrew-cask")
        || observations.get(request.observation).ok() != Some(&OperationState::Running)
    {
        return;
    }
    let kind = if provider.0.id() == "homebrew-cask" {
        "--cask"
    } else {
        "--formula"
    };
    let mut entity = commands.entity(request.observation);
    entity.insert((
        Accepted,
        Listing,
        JobRequest(Execute {
            program: "brew".into(),
            arguments: vec!["list".into(), kind.into(), "--versions".into()],
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(60),
        }),
    ));
    if request.refresh_catalogs {
        entity.insert(Refresh);
    }
}

fn list(
    requests: Query<
        (
            Entity,
            &JobResult<Execute>,
            &OperationState,
            Has<Refresh>,
            &EntityTarget<ToolProviderId>,
        ),
        With<Listing>,
    >,
    providers: Query<&ToolProviderId>,
    mut commands: Commands,
) {
    for (entity, result, state, refresh, target) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Listing, JobResult<Execute>)>();
        let output = match &result.0 {
            Ok(Ok(output)) => output,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Finished {
                    error: Some(error.clone()),
                });
                continue;
            }
        };
        let listed = match output {
            Some(output) if output.0.status.success() => {
                String::from_utf8_lossy(&output.0.stdout).into_owned()
            }
            Some(output) => {
                let detail = String::from_utf8_lossy(&output.0.stderr).trim().to_string();
                let error = if detail.is_empty() {
                    format!("brew exited with {}", output.0.status)
                } else {
                    detail
                };
                commands
                    .entity(entity)
                    .insert(Finished { error: Some(error) });
                continue;
            }
            None => String::new(),
        };
        commands.entity(entity).insert(Listed(listed));
        if refresh && output.is_some() {
            let Ok(provider) = providers.get(target.entity()) else {
                continue;
            };
            let kind = if provider.0.id() == "homebrew-cask" {
                "--cask"
            } else {
                "--formula"
            };
            commands.entity(entity).insert((
                Outdated,
                JobRequest(Execute {
                    program: "brew".into(),
                    arguments: vec!["outdated".into(), kind.into()],
                    environment: Vec::new(),
                    timeout: std::time::Duration::from_secs(60),
                }),
            ));
        } else {
            commands.entity(entity).insert(Ready);
        }
    }
}

fn outdated(
    requests: Query<(Entity, &JobResult<Execute>, &OperationState), With<Outdated>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let names = match &result.0 {
            Ok(Ok(Some(output))) => String::from_utf8_lossy(&output.0.stdout).into_owned(),
            _ => String::new(),
        };
        commands
            .entity(entity)
            .remove::<(Outdated, JobResult<Execute>)>()
            .insert((OutdatedNames(names), Ready));
    }
}

fn publish(
    requests: Query<
        (
            Entity,
            &Listed,
            Option<&OutdatedNames>,
            &OperationState,
            &EntityTarget<ToolProviderId>,
        ),
        (With<Ready>, Without<Finished>),
    >,
    providers: Query<&ToolProviderId>,
    mut commands: Commands,
) {
    for (entity, listed, outdated, state, target) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let Ok(provider) = providers.get(target.entity()) else {
            continue;
        };
        let cask = provider.0.id() == "homebrew-cask";
        let mut outdated_names = BTreeSet::new();
        if let Some(outdated) = outdated {
            for line in outdated.0.lines() {
                if let Some(name) = line.split_whitespace().next() {
                    outdated_names.insert(name);
                }
            }
        }
        for line in listed.0.lines() {
            let mut fields = line.split_whitespace();
            let Some(name) = fields.next() else {
                continue;
            };
            let version = fields.collect::<Vec<_>>().join(" ");
            let status = if outdated_names.contains(name) {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            let detail = if cask {
                "Homebrew cask"
            } else {
                "Homebrew formula"
            };
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        name.into(),
                        name.into(),
                        (!version.is_empty()).then_some(version),
                        None,
                        detail.into(),
                        status,
                        Vec::new(),
                    ),
                    !cask || name != "vmux",
                ),
                ChildOf(entity),
            ));
        }
        commands
            .entity(entity)
            .remove::<(Listed, OutdatedNames, Ready, Refresh)>()
            .insert(Finished { error: None });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use bevy::prelude::{App, Commands, Entity, Plugin, Query, Update};
    use std::os::unix::process::ExitStatusExt;

    struct ProcessFixturePlugin;

    impl Plugin for ProcessFixturePlugin {
        fn build(&self, app: &mut App) {
            if !app.is_plugin_added::<vmux_ecs::JobPlugin<Execute>>() {
                app.add_plugins(vmux_ecs::JobPlugin::<Execute>::default());
            }
            app.add_systems(
                Update,
                output
                    .after(ToolTaskSet::Start)
                    .before(vmux_ecs::JobSet::Submit),
            );
        }
    }

    fn output(requests: Query<(Entity, &JobRequest<Execute>)>, mut commands: Commands) {
        for (entity, request) in &requests {
            if request.0.program != "brew" {
                continue;
            }
            let cask = request
                .0
                .arguments
                .iter()
                .any(|argument| argument == "--cask");
            let outdated = request
                .0
                .arguments
                .first()
                .is_some_and(|argument| argument == "outdated");
            let source = match (cask, outdated) {
                (true, false) => "vmux 1\npackage 1\n",
                (true, true) => "package\n",
                (false, false) => "ripgrep 14.1.1\nopenssl@3 3.5.0 3.5.1\n",
                (false, true) => "ripgrep\n",
            };
            commands
                .entity(entity)
                .remove::<JobRequest<Execute>>()
                .insert(JobResult::<Execute>(Ok(Ok(Some(
                    crate::process::ProcessOutput(std::process::Output {
                        status: std::process::ExitStatus::from_raw(0),
                        stdout: source.as_bytes().to_vec(),
                        stderr: Vec::new(),
                    }),
                )))));
        }
    }

    #[test]
    fn brew_observations_keep_formula_cask_identity_and_protect_the_vmux_cask() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::ToolStore::new(directory.path(), directory.path());
        let mut original = crate::ToolsManifest::default();
        original.set_package("homebrew-cask", "missing-cask", true);
        original.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            crate::ToolRuntimePlugin,
            crate::homebrew::HomebrewToolPlugin,
            ProcessFixturePlugin,
        ));
        let registry = app
            .world_mut()
            .spawn((
                crate::inventory::Registry::default(),
                crate::catalog::Catalog::default(),
                store.clone(),
                crate::ToolsManifest::default(),
                crate::inventory::Sequence::default(),
            ))
            .id();
        for provider in ["homebrew-cask", "homebrew-formula"] {
            app.world_mut().spawn((
                ToolProviderId(crate::state::ToolProvider::new(provider)),
                crate::observation::Handler,
                crate::observation::PackageProvider,
            ));
        }
        app.world_mut().trigger(crate::inventory::RefreshRequest {
            entity: registry,
            refresh_catalogs: true,
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !app
            .world()
            .get::<crate::catalog::Catalog>(registry)
            .unwrap()
            .loaded
        {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "Brew observations did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut state =
            bevy::ecs::system::SystemState::<crate::catalog::Inventory>::new(app.world_mut());
        let snapshot = state.get(app.world()).unwrap().snapshot(registry).unwrap();
        assert_eq!(snapshot.installed, 4);
        assert_eq!(snapshot.updates, 2);
        let casks = snapshot
            .categories
            .iter()
            .find(|category| category.provider.id() == "homebrew-cask")
            .unwrap();
        let vmux = casks.items.iter().find(|item| item.id == "vmux").unwrap();
        assert!(
            !vmux
                .operations
                .contains(&crate::state::ToolOperationKind::Uninstall)
        );
        assert_eq!(
            casks
                .items
                .iter()
                .find(|item| item.id == "missing-cask")
                .unwrap()
                .status,
            ToolStatus::Missing
        );
        let saved = crate::ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(saved.packages["homebrew-formula"], ["openssl@3", "ripgrep"]);
        let formulae = snapshot
            .categories
            .iter()
            .find(|category| category.provider.id() == "homebrew-formula")
            .unwrap();
        assert_eq!(
            formulae
                .items
                .iter()
                .find(|item| item.id == "openssl@3")
                .unwrap()
                .version
                .as_deref(),
            Some("3.5.0 3.5.1")
        );
        assert_eq!(
            saved.packages["homebrew-cask"],
            ["missing-cask", "package", "vmux"]
        );
        assert!(store.lease().is_some());
    }
}
