#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod appearance;
mod command;
mod extension;
mod frame_rate;
mod host;
mod host_focus;
mod input;
mod native_bridge;
mod native_layout;
mod navigation;
mod page;
mod page_life;
mod platform;
mod present;
mod scroll;
mod snapshot;
mod state;
mod tool;
mod window_drag;
pub use command::{NavigationRequest, OpenRequest, ShowDevToolsRequest, ZoomRequest};
pub use host_focus::{HostFocusIntent, KeyboardContext, KeyboardContextSet};
pub use navigation::OpenHistoryRequest;
pub use tool::BrowserToolPlugin;
pub use window_drag::WindowDragRegion;

pub use native_bridge::NativeBridge;
#[cfg(target_os = "macos")]
pub use native_bridge::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
pub use native_layout::NativeLayout;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::CefEmbeddedHosts;
use vmux_command::ReadCommandRequests;
use vmux_core::page::PageManifest;
use vmux_layout::event::{
    HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
    RemoteCopyEvent, RemotePairingDismissRequest, RemotePairingShowRequest, RemoteRequest,
    RemoteRevokeRequest, SideSheetProjectOpenRequest, SideSheetResizeEvent,
    SideSheetSectionRequest, SideSheetStackActivateRequest, SideSheetStackCloseRequest,
    SideSheetStackCreateRequest, WindowDragRegionEvent,
};
pub use vmux_layout::{Browser, Loading};

use vmux_flex::prelude::*;
use vmux_ui::i18n::Locale;

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let profile = vmux_core::profile::active_profile_name();
        let startup_settings = vmux_setting::AppSettings::from_disk();
        let startup_locale =
            Locale::requested(Some(&startup_settings.appearance.locale)).into_string();
        let startup_accept_language_list = host::browser_accept_language_list(&startup_locale);
        let prepared_extensions = crate::extension::load::apply_env().unwrap_or_else(|error| {
            bevy::log::error!(%error, "failed to prepare extensions; starting without them");
            unsafe { std::env::remove_var("VMUX_LOAD_EXTENSIONS") };
            Vec::new()
        });
        let conformance_extension = std::env::var("VMUX_EXTENSION_CONFORMANCE_ID").ok();
        let extension_registrations = prepared_extensions
            .iter()
            .map(|runtime| crate::extension::bridge::BridgeRegistration {
                extension_id: runtime.extension_id.clone(),
                authorization: crate::extension::bridge::BridgeAuthorization {
                    permissions: runtime.granted_permissions.iter().cloned().collect(),
                    host_permissions: runtime
                        .granted_host_permissions
                        .iter()
                        .map(|pattern| {
                            vmux_extension::match_pattern::ChromeMatchPattern::parse(pattern)
                                .unwrap_or_else(|error| {
                                    panic!("invalid stored host permission: {error}")
                                })
                        })
                        .collect(),
                    conformance: conformance_extension.as_deref()
                        == Some(runtime.extension_id.as_str()),
                },
            })
            .collect::<Vec<_>>();
        let extension_bridge = crate::extension::bridge::ExtensionBridgeServer::start_registered(
            &profile,
            extension_registrations,
        )
        .unwrap_or_else(|error| panic!("failed to start extension bridge: {error}"));
        app.add_plugins((
            vmux_command::command_bar::CommandBarPlugin,
            BrowserToolPlugin,
            platform::BrowserPlatformPlugin,
            extension::ExtensionBrowserPlugin,
            extension::bridge_page::ExtensionBridgePagePlugin,
            extension::broker::ExtensionBrokerPlugin,
            extension::project::ExtensionProjectPlugin,
            extension::windows::ExtensionWindowsPlugin,
        ));
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        let cef_command_line = host::cef_command_line_config();
        host::configure_cef_backend_sync(app)
            .insert_resource(crate::extension::load::PreparedExtensions(
                prepared_extensions,
            ))
            .insert_resource(extension_bridge)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<host::WebviewLoadCompleted>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
            .configure_sets(
                Update,
                CefSystems::CreateAndResize.after(ReadCommandRequests),
            )
            .add_plugins((
                CefPlugin {
                    command_line_config: cef_command_line,
                    root_cache_path: host::cef_root_cache_path(),
                    locale: startup_locale,
                    accept_language_list: startup_accept_language_list,
                    embedded_hosts,
                    #[cfg(target_os = "macos")]
                    os_crypt_key_provider: host::cef_os_crypt_key_provider(),
                    ..default()
                },
                UiEventPlugin::<(
                    HeaderBackRequest,
                    HeaderForwardRequest,
                    HeaderReloadRequest,
                    HeaderAddressFocusRequest,
                    SideSheetStackActivateRequest,
                    SideSheetStackCloseRequest,
                    SideSheetStackCreateRequest,
                    SideSheetProjectOpenRequest,
                )>::default(),
                UiEventPlugin::<(
                    SideSheetSectionRequest,
                    SideSheetResizeEvent,
                    WindowDragRegionEvent,
                    RemoteRequest,
                    RemotePairingShowRequest,
                    RemotePairingDismissRequest,
                    RemoteCopyEvent,
                    RemoteRevokeRequest,
                )>::default(),
            ))
            .add_systems(Update, (vmux_layout::apply_cef_state_from_webview,))
            .add_systems(
                Update,
                vmux_layout::mirror_metadata_to_url
                    .after(vmux_layout::apply_cef_state_from_webview),
            )
            .add_plugins((
                host_focus::HostFocusPlugin,
                appearance::AppearancePlugin,
                page_life::PageLifePlugin,
                command::CommandPlugin,
                frame_rate::FrameRatePlugin,
                input::InputPlugin,
                navigation::NavigationPlugin,
                present::PresentPlugin,
                page::PagePlugin,
                state::StatePlugin,
                snapshot::SnapshotPlugin,
                scroll::ScrollPlugin,
                window_drag::WindowDragPlugin,
            ));
    }
}

pub use host::{native_left_mouse_down, set_native_left_mouse_down};

#[cfg(test)]
mod tests;
