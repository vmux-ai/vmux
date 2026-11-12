use crate::filesystem::FilesystemPlugin;
use crate::{AutoSave, Catalog, CorePlugin, ExtensionSchedule, Root, Seed};
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};
use vmux_ecs::manifest::FeaturePlugin;
use vmux_tool::{AddedTool, ToolCommand, ToolDispatchSet};

mod access;
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

pub struct ExtensionPlugin;

impl Plugin for ExtensionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());

        assert!(
            app.is_plugin_added::<CorePlugin>(),
            "Native Extension requires CorePlugin in application composition"
        );
        let store = crate::store::ExtensionStore::current();
        let mut seed = store.load_index();
        if let Ok(index) = &mut seed
            && index.requires_save()
            && let Err(error) = load::PreparedExtensions::migrate_index_permissions(&store, index)
        {
            seed = Err(error);
        }
        let root = app
            .world_mut()
            .spawn((Binding, Root(store.path().to_path_buf()), Seed(seed)))
            .id();
        app.world_mut().run_schedule(ExtensionSchedule);
        let mut catalog = SystemState::<Catalog>::new(app.world_mut());
        let generation = catalog
            .get(app.world())
            .unwrap()
            .ticket(root)
            .unwrap()
            .generation;
        let initial = catalog.get(app.world()).unwrap().snapshot(root);
        let preparation = initial.and_then(|index| load::PreparedExtensions::prepare(&index));
        let prepared = match preparation {
            Ok((prepared, evidence)) => {
                app.world_mut().entity_mut(root).insert(evidence);
                app.world_mut().run_schedule(ExtensionSchedule);
                prepared
            }
            Err(error) => {
                bevy::log::error!(%error, "failed to prepare extensions; starting without them");
                unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
                load::PreparedExtensions::default()
            }
        };
        let mut ids = Vec::new();
        for runtime in &prepared.0 {
            ids.push(runtime.extension_id.clone());
            let profile = vmux_ecs::profile::Profile::current().into_id();
            let association = catalog.get(app.world()).unwrap().association(
                root,
                &runtime.extension_id,
                &profile,
            );
            if let Some(association) = association {
                app.world_mut().spawn((
                    RuntimeBinding,
                    crate::Id(runtime.extension_id.clone()),
                    ChildOf(association),
                ));
            }
        }
        app.world_mut().entity_mut(root).insert((
            AutoSave,
            RuntimeIds(ids),
            RuntimeGeneration(generation),
            crate::ProfileId(vmux_ecs::profile::Profile::current().into_id()),
        ));
        app.add_plugins(FilesystemPlugin);
        let profile = vmux_ecs::profile::Profile::current().into_id();
        let conformance_extension = std::env::var("VMUX_EXTENSION_CONFORMANCE_ID").ok();
        let registrations = prepared
            .0
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
        app.world_mut().spawn((Name::new("Extensions"), prepared));
        app.add_agent_request::<AgentBrowserInstallExtension>()
            .add_plugins((
                crate::catalog::ExtensionCatalogPlugin,
                vmux_ecs::UiStatePlugin::<vmux_api::extension::ExtensionsUiState>::default(),
                bridge::ExtensionBridgePlugin::new(profile, registrations),
                access::Plugin,
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

#[derive(Component)]
pub(crate) struct Binding;

#[derive(Component)]
pub(crate) struct RuntimeIds(pub Vec<String>);

#[derive(Component)]
pub(crate) struct RuntimeGeneration(pub u64);

#[derive(Component)]
pub(crate) struct RuntimeBinding;

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
struct BrowserInstallExtension {
    source: String,
}

fn request_install(
    mut requests: MessageReader<AgentRequestMessage<AgentBrowserInstallExtension>>,
    mut install: MessageWriter<crate::ExtensionInstallRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        install.write(crate::ExtensionInstallRequest {
            source: request.payload.source.clone(),
            requester: None,
        });
        responses.write(request.reply.ok());
    }
}

fn encode_install(
    mut commands: Commands,
    requests: Query<(Entity, &BrowserInstallExtension), AddedTool<BrowserInstallExtension>>,
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
