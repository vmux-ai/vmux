use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentOpenBeside, AgentRequest, AgentRequestId, AgentVaultStatus, ClientMessage,
};
use vmux_core::ProcessAnchor;
use vmux_core::service::ServiceRequest;
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet,
};

pub struct VaultToolPlugin;

impl Plugin for VaultToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("feature.ron"),
            "default",
        ))
        .add_message::<ToolQueryRequest>()
        .add_message::<ToolQueryHandled>()
        .add_message::<ServiceRequest>()
        .register_tool::<VaultStatusArgs>("vault_status")
        .register_tool::<OpenVaultArgs>("open_vault")
        .add_systems(Update, (vault_status, open_vault).in_set(ToolDispatchSet));
    }
}

pub(super) struct VaultAgentPlugin;

impl Plugin for VaultAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VaultToolPlugin)
            .add_message::<VaultStatusRequest>()
            .add_systems(Update, route_vault_queries.in_set(ToolQueryRouteSet))
            .add_systems(Update, answer_vault_queries.after(ToolQueryRouteSet));
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultStatusArgs {}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultProvider {
    Overview,
    Github,
    CloudFolder,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenVaultArgs {
    provider: Option<VaultProvider>,
}

#[derive(Message)]
struct VaultStatusRequest {
    request_id: AgentRequestId,
}

fn vault_status(mut commands: Commands, calls: Query<Entity, AddedTool<VaultStatusArgs>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(AgentRequest::encode(&AgentVaultStatus)));
    }
}

fn open_vault(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &OpenVaultArgs), Added<OpenVaultArgs>>,
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
                    VaultProvider::Overview => "vmux://vault/",
                    VaultProvider::Github => "vmux://vault/?provider=github",
                    VaultProvider::CloudFolder => "vmux://vault/?provider=cloud_folder",
                };
                AgentRequest::encode(&AgentOpenBeside {
                    anchor,
                    direction: None,
                    url: url.to_string(),
                    focus: true,
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn route_vault_queries(
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
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message,
                }));
            }
            Ok(None) => {}
        }
    }
}

fn answer_vault_queries(
    mut requests: MessageReader<VaultStatusRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentVaultStatusResult {
            request_id: request.request_id,
            result: Ok(vmux_core::profile::vault::VaultStatus::current().snapshot()),
        }));
    }
}
