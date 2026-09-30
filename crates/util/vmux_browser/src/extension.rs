pub(crate) mod bridge;
pub(crate) mod bridge_page;
pub(crate) mod broker;
mod capability;
pub mod load;
mod manager_page;
pub(crate) mod model;
pub(crate) mod project;
mod runtime;
mod service_worker_cache;
mod shim;
mod tabs;
mod template;
pub(crate) mod windows;

pub(crate) use manager_page::{ExtensionPopup, ExtensionPopupBounds, ExtensionPopupPresented};

pub(crate) struct ExtensionPlugin;

impl bevy::prelude::Plugin for ExtensionPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        let prepared = load::apply_env().unwrap_or_else(|error| {
            bevy::log::error!(%error, "failed to prepare extensions; starting without them");
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
            Vec::new()
        });
        let profile = vmux_core::profile::Profile::current().into_id();
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
                            vmux_extension::match_pattern::ExtensionMatchPattern::parse(pattern)
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
        app.world_mut().spawn((
            bevy::prelude::Name::new("Extensions"),
            load::PreparedExtensions(prepared),
        ));
        app.add_plugins((
            bridge::ExtensionBridgePlugin::new(profile, registrations),
            bridge_page::ExtensionBridgePagePlugin,
            broker::ExtensionBrokerPlugin,
            project::ExtensionProjectPlugin,
            windows::ExtensionWindowsPlugin,
            manager_page::ManagerPagePlugin,
        ));
    }
}

#[derive(bevy::prelude::SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum ExtensionSystemSet {
    DrainBridge,
    SyncWindows,
}
