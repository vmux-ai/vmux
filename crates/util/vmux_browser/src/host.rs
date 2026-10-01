use crate::present::CommandBarWindowedFrame;
pub(crate) use agent::{
    AgentBrowserGoBack, AgentBrowserGoForward, AgentBrowserHistorySearch, AgentBrowserNavigate,
    AgentBrowserPlugin, AgentBrowserScroll, AgentBrowserSnapshot,
};
pub use agent_pane::AgentBrowserResolve;
#[cfg(not(target_os = "macos"))]
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
#[cfg(not(target_os = "macos"))]
use bevy_cef::prelude::Browsers;
use bevy_cef_core::prelude::CommandLineConfig;
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};
use vmux_command::CommandBarPanelActive;
use vmux_flex::prelude::*;
use vmux_layout::{
    Header, Open, UpdateState, bookmark::BookmarkContextMenuActive, overlay::LayoutOverlayActive,
    side_sheet::SideSheet,
};
use vmux_setting::AppSettings;

use vmux_api::protocol::{AgentCommandResult, AgentRequestId, ClientMessage};
use vmux_ecs::service::ServiceRequest;

mod agent;
mod agent_pane;

#[derive(Clone, Copy, Debug, Message)]
pub struct WebviewLoadCompleted {
    pub webview: Entity,
}

pub(crate) struct CefStartup;

impl CefStartup {
    pub(crate) fn command_line() -> CommandLineConfig {
        CommandLineConfig {
            switches: vmux_ecs::profile::Profile::current()
                .cef_keychain_switches()
                .to_vec(),
            switch_values: vec![("disable-features", "BackForwardCache")],
        }
    }

    pub(crate) fn root_cache_path() -> Option<String> {
        vmux_ecs::profile::ProfilePaths::current().cef_cache()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn os_crypt_key_provider() -> Option<bevy_cef::CefOsCryptKeyProvider> {
        Some(Self::os_crypt_key)
    }

    #[cfg(target_os = "macos")]
    fn os_crypt_key() -> Result<bevy_cef::CefOsCryptKey, String> {
        let key = vmux_ecs::profile::safe_storage::SafeStorage::browser_key()
            .map_err(|error| error.to_string())?;
        Ok(bevy_cef::CefOsCryptKey::new(*key.as_bytes()))
    }
}

type CefPointerRegionRow<'a> = (
    Option<&'a Header>,
    Option<&'a SideSheet>,
    &'a Node,
    &'a ComputedNode,
    Option<&'a Visibility>,
    bool,
);

type CefPointerRegionQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Header>,
        Option<&'static SideSheet>,
        &'static Node,
        &'static ComputedNode,
        Option<&'static Visibility>,
        Has<Open>,
    ),
    Or<(With<Header>, With<SideSheet>)>,
>;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CefPointerRegions<'w, 's> {
    rows: CefPointerRegionQuery<'w, 's>,
}

impl CefPointerRegions<'_, '_> {
    pub(crate) fn contains(&self, cursor: Vec2) -> bool {
        for row in self.rows.iter() {
            if CefPointerHitRect::from_row(row).contains(cursor) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CefPointerHitRect {
    pub(crate) rect: ComputedNode,
    pub(crate) interactive: bool,
}

pub(crate) static NATIVE_LAYOUT_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);

impl CefPointerHitRect {
    fn from_row(row: CefPointerRegionRow<'_>) -> Self {
        let (header, side_sheet, node, &rect, visibility, open) = row;
        let interactive = (header.is_some() || side_sheet.is_some())
            && open
            && node.display != Display::None
            && !matches!(visibility, Some(Visibility::Hidden))
            && !rect.is_empty();
        Self { rect, interactive }
    }

    pub(crate) fn contains(self, point: Vec2) -> bool {
        self.interactive && self.rect.contains(point)
    }
}

pub(crate) type LayoutPointerCapture = Or<(
    With<BookmarkContextMenuActive>,
    With<CommandBarPanelActive>,
    With<LayoutOverlayActive>,
)>;

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub(crate) struct LayoutHoverRefreshState {
    pub(crate) sequence: u64,
    pub(crate) position: Option<Vec2>,
    pub(crate) in_region: bool,
}

#[cfg(not(target_os = "macos"))]
impl LayoutHoverRefreshState {
    pub(crate) fn reset(
        &mut self,
        browsers: &Browsers,
        buttons: &ButtonInput<MouseButton>,
        layout: Entity,
    ) {
        if self.in_region {
            browsers.send_mouse_move(
                &layout,
                buttons.get_pressed(),
                self.position.unwrap_or_default(),
                true,
            );
        }
        *self = Self::default();
    }
}

#[derive(Default)]
pub(crate) struct WindowedHoverRefreshState {
    pub(crate) entity: Option<Entity>,
    pub(crate) position: Option<Vec2>,
}

pub(crate) const LAYOUT_INPUT_BURST: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Component, Default)]
pub(crate) struct LayoutFrameRateState {
    pub(crate) native_sequence: u64,
    pub(crate) last_input: Option<std::time::Instant>,
    pub(crate) last_emit: Option<std::time::Instant>,
    pub(crate) dragging_layout: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CommandBarRoute {
    pub(crate) generation: u64,
    pub(crate) owns_input: bool,
    pub(crate) frame: Option<CommandBarWindowedFrame>,
    pub(crate) scale: f32,
}

impl CommandBarRoute {
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn current() -> Self {
        *NATIVE_COMMAND_BAR_ROUTE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn local_position(self, cursor: Vec2) -> Option<Vec2> {
        if !self.owns_input {
            return None;
        }
        let frame = self.frame?;
        if cursor.x < frame.left_px
            || cursor.x > frame.left_px + frame.width_px
            || cursor.y < frame.top_px
            || cursor.y > frame.top_px + frame.height_px
        {
            return None;
        }
        let scale = self.scale.max(1.0e-6);
        Some(Vec2::new(
            (cursor.x - frame.left_px) / scale,
            (cursor.y - frame.top_px) / scale,
        ))
    }
}

pub(crate) static NATIVE_COMMAND_BAR_ROUTE: LazyLock<Mutex<CommandBarRoute>> =
    LazyLock::new(|| Mutex::new(CommandBarRoute::default()));

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutWindowPadding {
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
    pub(crate) left: f32,
}

impl LayoutWindowPadding {
    pub(crate) fn from_node(node: &Node) -> Self {
        Self {
            top: Self::px(node.padding.top),
            right: Self::px(node.padding.right),
            bottom: Self::px(node.padding.bottom),
            left: Self::px(node.padding.left),
        }
    }

    pub(crate) fn from_settings(settings: &AppSettings) -> Self {
        Self {
            top: settings.layout.window.pad_top(),
            right: settings.layout.window.pad_right(),
            bottom: settings.layout.window.pad_bottom(),
            left: settings.layout.window.pad_left(),
        }
    }

    fn px(value: Val) -> f32 {
        match value {
            Val::Px(px) => px,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutFixedOffsets {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) height: f32,
}

impl LayoutFixedOffsets {
    pub(crate) fn from_node(rect: &ComputedNode, window_width_px: f32) -> Option<Self> {
        if rect.is_empty() || window_width_px <= 0.0 {
            return None;
        }

        let logical = rect.to_logical();
        let window_width = window_width_px * rect.inverse_scale_factor.max(1.0e-6);

        Some(Self {
            left: logical.min().x,
            top: logical.min().y,
            right: window_width - logical.max().x,
            height: logical.size.y,
        })
    }
}

pub(crate) struct UpdateProjection;

impl UpdateProjection {
    pub(crate) fn should_emit(
        current: &UpdateState,
        last: &Option<UpdateState>,
        page_ready_changed: bool,
    ) -> bool {
        last.as_ref() != Some(current) || (page_ready_changed && *current != UpdateState::Idle)
    }
}

#[derive(Component, Clone, Debug)]
pub(crate) struct PageOpenFallbackDeferred;

#[derive(Component, Clone, Debug)]
pub(crate) struct PageOpenAwaitSnapshot {
    pub(crate) started: std::time::Duration,
}

pub(crate) struct PageOpenResponse;

impl PageOpenResponse {
    pub(crate) fn from_result(
        request_id: Option<[u8; 16]>,
        result: Result<(), String>,
    ) -> Option<ServiceRequest> {
        let request_id = request_id?;
        let result = match result {
            Ok(()) => AgentCommandResult::Ok,
            Err(message) => AgentCommandResult::Error(message),
        };
        Some(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(request_id),
            result,
        }))
    }
}

#[derive(Component, Clone)]
pub(crate) struct PendingNavigationSnapshot {
    pub(crate) webview: Entity,
    pub request_id: [u8; 16],
    pub started: std::time::Duration,
    pub saw_loading: bool,
    pub pane: Option<String>,
}

#[derive(Message)]
pub(crate) struct PendingNavigationUpdate {
    pub(crate) webview: Entity,
    pub(crate) pending: Option<PendingNavigationSnapshot>,
}

impl PendingNavigationUpdate {
    pub(crate) fn set(
        webview: Entity,
        request_id: [u8; 16],
        started: std::time::Duration,
        pane: Option<String>,
    ) -> Self {
        Self {
            webview,
            pending: Some(PendingNavigationSnapshot {
                webview,
                request_id,
                started,
                saw_loading: false,
                pane,
            }),
        }
    }

    pub(crate) fn clear(webview: Entity) -> Self {
        Self {
            webview,
            pending: None,
        }
    }
}
