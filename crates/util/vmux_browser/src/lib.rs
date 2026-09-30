#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub(crate) struct Feature;

impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

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
pub use host::AgentBrowserResolve;
pub use host_focus::HostFocusIntent;
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

use vmux_ui::i18n::Locale;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub(crate) enum BrowserSystemSet {
    ApplyPendingNavigation,
    DrainLoadingState,
    DrivePendingNavigationSnapshots,
    HostFocusApplied,
    Navigate,
    Scroll,
    SpawnPopupStacks,
    SyncCefBackend,
    SyncWindowedCommandBar,
    SyncWindowedExtensionPopups,
    SyncWindowedFrames,
}

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let startup_settings = vmux_setting::AppSettings::from_disk();
        let startup_locale =
            Locale::requested(Some(&startup_settings.appearance.locale)).into_string();
        let startup_accept_language_list = host::browser_accept_language_list(&startup_locale);
        app.add_plugins((
            host::AgentBrowserPlugin,
            vmux_command::command_bar::CommandBarPlugin,
            BrowserToolPlugin,
            platform::BrowserPlatformPlugin,
            extension::ExtensionPlugin,
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
                vmux_layout::LayoutCefPlugin,
            ))
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
