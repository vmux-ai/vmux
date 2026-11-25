use bevy::prelude::*;
use serde_json::Value;
use vmux_ecs::OperationState;
use vmux_job::{JobRequest, JobResult};

use crate::ToolTaskSet;
use crate::io::Execute;
use crate::npm::NpmProvider;
use crate::observation::{
    Accepted, Finished, Observation, ObserveProviderRequest, ObservedPackage, ObservedTool,
};
use crate::state::ToolStatus;

pub(crate) struct NpmInventoryPlugin;

impl Plugin for NpmInventoryPlugin {
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
struct Listed(Value);

#[derive(Component)]
struct Outdated;

#[derive(Component)]
struct OutdatedDocument(Value);

#[derive(Component)]
struct Ready;

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<(), With<NpmProvider>>,
    observations: Query<&OperationState, With<Observation>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !providers.contains(request.entity)
        || observations.get(request.observation).ok() != Some(&OperationState::Running)
    {
        return;
    }
    let mut entity = commands.entity(request.observation);
    entity.insert((
        Accepted,
        Listing,
        JobRequest(Execute {
            program: "npm".into(),
            arguments: vec![
                "list".into(),
                "--global".into(),
                "--depth=0".into(),
                "--json".into(),
            ],
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(60),
        }),
    ));
    if request.refresh_catalogs {
        entity.insert(Refresh);
    }
}

fn list(
    requests: Query<(Entity, &JobResult<Execute>, &OperationState, Has<Refresh>), With<Listing>>,
    mut commands: Commands,
) {
    for (entity, result, state, refresh) in &requests {
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
        let document = match output {
            Some(output) => {
                if output.0.stdout.is_empty() && !output.0.status.success() {
                    let detail = String::from_utf8_lossy(&output.0.stderr).trim().to_string();
                    let error = if detail.is_empty() {
                        format!("npm exited with {}", output.0.status)
                    } else {
                        detail
                    };
                    commands
                        .entity(entity)
                        .insert(Finished { error: Some(error) });
                    continue;
                }
                match serde_json::from_slice(&output.0.stdout) {
                    Ok(document) => document,
                    Err(error) => {
                        commands.entity(entity).insert(Finished {
                            error: Some(error.to_string()),
                        });
                        continue;
                    }
                }
            }
            None => Value::Null,
        };
        commands.entity(entity).insert(Listed(document));
        if refresh && output.is_some() {
            commands.entity(entity).insert((
                Outdated,
                JobRequest(Execute {
                    program: "npm".into(),
                    arguments: vec!["outdated".into(), "--global".into(), "--json".into()],
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
        let document = match &result.0 {
            Ok(Ok(Some(output))) => serde_json::from_slice(&output.0.stdout).unwrap_or(Value::Null),
            _ => Value::Null,
        };
        commands
            .entity(entity)
            .remove::<(Outdated, JobResult<Execute>)>()
            .insert((OutdatedDocument(document), Ready));
    }
}

fn publish(
    requests: Query<
        (Entity, &Listed, Option<&OutdatedDocument>, &OperationState),
        (With<Ready>, Without<Finished>),
    >,
    mut commands: Commands,
) {
    for (entity, listed, outdated, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        if let Some(packages) = listed.0.get("dependencies").and_then(Value::as_object) {
            for (id, metadata) in packages {
                let status = if outdated.is_some_and(|outdated| outdated.0.get(id).is_some()) {
                    ToolStatus::Outdated
                } else {
                    ToolStatus::Installed
                };
                let version = metadata
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                commands.spawn((
                    ObservedPackage::new(
                        ObservedTool::new(
                            id.clone(),
                            id.clone(),
                            version,
                            None,
                            "Global NPM package".into(),
                            status,
                            Vec::new(),
                        ),
                        true,
                    ),
                    ChildOf(entity),
                ));
            }
        }
        commands
            .entity(entity)
            .remove::<(Listed, OutdatedDocument, Ready, Refresh)>()
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
            if !app.is_plugin_added::<vmux_job::JobPlugin<Execute>>() {
                app.add_plugins(vmux_job::JobPlugin::<Execute>::default());
            }
            app.add_systems(
                Update,
                output
                    .after(ToolTaskSet::Start)
                    .before(vmux_job::JobSet::Submit),
            );
        }
    }

    fn output(requests: Query<(Entity, &JobRequest<Execute>)>, mut commands: Commands) {
        for (entity, request) in &requests {
            if request.0.program != "npm" {
                continue;
            }
            let outdated = request
                .0
                .arguments
                .first()
                .is_some_and(|argument| argument == "outdated");
            let stdout = if outdated {
                br#"{"@scope/tool":{"current":"2.0.0","latest":"3.0.0"},"beta":{"current":"1","latest":"2"}}"#.to_vec()
            } else {
                br#"{"dependencies":{"@scope/tool":{"version":"2.0.0"},"alpha":{"version":"1"},"beta":{"version":"1"}}}"#.to_vec()
            };
            let status = std::process::ExitStatus::from_raw(if outdated { 256 } else { 0 });
            commands
                .entity(entity)
                .remove::<JobRequest<Execute>>()
                .insert(JobResult::<Execute>(Ok(Ok(Some(
                    crate::process::ProcessOutput(std::process::Output {
                        status,
                        stdout,
                        stderr: Vec::new(),
                    }),
                )))));
        }
    }

    #[test]
    fn native_npm_observations_reconcile_updates_and_preserve_unobserved_declarations() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::ToolStore::new(directory.path(), directory.path());
        let mut manifest = crate::ToolsManifest::default();
        manifest.set_package("npm", "declared", true);
        manifest.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            crate::ToolRuntimePlugin,
            crate::npm::NpmToolPlugin,
            ProcessFixturePlugin,
        ));
        let registry = app
            .world_mut()
            .spawn((
                crate::tool_inventory::Registry::default(),
                crate::catalog::Catalog::default(),
                store.clone(),
                crate::ToolsManifest::default(),
                crate::tool_inventory::Sequence::default(),
            ))
            .id();
        app.world_mut().spawn((
            NpmProvider,
            crate::ToolProviderId(crate::state::ToolProvider::new("npm")),
            crate::observation::Handler,
            crate::observation::PackageProvider,
        ));
        app.world_mut()
            .trigger(crate::tool_inventory::RefreshRequest {
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
                "NPM observation did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut view =
            bevy::ecs::system::SystemState::<crate::catalog::Inventory>::new(app.world_mut());
        let snapshot = view.get(app.world()).unwrap().snapshot(registry).unwrap();
        let items = &snapshot.categories[0].items;
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["@scope/tool", "alpha", "beta", "declared"]
        );
        assert_eq!(items[0].status, ToolStatus::Outdated);
        assert_eq!(items[0].version.as_deref(), Some("2.0.0"));
        assert_eq!(items[1].status, ToolStatus::Installed);
        assert_eq!(items[2].status, ToolStatus::Outdated);
        assert_eq!(items[3].status, ToolStatus::Missing);
        assert!(items.iter().all(|item| item.managed));
        assert_eq!(snapshot.updates, 2);
        let saved = crate::ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(
            saved.packages["npm"],
            ["@scope/tool", "alpha", "beta", "declared"]
        );
        assert!(store.lease().is_some());
    }
}
