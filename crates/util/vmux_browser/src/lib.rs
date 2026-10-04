#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::CefEmbeddedHosts;
pub use command::{NavigationRequest, OpenRequest, ShowDevToolsRequest, ZoomRequest};
pub use host::AgentBrowserResolve;
pub use host::WebviewLoadCompleted;
pub use host_focus::HostFocusIntent;
pub use infrastructure::{InfrastructureWebview, PopupWebview, RetiredInfrastructureWebview};
pub use native_bridge::NativeBridge;
#[cfg(target_os = "macos")]
pub use native_bridge::{queue_command_bar_pointer_button, queue_command_bar_pointer_move};
pub use native_layout::NativeLayout;
pub use navigation::OpenHistoryRequest;
pub use tool::BrowserToolPlugin;
use vmux_command::{PendingCommandBarReveal, ReadCommandRequests};
use vmux_ecs::{
    PageOpenSet,
    page::{PageManifest, PageReady},
};
use vmux_layout::PendingWebviewReveal;
use vmux_layout::event::{
    HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
    RemoteCopyEvent, RemotePairingDismissRequest, RemotePairingShowRequest, RemoteRequest,
    RemoteRevokeRequest, SideSheetProjectOpenRequest, SideSheetResizeEvent,
    SideSheetSectionRequest, SideSheetStackActivateRequest, SideSheetStackCloseRequest,
    SideSheetStackCreateRequest, WindowDragRegionEvent,
};
pub use vmux_layout::{Browser, Loading};
pub use window_drag::WindowDragRegion;

pub(crate) struct Feature;

impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod appearance;
mod command;
mod dom_snapshot;
mod frame_rate;
mod host;
mod host_focus;
mod infrastructure;
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
    SyncWindowedFrames,
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BrowserLoadSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BrowserOverlaySet;

pub struct BrowserPlugin;

impl BrowserPlugin {
    fn configure_backend(app: &mut App) -> &mut App {
        app.configure_sets(
            Update,
            BrowserSystemSet::SyncCefBackend.before(CefSystems::CreateAndResize),
        )
        .add_systems(
            Update,
            sync_cef_backend
                .in_set(BrowserSystemSet::SyncCefBackend)
                .after(PageOpenSet::Fallback)
                .after(BrowserSystemSet::SpawnPopupStacks),
        )
    }
}

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        let startup_appearance = vmux_setting::AppearanceSettings::from_disk();
        let startup_locale = appearance::BrowserLocale::requested(&startup_appearance.locale);
        let startup_accept_language_list = startup_locale.accept_language_list();
        app.add_plugins((
            host::AgentBrowserPlugin,
            vmux_command::CommandBarPlugin,
            BrowserToolPlugin,
            platform::BrowserPlatformPlugin,
        ));
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let embedded_hosts = CefEmbeddedHosts(
            manifests
                .iter(app.world())
                .map(PageManifest::embedded_host)
                .collect(),
        );
        Self::configure_backend(app)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
            .configure_sets(
                Update,
                CefSystems::CreateAndResize.after(ReadCommandRequests),
            )
            .add_plugins((
                CefPlugin {
                    command_line_config: host::CefStartup::command_line(),
                    root_cache_path: host::CefStartup::root_cache_path(),
                    locale: startup_locale.value().to_string(),
                    accept_language_list: startup_accept_language_list,
                    embedded_hosts,
                    #[cfg(target_os = "macos")]
                    os_crypt_key_provider: host::CefStartup::os_crypt_key_provider(),
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

fn sync_cef_backend(
    browser_entities: Query<Entity, With<Browser>>,
    webviews: Query<
        (Entity, Has<WebviewNativeOverlay>, Has<WebviewWindowed>),
        (With<Browser>, With<WebviewSource>),
    >,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    mut browsers: NonSendMut<Browsers>,
    mut commands: Commands,
) {
    let mut moved = Vec::new();
    for entity in &browser_entities {
        let mut current = entity;
        let mut inherited = None;
        while let Ok(parent) = child_of.get(current).map(Relationship::get) {
            if let Ok(host) = host_windows.get(parent) {
                inherited = Some(*host);
                break;
            }
            current = parent;
        }
        let Some(inherited) = inherited else {
            continue;
        };
        if host_windows.get(entity).ok() != Some(&inherited) {
            commands.entity(entity).insert(inherited);
            moved.push(entity);
        }
    }

    let mut recreate = Vec::new();
    for (entity, native_overlay, _) in &webviews {
        let stale_backend = browsers
            .is_windowed(&entity)
            .is_some_and(|windowed| !windowed);
        let stale_overlay = browsers.has_browser(entity) && native_overlay;
        if stale_backend || stale_overlay || moved.contains(&entity) {
            recreate.push(entity);
        }
    }
    for entity in &recreate {
        browsers.close(entity);
    }
    for (entity, native_overlay, windowed) in &webviews {
        let needs_recreate = recreate.contains(&entity);
        let settled = windowed && !native_overlay && !needs_recreate;
        if settled {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity
            .insert(WebviewWindowed)
            .remove::<WebviewNativeOverlay>();
        if needs_recreate {
            entity
                .remove::<PageReady>()
                .remove::<PendingWebviewReveal>()
                .remove::<PendingCommandBarReveal>();
        }
    }
}

#[cfg(test)]
mod tests;
