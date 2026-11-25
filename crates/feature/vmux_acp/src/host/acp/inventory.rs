use std::collections::BTreeSet;

use bevy::prelude::*;
use vmux_ecs::profile::{CurrentProfile, ProfilePaths};
use vmux_ecs::{EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState};
use vmux_editor::lsp::package_path::PackageName;
use vmux_editor::lsp::store::Receipt;
use vmux_tool::ToolProviderId;
use vmux_tool::io::{ReadDirectory, ReadFile};
use vmux_tool::observation::{
    Accepted, Finished, Observation, ObserveProviderRequest, ObservedPackage, ObservedTool,
};
use vmux_tool::state::ToolStatus;

use super::registry::{Catalog, RefreshRequest, RegistryAgent};

pub(super) struct AcpInventoryPlugin;

impl Plugin for AcpInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(observe).add_systems(
            Update,
            (
                directories,
                receipts,
                publish_receipts,
                ApplyDeferred,
                publish,
            )
                .chain()
                .in_set(vmux_tool::observation::ObservationSet),
        );
    }
}

#[derive(Component)]
struct Listing;
#[derive(Component)]
struct Reading;
#[derive(Component)]
struct ReadDone;
#[derive(Component)]
struct ReceiptsPublished;
#[derive(Component)]
struct Expected(PackageName);
#[derive(Component)]
struct WaitingCatalog(u64);

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<&ToolProviderId>,
    observations: Query<&OperationState, With<Observation>>,
    catalogs: Query<&Catalog>,
    profile: CurrentProfile,
    mut refresh: MessageWriter<RefreshRequest>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !providers
        .get(request.entity)
        .is_ok_and(|provider| provider.0.id() == "acp")
        || observations.get(request.observation).ok() != Some(&OperationState::Running)
    {
        return;
    }
    let Ok(catalog) = catalogs.single() else {
        commands.entity(request.observation).insert((
            Accepted,
            Finished {
                error: Some("ACP registry is unavailable".into()),
            },
        ));
        return;
    };
    let path = profile
        .paths()
        .map(|paths| paths.agents())
        .unwrap_or_else(|| ProfilePaths::current().agents())
        .join("packages");
    let mut observation = commands.entity(request.observation);
    observation.insert((
        Accepted,
        Listing,
        JobRequest(ReadDirectory { path, limit: 4096 }),
    ));
    if request.refresh_catalogs {
        observation.insert(WaitingCatalog(catalog.revision));
        refresh.write(RefreshRequest);
    }
}

fn directories(
    requests: Query<(Entity, &OperationState, &JobResult<ReadDirectory>), With<Listing>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Listing, JobResult<ReadDirectory>)>()
            .insert(Reading);
        let entries = match &result.0 {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Finished {
                    error: Some(error.clone()),
                });
                continue;
            }
        };
        if entries.exists && !entries.is_directory {
            commands.entity(entity).insert(Finished {
                error: Some("ACP package root is not a directory".into()),
            });
            continue;
        }
        for entry in &entries.entries {
            if !entry.file_type.is_dir() {
                continue;
            }
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            let Ok(name) = PackageName::parse(name) else {
                continue;
            };
            commands.spawn((
                Name::new("ACP package receipt"),
                Expected(name),
                ChildOf(entity),
                EntityTarget::<Observation>::new(entity),
                OperationRetained,
                Operation::bundle("acp.receipt").unwrap().running(),
                JobRequest(ReadFile {
                    path: entry.path.join("vmux-receipt.json"),
                    limit: 1024 * 1024,
                }),
            ));
        }
    }
}

fn receipts(
    requests: Query<(Entity, &Expected, &OperationState, &JobResult<ReadFile>), Without<ReadDone>>,
    mut commands: Commands,
) {
    for (entity, expected, state, result) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<JobResult<ReadFile>>()
            .insert((ReadDone, OperationState::Succeeded));
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => bytes.0.as_ref(),
            _ => None,
        };
        if let Some(bytes) = bytes
            && let Ok(receipt) = serde_json::from_slice::<Receipt>(bytes)
            && receipt.name == expected.0
            && receipt.source_id.starts_with("acp:")
        {
            commands.entity(entity).insert(receipt);
        }
    }
}

fn publish_receipts(
    observations: Query<
        (Entity, &OperationState, Option<&WaitingCatalog>),
        (With<Reading>, Without<ReceiptsPublished>, Without<Finished>),
    >,
    pending: Query<(&ChildOf, Has<ReadDone>), With<Expected>>,
    receipts: Query<(&Receipt, &ChildOf)>,
    catalogs: Query<(Entity, &Catalog)>,
    agents: Query<(&RegistryAgent, Option<&ChildOf>)>,
    mut commands: Commands,
) {
    let Ok((catalog_entity, catalog)) = catalogs.single() else {
        return;
    };
    for (entity, state, waiting) in &observations {
        if *state != OperationState::Running
            || !catalog.ready
            || waiting.is_some_and(|waiting| catalog.revision <= waiting.0)
            || pending
                .iter()
                .any(|(parent, done)| parent.parent() == entity && !done)
        {
            continue;
        }
        for (receipt, parent) in &receipts {
            if parent.parent() != entity {
                continue;
            }
            let agent = agents.iter().find_map(|(agent, owner)| {
                (owner.is_none_or(|owner| owner.parent() == catalog_entity)
                    && agent.id == receipt.name.as_str())
                .then_some(agent)
            });
            let latest = agent.and_then(|agent| agent.version.as_ref());
            let status = if receipt.version.is_some()
                && latest.is_some()
                && receipt.version.as_ref() != latest
            {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            let name = agent
                .map(|agent| agent.name.as_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| receipt.name.as_str())
                .to_string();
            let detail = agent
                .and_then(|agent| agent.description.clone())
                .unwrap_or_else(|| "ACP agent".into());
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        receipt.name.as_str().to_string(),
                        name,
                        receipt.version.clone(),
                        agent.and_then(|agent| agent.icon.clone()),
                        detail,
                        status,
                        Vec::new(),
                    ),
                    true,
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(ReceiptsPublished);
    }
}

fn publish(
    observations: Query<
        (Entity, &OperationState),
        (With<Reading>, With<ReceiptsPublished>, Without<Finished>),
    >,
    receipts: Query<(&Receipt, &ChildOf)>,
    catalogs: Query<(Entity, &Catalog)>,
    agents: Query<(&RegistryAgent, Option<&ChildOf>)>,
    mut commands: Commands,
) {
    let Ok((catalog_entity, _)) = catalogs.single() else {
        return;
    };
    for (entity, state) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        let mut installed = BTreeSet::new();
        for (receipt, parent) in &receipts {
            if parent.parent() == entity {
                installed.insert(receipt.name.as_str());
            }
        }
        for (agent, owner) in &agents {
            if owner.is_some_and(|owner| owner.parent() != catalog_entity)
                || installed.contains(agent.id.as_str())
                || PackageName::parse(&agent.id).is_err()
                || !agent.installable()
            {
                continue;
            }
            let name = if agent.name.trim().is_empty() {
                agent.id.clone()
            } else {
                agent.name.clone()
            };
            let detail = agent
                .description
                .clone()
                .unwrap_or_else(|| "ACP agent".into());
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        agent.id.clone(),
                        name,
                        agent.version.clone(),
                        agent.icon.clone(),
                        detail,
                        ToolStatus::Available,
                        Vec::new(),
                    ),
                    false,
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(Finished { error: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::acp::registry::{Distribution, PackageDist};
    use vmux_tool::io::{DirectoryEntries, DirectoryEntry, FileBytes};
    use vmux_tool::observation::{Detail, Icon, Id, Item, Removable, Status, Version};
    use vmux_tool::state::ToolProvider;

    #[derive(Resource)]
    struct FixtureInputs {
        directory: tempfile::TempDir,
    }

    struct FixtureIoPlugin;

    impl Plugin for FixtureIoPlugin {
        fn build(&self, app: &mut App) {
            app.add_observer(fake_directories).add_observer(fake_files);
        }
    }

    fn fake_directories(
        trigger: On<Add, JobRequest<ReadDirectory>>,
        requests: Query<Entity, With<JobRequest<ReadDirectory>>>,
        inputs: Res<FixtureInputs>,
        mut commands: Commands,
    ) {
        let entity = trigger.event_target();
        if !requests.contains(entity) {
            return;
        }
        let entry = DirectoryEntry {
            name: "gemini".into(),
            path: inputs.directory.path().join("gemini"),
            file_type: inputs.directory.path().metadata().unwrap().file_type(),
        };
        commands
            .entity(entity)
            .remove::<JobRequest<ReadDirectory>>()
            .insert(JobResult::<ReadDirectory>(Ok(Ok(DirectoryEntries {
                exists: true,
                is_directory: true,
                entries: vec![entry],
            }))));
    }

    fn fake_files(
        trigger: On<Add, JobRequest<ReadFile>>,
        requests: Query<&JobRequest<ReadFile>>,
        mut commands: Commands,
    ) {
        let entity = trigger.event_target();
        let Ok(request) = requests.get(entity) else {
            return;
        };
        let bytes = request
            .0
            .path
            .file_name()
            .is_some_and(|name| name == "vmux-receipt.json")
            .then(|| {
                br#"{"name":"gemini","version":"0.63.0","source_id":"acp:gemini","bin":{}}"#
                    .to_vec()
            });
        commands
            .entity(entity)
            .remove::<JobRequest<ReadFile>>()
            .insert(JobResult::<ReadFile>(Ok(Ok(FileBytes(bytes)))));
    }

    #[test]
    fn observation_publishes_installed_receipts_and_available_registry_agents() {
        let mut app = App::new();
        app.insert_resource(FixtureInputs {
            directory: tempfile::tempdir().unwrap(),
        })
        .add_plugins((AcpInventoryPlugin, FixtureIoPlugin))
        .add_message::<RefreshRequest>();

        let mut catalog = Catalog::new();
        catalog.ready = true;
        catalog.revision = 1;
        let catalog_entity = app.world_mut().spawn(catalog).id();
        app.world_mut().spawn((
            RegistryAgent {
                id: "gemini".into(),
                name: "Gemini CLI".into(),
                version: Some("0.71.1".into()),
                description: Some("Google's official CLI for Gemini".into()),
                icon: Some("https://cdn.example/gemini.svg".into()),
                distribution: Distribution {
                    npx: Some(PackageDist {
                        package: "@google/gemini-cli@0.71.1".into(),
                        args: vec!["--acp".into()],
                        env: Default::default(),
                    }),
                    ..Default::default()
                },
            },
            ChildOf(catalog_entity),
        ));
        app.world_mut().spawn((
            RegistryAgent {
                id: "goose".into(),
                name: "Goose".into(),
                version: Some("1.2.0".into()),
                description: Some("Open-source coding agent".into()),
                icon: Some("https://cdn.example/goose.svg".into()),
                distribution: Distribution {
                    npx: Some(PackageDist {
                        package: "goose-acp@1.2.0".into(),
                        args: Vec::new(),
                        env: Default::default(),
                    }),
                    ..Default::default()
                },
            },
            ChildOf(catalog_entity),
        ));
        app.world_mut().spawn((
            RegistryAgent {
                id: "unsupported".into(),
                name: "Unsupported".into(),
                version: Some("0.1.0".into()),
                description: Some("No host distribution".into()),
                icon: None,
                distribution: Distribution::default(),
            },
            ChildOf(catalog_entity),
        ));

        let provider = app
            .world_mut()
            .spawn(ToolProviderId(ToolProvider::new("acp")))
            .id();
        let observation = app
            .world_mut()
            .spawn((
                Observation,
                Operation::bundle("tool.provider.observe")
                    .unwrap()
                    .running(),
            ))
            .id();
        app.world_mut().trigger(ObserveProviderRequest {
            entity: provider,
            observation,
            refresh_catalogs: false,
        });

        for _ in 0..8 {
            if app.world().get::<Finished>(observation).is_some() {
                break;
            }
            app.update();
        }

        assert_eq!(
            app.world().get::<Finished>(observation).unwrap().error,
            None
        );
        let mut observed = app
            .world_mut()
            .query_filtered::<(
                &ChildOf,
                &Id,
                &Name,
                &Version,
                &Icon,
                &Detail,
                &Status,
                &Removable,
            ), With<Item>>()
            .iter(app.world())
            .filter(|(parent, ..)| parent.parent() == observation)
            .map(|(_, id, name, version, icon, detail, status, removable)| {
                (
                    id.0.clone(),
                    name.as_str().to_string(),
                    version.0.clone(),
                    icon.0.clone(),
                    detail.0.clone(),
                    status.0,
                    removable.0,
                )
            })
            .collect::<Vec<_>>();
        observed.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            observed,
            vec![
                (
                    "gemini".to_string(),
                    "Gemini CLI".to_string(),
                    Some("0.63.0".to_string()),
                    Some("https://cdn.example/gemini.svg".to_string()),
                    "Google's official CLI for Gemini".to_string(),
                    ToolStatus::Outdated,
                    true,
                ),
                (
                    "goose".to_string(),
                    "Goose".to_string(),
                    Some("1.2.0".to_string()),
                    Some("https://cdn.example/goose.svg".to_string()),
                    "Open-source coding agent".to_string(),
                    ToolStatus::Available,
                    false,
                ),
            ]
        );
    }
}
