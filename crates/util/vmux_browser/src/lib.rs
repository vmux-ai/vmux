#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::{ecs::relationship::Relationship, ecs::system::Commands, prelude::*};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{CefEmbeddedHosts, CefSchemeRoute};
pub use command::{
    NavigationOperation, NavigationRequest, OpenRequest, ShowDevToolsRequest, ZoomChange,
    ZoomRequest,
};
pub use host::AgentBrowserResolve;
pub use host::WebviewLoadCompleted;
pub use host_focus::{HostFocusIntent, HostFocusTarget};
pub use infrastructure::{InfrastructureWebview, PopupWebview, RetiredInfrastructureWebview};
pub use input::WindowPointerRegions;
pub use native_bridge::NativeBridge;
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
    HeaderStopRequest, RemoteCopyEvent, RemotePairingDismissRequest, RemotePairingShowRequest,
    RemoteRequest, RemoteRevokeRequest, SideSheetProjectOpenRequest,
    SideSheetProjectPageOpenRequest, SideSheetResizeEvent, SideSheetSectionRequest,
    SideSheetStackActivateRequest, SideSheetStackCloseRequest, SideSheetStackCreateRequest,
    WindowDragRegionEvent,
};
pub use vmux_layout::{Browser, Loading};
pub use window_drag::{WindowDragRegion, WindowDragRegions};

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod appearance;
mod command;
mod dom_snapshot;
mod frame_rate;
mod history_swipe;
mod host;
mod host_focus;
mod infrastructure;
mod input;
mod layout_placement;
mod native_bridge;
mod navigation;
mod page;
mod page_life;
mod platform;
mod present;
mod scroll;
mod snapshot;
mod state;
mod tool;
mod viewport;
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

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct HostedPageLifecycle {
    pub mount_revision: u64,
    pub remount_revision: u64,
    pub navigation_revision: u64,
    pub frames_applied: u64,
    pub frame_revision_delivered: u64,
    pub frame_revision_applied: u64,
    pub frame_revision_painted: u64,
    pub frame_pending: bool,
    pub ui_events: u64,
    pub host_events: u64,
    pub hydration_frame_count: Option<u64>,
    pub listeners: usize,
    pub measurements: usize,
}

impl HostedPageLifecycle {
    pub fn painted_after(&self, navigation_revision: u64) -> bool {
        self.navigation_revision > navigation_revision
            && self.frame_revision_painted >= self.navigation_revision
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BridgeOrigin(pub &'static str);

impl BridgeOrigin {
    const BUILTIN: &[&'static str] = &[
        "chat.mistral.ai",
        "chat.local.mistral.ai:8443",
        "chromewebstore.google.com",
    ];
}

pub struct BrowserPlugin;

impl BrowserPlugin {
    fn scheme_routes(embedded_scheme: &str, manifests: &[PageManifest]) -> Vec<CefSchemeRoute> {
        let mut routes = Vec::new();
        for manifest in manifests {
            let Some((scheme, _)) = manifest.url.split_once("://") else {
                continue;
            };
            if scheme.is_empty() || scheme == embedded_scheme || matches!(scheme, "http" | "https")
            {
                continue;
            }
            if routes
                .iter()
                .any(|existing: &CefSchemeRoute| existing.scheme == scheme)
            {
                continue;
            }
            let host = manifest.embedded_host();
            let file_scheme = scheme == "file";
            routes.push(CefSchemeRoute {
                scheme: scheme.to_string(),
                host: host.host.clone(),
                default_document: host.default_document,
                trusted_ipc: file_scheme,
                extension_free: file_scheme,
                register_handler: file_scheme,
                strip_self_host: scheme == host.host,
            });
        }
        routes
    }

    pub fn configure_schedule(app: &mut App) -> &mut App {
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
        .configure_sets(
            Update,
            bevy_cef::prelude::UiEventSystems::Flush
                .before(ReadCommandRequests)
                .before(PageOpenSet::ResolveTarget),
        )
        .configure_sets(
            Update,
            CefSystems::CreateAndResize.after(ReadCommandRequests),
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
        for authority in BridgeOrigin::BUILTIN {
            app.world_mut().spawn(BridgeOrigin(authority));
        }
        let embedded_scheme = compile_time_cef_embedded_scheme().to_string();
        let mut manifests = app.world_mut().query::<&PageManifest>();
        let page_manifests: Vec<PageManifest> = manifests.iter(app.world()).copied().collect();
        let embedded_hosts = CefEmbeddedHosts(
            page_manifests
                .iter()
                .map(PageManifest::embedded_host)
                .collect(),
        );
        let scheme_routes = Self::scheme_routes(&embedded_scheme, &page_manifests);
        let mut origins = app.world_mut().query::<&BridgeOrigin>();
        let bridge_origins: Vec<String> = origins
            .iter(app.world())
            .map(|origin| origin.0.to_string())
            .collect();
        Self::configure_schedule(app)
            .add_systems(Startup, sync_embedded_page_config)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
            .add_plugins((
                CefPlugin {
                    command_line_config: host::CefStartup::command_line(),
                    root_cache_path: host::CefStartup::root_cache_path(),
                    locale: startup_locale.value().to_string(),
                    accept_language_list: startup_accept_language_list,
                    embedded_scheme,
                    embedded_hosts,
                    scheme_routes,
                    bridge_origins,
                    #[cfg(target_os = "macos")]
                    os_crypt_key_provider: host::CefStartup::os_crypt_key_provider(),
                    ..default()
                },
                UiEventPlugin::<(
                    HeaderBackRequest,
                    HeaderForwardRequest,
                    HeaderReloadRequest,
                    HeaderStopRequest,
                    HeaderAddressFocusRequest,
                    SideSheetStackActivateRequest,
                    SideSheetStackCloseRequest,
                    SideSheetStackCreateRequest,
                    SideSheetProjectOpenRequest,
                    SideSheetProjectPageOpenRequest,
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
                history_swipe::HistorySwipePlugin,
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

fn sync_embedded_page_config(
    manifests: Query<&PageManifest>,
    origins: Query<&BridgeOrigin>,
    config: Option<ResMut<CefEmbeddedPageConfig>>,
) {
    let embedded_scheme = compile_time_cef_embedded_scheme().to_string();
    let page_manifests: Vec<PageManifest> = manifests.iter().copied().collect();
    let embedded_hosts = CefEmbeddedHosts(
        page_manifests
            .iter()
            .map(PageManifest::embedded_host)
            .collect(),
    );
    let scheme_routes = BrowserPlugin::scheme_routes(&embedded_scheme, &page_manifests);
    let mut bridge_origins = Vec::new();
    for origin in &origins {
        if !bridge_origins.iter().any(|existing| existing == origin.0) {
            bridge_origins.push(origin.0.to_string());
        }
    }
    let next = CefEmbeddedPageConfig::new(embedded_scheme, embedded_hosts)
        .with_scheme_routes(scheme_routes)
        .with_bridge_origins(bridge_origins);
    if let Some(mut current) = config {
        *current = next.clone();
    }
    bevy_cef_core::prelude::try_set_cef_embedded_page_config(next);
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
