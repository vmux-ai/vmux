use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_tool::{AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet};

mod bridge;
mod bridge_page;
mod broker;
mod capability;
mod load;
mod manager_page;
mod model;
mod project;
mod runtime;
mod service_worker_cache;
mod shim;
mod tabs;
mod template;
mod windows;

#[vmux_native::page]
pub struct ExtensionPlugin;

impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::ExtensionPage::plugin());

        let prepared = load::apply_env().unwrap_or_else(|error| {
            bevy::log::error!(%error, "failed to prepare extensions; starting without them");
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
            Vec::new()
        });
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let conformance_extension = std::env::var("VMUX_EXTENSION_CONFORMANCE_ID").ok();
        let registrations = prepared
            .iter()
            .map(|runtime| bridge::BridgeRegistration {
                extension_id: runtime.extension_id.clone(),
                authorization: bridge::BridgeAuthorization {
                    permissions: runtime.granted_permissions.iter().cloned().collect(),
                    host_permissions: runtime
                        .granted_host_permissions
                        .iter()
                        .map(|pattern| {
                            crate::match_pattern::ExtensionMatchPattern::parse(pattern)
                                .unwrap_or_else(|error| {
                                    panic!("invalid stored host permission: {error}")
                                })
                        })
                        .collect(),
                    conformance: conformance_extension.as_deref()
                        == Some(runtime.extension_id.as_str()),
                },
            })
            .collect();
        app.world_mut()
            .spawn((Name::new("Extensions"), load::PreparedExtensions(prepared)));
        app.add_agent_request::<AgentBrowserInstallExtension>()
            .bind_tool::<BrowserInstallExtensionArgs>()
            .add_plugins((
                crate::catalog::ExtensionCatalogPlugin,
                vmux_ecs::host::UiStatePlugin::<vmux_api::extension::ExtensionsEvent>::default(),
                bridge::ExtensionBridgePlugin::new(profile, registrations),
                bridge_page::ExtensionBridgePagePlugin,
                broker::ExtensionBrokerPlugin,
                project::ExtensionProjectPlugin,
                windows::ExtensionWindowsPlugin,
                manager_page::ManagerPagePlugin,
            ))
            .add_systems(Update, request_install.after(AgentRequestRouteSet))
            .add_systems(Update, encode_install.in_set(ToolDispatchSet));
    }
}

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
enum ExtensionSystemSet {
    DrainBridge,
    SyncWindows,
}

#[vmux_api::agent]
struct AgentBrowserInstallExtension {
    source: String,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserInstallExtensionArgs {
    source: String,
}

fn request_install(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserInstallExtension>>,
    mut install: MessageWriter<crate::catalog::ExtensionInstallRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        install.write(crate::catalog::ExtensionInstallRequest {
            source: request.payload.source.clone(),
            requester: None,
        });
        responses.write(request.reply.ok());
    }
}

fn encode_install(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserInstallExtensionArgs), AddedTool<BrowserInstallExtensionArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.source.trim().is_empty() {
            Err("browser_install_extension.source is empty".to_string())
        } else {
            AgentRequest::encode(&AgentBrowserInstallExtension {
                source: args.source.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
