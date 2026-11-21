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
            (directories, receipts, publish)
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
    observation.insert((Accepted, Listing, JobRequest(ReadDirectory { path })));
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

fn publish(
    observations: Query<
        (Entity, &OperationState, Option<&WaitingCatalog>),
        (With<Reading>, Without<Finished>),
    >,
    pending: Query<(&ChildOf, Has<ReadDone>), With<Expected>>,
    receipts: Query<(&Receipt, &ChildOf)>,
    catalogs: Query<&Catalog>,
    agents: Query<&RegistryAgent>,
    mut commands: Commands,
) {
    let Ok(catalog) = catalogs.single() else {
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
            let agent = agents
                .iter()
                .find(|agent| agent.id == receipt.name.as_str());
            let latest = agent.and_then(|agent| agent.version.as_ref());
            let status = if receipt.version.is_some()
                && latest.is_some()
                && receipt.version.as_ref() != latest
            {
                ToolStatus::Outdated
            } else {
                ToolStatus::Installed
            };
            commands.spawn((
                ObservedPackage::new(
                    ObservedTool::new(
                        receipt.name.as_str().to_string(),
                        agent
                            .map(|agent| agent.name.clone())
                            .unwrap_or_else(|| receipt.name.as_str().to_string()),
                        receipt.version.clone(),
                        agent.and_then(|agent| agent.icon.clone()),
                        agent
                            .and_then(|agent| agent.description.clone())
                            .unwrap_or_else(|| "ACP agent".into()),
                        status,
                        Vec::new(),
                    ),
                    true,
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(Finished { error: None });
    }
}
