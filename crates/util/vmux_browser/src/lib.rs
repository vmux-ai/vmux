#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_cef::prelude::*;
use bevy_cef_core::prelude::{CefEmbeddedHost, CefEmbeddedHosts, CefEmbeddedPageConfig};
pub use command::{
    NavigationOperation, NavigationRequest, OpenRequest, ShowDevToolsRequest, ZoomChange,
    ZoomRequest,
};
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
use vmux_ecs::{PageOpenSet, page::PageReady};
use vmux_layout::PendingWebviewReveal;
use vmux_layout::event::{
    HeaderAddressFocusRequest, HeaderBackRequest, HeaderForwardRequest, HeaderReloadRequest,
    RemoteCopyEvent, RemotePairingDismissRequest, RemotePairingShowRequest, RemoteRequest,
    RemoteRevokeRequest, SideSheetProjectOpenRequest, SideSheetProjectPageOpenRequest,
    SideSheetResizeEvent, SideSheetSectionRequest, SideSheetStackActivateRequest,
    SideSheetStackCloseRequest, SideSheetStackCreateRequest, WindowDragRegionEvent,
};
pub use vmux_layout::{Browser, Loading};
pub use window_drag::WindowDragRegion;

pub(crate) struct Feature;

impl vmux_feature::FeatureManifestSource for Feature {
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

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct HostedPageLifecycle {
    pub mount_revision: u64,
    pub remount_revision: u64,
    pub navigation_revision: u64,
    pub frames_applied: u64,
    pub frame_revision_delivered: u64,
    pub frame_revision_applied: u64,
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
            && self.frame_revision_applied >= self.navigation_revision
    }
}

pub struct BrowserPlugin;

impl BrowserPlugin {
    fn embedded_hosts(registry: &vmux_feature::FeatureRegistry) -> CefEmbeddedHosts {
        let mut hosts = Vec::new();
        for manifest in registry.manifests() {
            for page in &manifest.pages {
                let host = page
                    .asset_host()
                    .expect("page manifest requires asset_host");
                hosts.push(CefEmbeddedHost {
                    host: host.to_string(),
                    default_document: format!("{}/index.html", host.trim().trim_matches('/')),
                });
            }
        }
        CefEmbeddedHosts(hosts)
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
        Self::configure_schedule(app)
            .add_message::<bevy_cef_core::prelude::WebviewCommittedNavigationEvent>()
            .add_message::<WebviewLoadCompleted>()
            .add_plugins(vmux_layout::LayoutContractPlugin)
            .add_plugins((
                CefPlugin {
                    command_line_config: host::CefStartup::command_line(),
                    root_cache_path: host::CefStartup::root_cache_path(),
                    locale: startup_locale.value().to_string(),
                    accept_language_list: startup_accept_language_list,
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

    fn finish(&self, app: &mut App) {
        let hosts = Self::embedded_hosts(app.world().resource::<vmux_feature::FeatureRegistry>());
        let mut config = app.world_mut().resource_mut::<CefEmbeddedPageConfig>();
        *config = CefEmbeddedPageConfig::new(config.scheme.clone(), hosts);
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

#[cfg(test)]
mod embedded_registration {
    use super::*;
    use vmux_feature::AppFeatureExt;

    struct EmbeddedRegistration;

    impl Plugin for EmbeddedRegistration {
        fn build(&self, app: &mut App) {
            app.insert_resource(CefEmbeddedPageConfig::default());
        }

        fn finish(&self, app: &mut App) {
            BrowserPlugin.finish(app);
        }
    }

    struct Pages;

    impl vmux_feature::FeatureManifestSource for Pages {
        const SOURCE: &'static str = r#"(pages: [
            (url: "vmux://sessions/", title: "Sessions"),
            (url: "file://", manifest_url: "vmux://files/", title: "Files"),
            (url: "git://", title: "Git"),
        ])"#;
    }

    #[test]
    fn embedded_hosts_use_the_frozen_declarations_including_late_features() {
        let mut app = App::new();
        app.add_plugins((vmux_feature::FeatureRegistryPlugin, EmbeddedRegistration))
            .register_feature::<Pages>();
        app.finish();
        let config = app.world().resource::<CefEmbeddedPageConfig>();
        assert_eq!(
            config.hosts.0,
            [
                CefEmbeddedHost {
                    host: "sessions".into(),
                    default_document: "sessions/index.html".into()
                },
                CefEmbeddedHost {
                    host: "files".into(),
                    default_document: "files/index.html".into()
                },
                CefEmbeddedHost {
                    host: "git".into(),
                    default_document: "git/index.html".into()
                },
            ]
        );
    }
}
