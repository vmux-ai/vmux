use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{AgentQueryResult, AgentRequest, AgentRequestId, ClientMessage};
use vmux_ecs::ProcessAnchor;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::service::ServiceRequest;
use vmux_layout::AgentOpenBeside;
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchSet, ToolQuery, ToolQueryHandled, ToolQueryRequest,
    ToolQueryRouteSet,
};

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
        app.add_plugins(VaultToolPlugin)
            .add_message::<VaultStatusRequest>()
            .add_systems(Update, route_queries.in_set(ToolQueryRouteSet))
            .add_systems(Update, answer_queries.after(ToolQueryRouteSet));
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
                    VaultProvider::Overview => crate::VaultPlugin::URL.to_string(),
                    VaultProvider::Github => {
                        format!("{}?provider=github", crate::VaultPlugin::URL)
                    }
                    VaultProvider::CloudFolder => {
                        format!("{}?provider=cloud_folder", crate::VaultPlugin::URL)
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

fn answer_queries(
    mut requests: MessageReader<VaultStatusRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let snapshot = crate::storage::VaultStatus::current().snapshot();
        let result = serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string());
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
    }
}
