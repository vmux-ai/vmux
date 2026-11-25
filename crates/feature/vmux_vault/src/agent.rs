use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{AgentQueryResult, AgentRequest, AgentRequestId, ClientMessage};
use vmux_ecs::service::ServiceRequest;
use vmux_ecs::{
    AtGeneration, Generation, GenerationPlugin, GenerationSet, OperationTarget, ProcessAnchor,
    Stale,
};
use vmux_feature::manifest::FeaturePlugin;
use vmux_layout::AgentOpenBeside;
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchSet, ToolQuery, ToolQueryHandled, ToolQueryRequest,
    ToolQueryRouteSet,
};

use crate::host::key::VaultNamespace;

#[vmux_api::agent(Copy, Eq)]
struct AgentVaultStatus;

pub struct VaultToolPlugin;

impl Plugin for VaultToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_systems(Update, (status, open).in_set(ToolDispatchSet));
    }
}

pub(super) struct VaultAgentPlugin;

impl Plugin for VaultAgentPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<GenerationPlugin<VaultNamespace>>() {
            app.add_plugins(GenerationPlugin::<VaultNamespace>::default());
        }
        app.add_plugins(VaultToolPlugin)
            .add_message::<VaultStatusRequest>()
            .add_systems(Update, route_queries.in_set(ToolQueryRouteSet))
            .add_systems(
                Update,
                (collect_queries, answer_queries)
                    .chain()
                    .after(ToolQueryRouteSet)
                    .after(crate::host::RuntimeSet)
                    .after(GenerationSet::of::<VaultNamespace>()),
            );
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultStatusTool {}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultProvider {
    Overview,
    Github,
    CloudFolder,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenVaultTool {
    provider: Option<VaultProvider>,
}

#[derive(Message)]
struct VaultStatusRequest {
    request_id: AgentRequestId,
}

fn status(mut commands: Commands, calls: Query<Entity, AddedTool<VaultStatusTool>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(AgentRequest::encode(&AgentVaultStatus)));
    }
}

fn open(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &OpenVaultTool), Added<OpenVaultTool>>,
) {
    for (entity, name, anchor, args) in &requests {
        let command = anchor
            .map(|anchor| anchor.0)
            .ok_or_else(|| {
                format!(
                    "{} requires an agent anchor (not available to this client)",
                    name.as_str()
                )
            })
            .and_then(|anchor| {
                let url = match args.provider.as_ref().unwrap_or(&VaultProvider::Overview) {
                    VaultProvider::Overview => crate::VaultPlugin::url().to_string(),
                    VaultProvider::Github => {
                        format!("{}?provider=github", crate::VaultPlugin::url())
                    }
                    VaultProvider::CloudFolder => {
                        format!("{}?provider=cloud_folder", crate::VaultPlugin::url())
                    }
                };
                AgentRequest::encode(&AgentOpenBeside {
                    anchor,
                    direction: None,
                    url,
                    focus: true,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn route_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut vault: MessageWriter<VaultStatusRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentVaultStatus::id() {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        match request.query.decode::<AgentVaultStatus>() {
            Ok(Some(_)) => {
                vault.write(VaultStatusRequest {
                    request_id: request.request_id,
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                    AgentQueryResult::text(request.request_id, Err(message)),
                )));
            }
            Ok(None) => {}
        }
    }
}

#[derive(Component)]
struct PendingVaultStatus(AgentRequestId);

fn collect_queries(
    mut requests: MessageReader<VaultStatusRequest>,
    registry: Query<(Entity, &Generation<VaultNamespace>), With<crate::host::VaultRegistry>>,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((namespace, generation)) = registry.single() else {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(
                    request.request_id,
                    Err("Vault status is unavailable".to_string()),
                ),
            )));
            continue;
        };
        commands.spawn((
            PendingVaultStatus(request.request_id),
            OperationTarget::new(namespace),
            generation.snapshot(),
        ));
    }
}

fn answer_queries(
    requests: Query<(
        Entity,
        &PendingVaultStatus,
        &OperationTarget,
        &AtGeneration<VaultNamespace>,
        Has<Stale<VaultNamespace>>,
    )>,
    registry: Query<(
        &crate::host::VaultRegistry,
        Option<&crate::host::VaultStatusProjection>,
        &Generation<VaultNamespace>,
    )>,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for (entity, request, target, accepted, stale) in &requests {
        let result = if stale {
            Err("Vault status owner replaced".to_string())
        } else {
            match registry.get(target.entity()) {
                Ok((_, _, generation)) if accepted.is_stale(*generation) => {
                    Err("Vault status owner replaced".to_string())
                }
                Ok((state, Some(status), _)) if state.current() => {
                    serde_json::to_string_pretty(&status.0).map_err(|error| error.to_string())
                }
                Ok(_) => continue,
                Err(_) => Err("Vault status owner retired".to_string()),
            }
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.0, result),
        )));
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{VaultRegistry, VaultStatusProjection};
    use bevy::ecs::message::Messages;

    struct StatusScenario {
        app: App,
        namespace: Entity,
    }

    impl StatusScenario {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(VaultAgentPlugin);
            let namespace = app
                .world_mut()
                .spawn((
                    VaultRegistry::default(),
                    Generation::<VaultNamespace>::default(),
                ))
                .id();
            Self { app, namespace }
        }

        fn query(&mut self, id: u8) {
            self.app.world_mut().write_message(VaultStatusRequest {
                request_id: AgentRequestId([id; 16]),
            });
            self.app.update();
        }

        fn publish(&mut self) {
            let observed = crate::storage::VaultStatus {
                root: std::path::PathBuf::from("/isolated/vault"),
                initialized: true,
                encrypted: true,
                unlocked: true,
                remote: "https://example.invalid/vault.git".to_string(),
                dirty: 2,
                ..Default::default()
            };
            let snapshot = crate::state::VaultSnapshot {
                initialized: true,
                encrypted: true,
                unlocked: true,
                dirty: 2,
                ..Default::default()
            };
            self.app
                .world_mut()
                .get_mut::<VaultRegistry>(self.namespace)
                .unwrap()
                .publish(snapshot);
            self.app
                .world_mut()
                .entity_mut(self.namespace)
                .insert(VaultStatusProjection(observed.snapshot()));
        }

        fn replies(&mut self) -> Vec<AgentQueryResult> {
            self.app
                .world_mut()
                .resource_mut::<Messages<ServiceRequest>>()
                .drain()
                .filter_map(|request| match request.0 {
                    ClientMessage::AgentQueryResult(result) => Some(result),
                    _ => None,
                })
                .collect()
        }
    }

    #[test]
    fn status_queries_wait_for_a_current_canonical_observation() {
        let mut scenario = StatusScenario::new();
        scenario.query(1);
        assert!(scenario.replies().is_empty());
        scenario.publish();
        scenario.app.update();
        let replies = scenario.replies();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].request_id, AgentRequestId([1; 16]));
        assert!(!replies[0].is_error);
        let data: serde_json::Value = serde_json::from_str(&replies[0].content).unwrap();
        assert_eq!(data["connected"], true);
        assert_eq!(data["localChanges"], 2);
        assert_eq!(data["syncNeeded"], true);
    }

    #[test]
    fn a_replaced_namespace_cannot_answer_an_old_query() {
        let mut scenario = StatusScenario::new();
        scenario.query(2);
        assert!(scenario.replies().is_empty());
        scenario
            .app
            .world_mut()
            .get_mut::<Generation<VaultNamespace>>(scenario.namespace)
            .unwrap()
            .advance();
        scenario.publish();
        scenario.app.update();
        let replies = scenario.replies();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].request_id, AgentRequestId([2; 16]));
        assert!(replies[0].is_error);
    }
}
