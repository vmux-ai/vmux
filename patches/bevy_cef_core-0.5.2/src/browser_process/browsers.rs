use crate::browser_process::BrpHandler;
use crate::browser_process::ClientHandlerBuilder;
use crate::browser_process::client_handler::FocusCanceler;
use crate::browser_process::client_handler::{
    BinEmitEventHandler, BinIpcEventRaw, IpcEventRaw, JsEmitEventHandler,
};
use crate::prelude::*;
use async_channel::{Sender, TryRecvError};
use bevy::input::ButtonState;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_remote::BrpMessage;
#[cfg(target_os = "macos")]
use cef::Rect;
use cef::{
    Browser, BrowserHost, BrowserSettings, CefString, Client, CompositionUnderline,
    DictionaryValue, ImplBrowser, ImplBrowserHost, ImplDictionaryValue, ImplFrame, ImplListValue,
    ImplProcessMessage, ImplRequestContext, MouseButtonType, ProcessId, Range, RequestContext,
    RequestContextSettings, WindowInfo, binary_value_create, browser_host_create_browser_sync,
    dictionary_value_create, process_message_create, register_scheme_handler_factory,
};
use cef_dll_sys::{cef_event_flags_t, cef_mouse_button_type_t};
#[allow(deprecated)]
use raw_window_handle::RawWindowHandle;
use std::cell::Cell;
#[cfg(target_os = "macos")]
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;
use std::time::Duration;

mod devtool_render_handler;
mod keyboard;

use crate::browser_process::browsers::devtool_render_handler::DevToolRenderHandlerBuilder;
use crate::browser_process::display_handler::{
    DisplayHandlerBuilder, SystemCursorIconSenderInner, WebviewCefStateSenderInner,
};
use crate::browser_process::life_span_handler::{LifeSpanHandlerBuilder, WebviewPopupSenderInner};
use crate::browser_process::load_handler::{
    WebviewCommittedNavigationSenderInner, WebviewLoadHandlerBuilder,
    WebviewLoadingStateSenderInner,
};
use crate::browser_process::renderer_handler::SharedDeviceScaleFactor;
use crate::browser_process::request_handler::RequestHandlerBuilder;
pub use keyboard::*;

/// Default CEF [`BrowserSettings::background_color`] ARGB.
/// Opaque white so normal web pages render correctly.
/// UI overlay webviews pass `0x00000000` for transparency.
const CEF_OSR_BACKGROUND_COLOR_ARGB: u32 = 0xFFFFFFFF;
pub const DEFAULT_WINDOWLESS_FRAME_RATE: i32 = 120;

/// Frame rate for a visible-but-unfocused host window (another app is focused).
/// CEF keeps painting animations, but at a fraction of the focused rate to cut idle CPU.
pub const BACKGROUND_WINDOWLESS_FRAME_RATE: i32 = 30;

/// Frame rate for a hidden host window. CEF still needs a nonzero rate to keep timers alive.
pub const HIDDEN_WINDOWLESS_FRAME_RATE: i32 = 1;

static REGISTER_GLOBAL_SCHEME_HANDLER_FACTORIES: Once = Once::new();

/// Disk profile root for [`RequestContextSettings::cache_path`], aligned with `CefPlugin::root_cache_path` in the `bevy_cef` crate.
/// Inserted by that plugin; when `bevy_cef_core` is used without it, initialize via `init_resource` (default `None`).
#[derive(Resource, Clone, Debug, Default)]
pub struct CefDiskProfileRoot(pub Option<String>);

pub struct WebviewBrowser {
    pub client: Browser,
    pub host: BrowserHost,
    pub size: SharedViewSize,
    pub device_scale: SharedDeviceScaleFactor,
    windowless_frame_rate: Cell<i32>,
    hidden: Cell<bool>,
    /// Last applied windowed (native) frame in points `(x, y, w, h)`. Used to skip redundant
    /// `setFrame`/`was_resized` calls — re-resizing CEF to the same size every frame clears its
    /// surface and leaves it blank until the next real paint.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    last_frame: Cell<Option<(f64, f64, f64, f64)>>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    last_corner_radius: Cell<Option<f64>>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    last_corner_radius_all_corners: Cell<Option<bool>>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    last_focus_ring: Cell<Option<(f64, f64, f64, f64)>>,
    /// True for native (windowed) browsers. `set_focus(true)` makes a windowed browser's `NSView`
    /// the macOS first responder, stealing keyboard from winit so Bevy shortcuts die. Keyboard is
    /// routed via `CefKeyboardTarget` forwarding instead, so windowed browsers must not be focused.
    windowed: bool,
    allow_native_focus: bool,
    #[cfg(target_os = "macos")]
    native_liquid_glass: Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>>,
    #[cfg(target_os = "macos")]
    corner_cover: RefCell<Option<objc2::rc::Retained<objc2_quartz_core::CAShapeLayer>>>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    last_corner_cover: Cell<Option<(i32, bool, i32, i32)>>,
}

pub struct Browsers {
    browsers: HashMap<Entity, WebviewBrowser>,
    sender: TextureSender,
    receiver: TextureReceiver,
    accel_sender: AcceleratedSender,
    accel_receiver: AcceleratedReceiver,
    /// Lazily created when [`Self::create_browser`] is called with a non-empty disk profile root.
    /// Shared by all webviews so multiple panes use one cookie store and avoid conflicting contexts on the same path.
    shared_disk_context: Option<RequestContext>,
}

impl Default for Browsers {
    fn default() -> Self {
        let (sender, receiver) = async_channel::unbounded::<RenderTextureMessage>();
        let (accel_sender, accel_receiver) = async_channel::unbounded::<AcceleratedFrame>();
        Browsers {
            browsers: HashMap::default(),
            sender,
            receiver,
            accel_sender,
            accel_receiver,
            shared_disk_context: None,
        }
    }
}

impl Browsers {
    #[allow(clippy::too_many_arguments)]
    pub fn create_browser(
        &mut self,
        webview: Entity,
        _uri: &str,
        webview_size: Vec2,
        device_scale_factor: f32,
        requester: Requester,
        ipc_event_sender: Sender<IpcEventRaw>,
        bin_ipc_event_sender: Sender<BinIpcEventRaw>,
        brp_sender: Sender<BrpMessage>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        webview_loading_state_sender: WebviewLoadingStateSenderInner,
        webview_committed_nav_sender: WebviewCommittedNavigationSenderInner,
        webview_cef_state_sender: WebviewCefStateSenderInner,
        webview_popup_sender: WebviewPopupSenderInner,
        texture_wake: Option<TextureWake>,
        initialize_scripts: &[String],
        _window_handle: Option<RawWindowHandle>,
        disk_profile_root: Option<&str>,
        background_color: Option<u32>,
        windowless_frame_rate: i32,
        windowed: bool,
        shared_texture: bool,
        native_liquid_glass: bool,
        allow_native_focus: bool,
    ) {
        let _ = native_liquid_glass;
        // Only consumed by the macOS windowless window-info below; non-macOS OSR is CPU-only.
        let _ = shared_texture;
        let windowless_frame_rate = normalize_windowless_frame_rate(windowless_frame_rate);
        info!(
            "cef_create_browser webview={webview:?} uri={_uri} size={}x{} scale={device_scale_factor} windowed={windowed} bg={background_color:?} fps={windowless_frame_rate} native_liquid_glass={native_liquid_glass} allow_native_focus={allow_native_focus}",
            webview_size.x, webview_size.y
        );
        webview_debug_log(format!(
            "Browsers::create_browser entity={webview:?} uri={_uri} size={webview_size:?} scale={device_scale_factor} disk_profile={} bg={background_color:?} fps={windowless_frame_rate} allow_native_focus={allow_native_focus}",
            disk_profile_root.is_some_and(|s| !s.trim().is_empty())
        ));
        let size = Rc::new(Cell::new(webview_size));
        let device_scale = Rc::new(Cell::new(device_scale_factor));
        let mut client = self.client_handler(
            webview,
            size.clone(),
            device_scale.clone(),
            ipc_event_sender,
            bin_ipc_event_sender,
            brp_sender,
            system_cursor_icon_sender,
            webview_loading_state_sender,
            webview_committed_nav_sender,
            webview_cef_state_sender,
            webview_popup_sender,
            texture_wake,
            !allow_native_focus,
        );

        // `RequestContext::register_scheme_handler_factory` is not always enough: some navigations
        // still consult the process-wide factories registered via `cef_register_scheme_handler_factory`
        // (see CEF capi). Without this, custom embedded scheme URLs can yield ERR_UNKNOWN_URL_SCHEME
        // despite `on_register_custom_schemes` and per-context registration.
        let requester_for_global = requester.clone();
        REGISTER_GLOBAL_SCHEME_HANDLER_FACTORIES.call_once(move || {
            let cfg = resolved_cef_embedded_page_config();
            let ct = compile_time_cef_embedded_scheme();
            if cfg.scheme != ct {
                bevy::log::warn!(
                    "bevy_cef_core: runtime embedded scheme {:?} != build-time scheme {:?}; rebuild bevy_cef_core with the same scheme as CefPlugin.embedded_scheme (optional env BEVY_CEF_EMBEDDED_SCHEME during that build)",
                    cfg.scheme.as_str(),
                    ct
                );
            }
            let emb_scheme = cfg.scheme.clone();
            let mut cef_factory = LocalSchemaHandlerBuilder::build(requester_for_global.clone());
            let ok_cef = register_scheme_handler_factory(
                Some(&SCHEME_CEF.into()),
                Some(&HOST_CEF.into()),
                Some(&mut cef_factory),
            );
            webview_debug_log(format!(
                "register_scheme_handler_factory cef://localhost ok={ok_cef}"
            ));
            assert_eq!(
                ok_cef, 1,
                "cef_register_scheme_handler_factory(cef) failed with code {ok_cef}"
            );
            let mut embedded_factory =
                LocalSchemaHandlerBuilder::build(requester_for_global.clone());
            let ok_embedded = register_scheme_handler_factory(
                Some(&emb_scheme.as_str().into()),
                None,
                Some(&mut embedded_factory),
            );
            webview_debug_log(format!(
                "register_scheme_handler_factory {}://* ok={ok_embedded}",
                emb_scheme
            ));
            assert_eq!(
                ok_embedded, 1,
                "cef_register_scheme_handler_factory(embedded page scheme) failed with code {ok_embedded}"
            );
            let mut files_factory = LocalSchemaHandlerBuilder::build(requester_for_global);
            let ok_files = register_scheme_handler_factory(
                Some(&crate::util::FILES_SCHEME.into()),
                None,
                Some(&mut files_factory),
            );
            webview_debug_log(format!(
                "register_scheme_handler_factory {}://* ok={ok_files}",
                crate::util::FILES_SCHEME
            ));
            assert_eq!(
                ok_files, 1,
                "cef_register_scheme_handler_factory(files scheme) failed with code {ok_files}"
            );
        });

        // Holds the per-browser ephemeral context when not using `shared_disk_context`.
        #[allow(unused_assignments)]
        let mut ephemeral_local: Option<RequestContext> = None;
        let context_for_browser = match disk_profile_root.filter(|s| !s.trim().is_empty()) {
            Some(root) => {
                if self.shared_disk_context.is_none() {
                    self.ensure_shared_disk_context(requester, root);
                } else {
                    let _ = requester;
                }
                self.shared_disk_context.as_mut()
            }
            None => {
                ephemeral_local = Self::ephemeral_request_context(requester);
                ephemeral_local.as_mut()
            }
        };

        #[cfg(target_os = "macos")]
        let window_info = {
            let parent = match _window_handle {
                Some(RawWindowHandle::AppKit(handle)) => handle.ns_view.as_ptr(),
                _ => std::ptr::null_mut(),
            };
            if windowed {
                WindowInfo::default().set_as_child(
                    parent,
                    &Rect {
                        x: 0,
                        y: 0,
                        width: webview_size.x.max(1.0) as i32,
                        height: webview_size.y.max(1.0) as i32,
                    },
                )
            } else {
                WindowInfo {
                    windowless_rendering_enabled: true as _,
                    external_begin_frame_enabled: false as _,
                    parent_view: parent,
                    shared_texture_enabled: shared_texture as _,
                    ..Default::default()
                }
            }
        };
        #[cfg(not(target_os = "macos"))]
        let window_info = WindowInfo {
            windowless_rendering_enabled: true as _,
            external_begin_frame_enabled: false as _,
            #[cfg(target_os = "windows")]
            parent_window: match _window_handle {
                Some(RawWindowHandle::Win32(handle)) => cef_dll_sys::HWND(handle.hwnd.get() as _),
                _ => cef_dll_sys::HWND(std::ptr::null_mut()),
            },
            ..Default::default()
        };

        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&_uri.into()),
            Some(&BrowserSettings {
                windowless_frame_rate,
                background_color: background_color.unwrap_or(CEF_OSR_BACKGROUND_COLOR_ARGB),
                ..Default::default()
            }),
            Self::create_extra_info(initialize_scripts).as_mut(),
            context_for_browser,
        )
        .expect("Failed to create browser");
        let host = browser.host().expect("Failed to get browser host");
        if !windowed {
            host.was_hidden(0);
        }
        #[cfg(target_os = "macos")]
        let native_liquid_glass =
            (windowed && native_liquid_glass).then(|| Self::create_native_liquid_glass(&host));
        #[cfg(target_os = "macos")]
        let native_liquid_glass = native_liquid_glass.flatten();
        let webview_browser = WebviewBrowser {
            host,
            client: browser,
            size,
            device_scale,
            windowless_frame_rate: Cell::new(windowless_frame_rate),
            hidden: Cell::new(false),
            last_frame: Cell::new(None),
            last_corner_radius: Cell::new(None),
            last_corner_radius_all_corners: Cell::new(None),
            last_focus_ring: Cell::new(None),
            windowed,
            allow_native_focus,
            #[cfg(target_os = "macos")]
            native_liquid_glass,
            #[cfg(target_os = "macos")]
            corner_cover: RefCell::new(None),
            last_corner_cover: Cell::new(None),
        };
        self.browsers.insert(webview, webview_browser);
        webview_debug_log(format!(
            "Browsers::create_browser inserted entity={webview:?}"
        ));
    }

    /// Returns `true` if [`Self::create_browser`] has already succeeded for this webview entity.
    #[inline]
    pub fn has_browser(&self, webview: Entity) -> bool {
        self.browsers.contains_key(&webview)
    }

    #[inline]
    pub fn is_windowed(&self, webview: &Entity) -> Option<bool> {
        self.browsers.get(webview).map(|browser| browser.windowed)
    }

    /// `true` when [`Self::emit_event`] can send (main frame exists). If this is `false`,
    /// [`Self::emit_event`] is a no-op — callers must not treat the payload as delivered.
    #[inline]
    pub fn host_emit_ready(&self, webview: &Entity) -> bool {
        self.browsers
            .get(webview)
            .is_some_and(|b| b.client.main_frame().is_some())
    }

    #[inline]
    pub fn set_osr_not_hidden(&self, webview: &Entity) {
        if let Some(b) = self.browsers.get(webview) {
            if b.hidden.replace(false) {
                b.host.was_hidden(0);
            }
        }
    }

    #[inline]
    pub fn set_osr_hidden(&self, webview: &Entity) {
        if let Some(b) = self.browsers.get(webview) {
            if !b.hidden.replace(true) {
                b.host.was_hidden(1);
            }
        }
    }

    pub fn set_all_osr_hidden(&self) {
        for browser in self.browsers.values() {
            if !browser.hidden.replace(true) {
                browser.host.was_hidden(1);
            }
            browser.host.set_focus(false as _);
        }
    }

    pub fn set_windowless_frame_rate(&self, webview: &Entity, frame_rate: i32) {
        let frame_rate = normalize_windowless_frame_rate(frame_rate);
        if let Some(browser) = self.browsers.get(webview)
            && browser.windowless_frame_rate.replace(frame_rate) != frame_rate
        {
            browser.host.set_windowless_frame_rate(frame_rate);
        }
    }

    /// Align CEF focus with the tiled pane that has keyboard target / `Active` in the host app.
    ///
    /// Windowless (OSR) browsers may not composite visible frames until the host calls
    /// `CefBrowserHost::set_focus`; without this, the active pane can stay stuck until the first
    /// mouse click or move.
    ///
    /// `auxiliary_osr_focus` is for **additional** visible webviews that must keep compositing
    /// while another pane is active (e.g. a history split next to the main browser). Keyboard
    /// routing in the host app uses the `CefKeyboardTarget` component, not CEF `set_focus`.
    ///
    /// Chromium ties **clipboard shortcuts** (⌘C / ⌘V / …) to the browser that last received
    /// `set_focus(true)`. We therefore focus each auxiliary in order (so they can composite), then
    /// focus **`active` again last** so the main pane owns OSR focus for copy/paste.
    pub fn sync_osr_focus_to_active_pane(
        &self,
        active: Option<Entity>,
        auxiliary_osr_focus: &[Entity],
    ) {
        for browser in self.browsers.values() {
            if !browser.windowed {
                browser.host.set_focus(false as _);
            }
        }
        if let Some(a) = active
            && let Some(browser) = self.browsers.get(&a)
            && !browser.windowed
        {
            browser.host.set_focus(true as _);
        }
        for &h in auxiliary_osr_focus {
            if let Some(browser) = self.browsers.get(&h)
                && !browser.windowed
            {
                browser.host.set_focus(true as _);
            }
        }
        if let Some(a) = active
            && let Some(browser) = self.browsers.get(&a)
            && !browser.windowed
        {
            browser.host.set_focus(true as _);
        }
    }

    pub fn send_mouse_move<'a>(
        &self,
        webview: &Entity,
        buttons: impl IntoIterator<Item = &'a MouseButton>,
        position: Vec2,
        mouse_leave: bool,
    ) {
        // Route by webview entity. Requiring `focused_frame()` drops all input until CEF already
        // has focus — so the first click never reaches `set_focus`.
        if let Some(browser) = self.browsers.get(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers: modifiers_from_mouse_buttons(buttons),
            };
            browser
                .host
                .send_mouse_move_event(Some(&mouse_event), mouse_leave as _);
        }
    }

    pub fn send_mouse_click(
        &self,
        webview: &Entity,
        position: Vec2,
        button: PointerButton,
        mouse_up: bool,
    ) {
        if let Some(browser) = self.browsers.get(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers: match button {
                    PointerButton::Primary => cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON.0,
                    PointerButton::Secondary => cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON.0,
                    PointerButton::Middle => cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON.0,
                } as _, // No modifiers for simplicity
            };
            let mouse_button = match button {
                PointerButton::Secondary => cef_mouse_button_type_t::MBT_RIGHT,
                PointerButton::Middle => cef_mouse_button_type_t::MBT_MIDDLE,
                _ => cef_mouse_button_type_t::MBT_LEFT,
            };
            if !browser.windowed {
                browser.host.set_focus(true as _);
            }
            browser.host.send_mouse_click_event(
                Some(&mouse_event),
                MouseButtonType::from(mouse_button),
                mouse_up as _,
                1,
            );
        }
    }

    /// [`SendMouseWheelEvent`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowserHost.html#acd5d057bd5230baa9a94b7853ba755f7)
    pub fn send_mouse_wheel(&self, webview: &Entity, position: Vec2, delta: Vec2) {
        if let Some(browser) = self.browsers.get(webview) {
            let mouse_event = cef::MouseEvent {
                x: position.x as i32,
                y: position.y as i32,
                modifiers: 0,
            };
            browser
                .host
                .send_mouse_wheel_event(Some(&mouse_event), delta.x as _, delta.y as _);
        }
    }

    #[inline]
    pub fn send_key(&self, webview: &Entity, event: cef::KeyEvent) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.host.send_key_event(Some(&event));
        }
    }

    pub fn set_windowed_focus(&self, webview: &Entity, focused: bool) {
        if let Some(browser) = self.browsers.get(webview)
            && browser.windowed
            && browser.allow_native_focus
        {
            browser.host.set_focus(focused as _);
        }
    }

    /// Windowless/OSR: synthetic [`BrowserHost::send_key_event`] often does not run Chromium’s
    /// clipboard handling. When the chord is a plain **⌘C / ⌘V / ⌘X / ⌘A** (macOS) or **Ctrl+…**
    /// (Windows/Linux), forward to [`cef::Frame::copy`] / [`cef::Frame::paste`] / etc. on the
    /// focused frame instead of (or skipping) key delivery.
    ///
    /// Returns `true` if this key press was handled — callers should **not** also
    /// [`Self::send_key`] the same press.
    pub fn try_dispatch_clipboard_shortcut(
        &self,
        webview: &Entity,
        key_code: KeyCode,
        modifiers: u32,
        state: ButtonState,
    ) -> bool {
        if state != ButtonState::Pressed {
            return false;
        }
        if !Self::modifiers_plain_clipboard_chord(modifiers) {
            return false;
        }
        let Some(browser) = self.browsers.get(webview) else {
            return false;
        };
        let Some(frame) = browser.client.focused_frame() else {
            return false;
        };
        if frame.is_valid() == 0 {
            return false;
        }
        match key_code {
            KeyCode::KeyC => frame.copy(),
            KeyCode::KeyV => frame.paste(),
            KeyCode::KeyX => frame.cut(),
            KeyCode::KeyA => frame.select_all(),
            _ => return false,
        }
        true
    }

    fn modifiers_plain_clipboard_chord(modifiers: u32) -> bool {
        let shift = modifiers & (cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0 as u32) != 0;
        let alt = modifiers & (cef_event_flags_t::EVENTFLAG_ALT_DOWN.0 as u32) != 0;
        #[cfg(target_os = "macos")]
        {
            let cmd = modifiers & (cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32) != 0;
            let ctrl = modifiers & (cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32) != 0;
            cmd && !ctrl && !shift && !alt
        }
        #[cfg(not(target_os = "macos"))]
        {
            let ctrl = modifiers & (cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32) != 0;
            let cmd = modifiers & (cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32) != 0;
            ctrl && !cmd && !shift && !alt
        }
    }

    pub fn emit_event(&self, webview: &Entity, id: impl Into<String>, event: &serde_json::Value) {
        if let Some(mut process_message) =
            process_message_create(Some(&PROCESS_MESSAGE_HOST_EMIT.into()))
            && let Some(argument_list) = process_message.argument_list()
            && let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
            && crate::util::ipc_allowed_browser(&frame.url().into_string())
        {
            argument_list.set_string(0, Some(&id.into().as_str().into()));
            argument_list.set_string(1, Some(&event.to_string().as_str().into()));
            frame.send_process_message(
                ProcessId::from(cef_dll_sys::cef_process_id_t::PID_RENDERER),
                Some(&mut process_message),
            );
        };
    }

    /// Same as [`Self::emit_event`] but passes JSON already serialized (avoids parse + stringify for large payloads).
    pub fn emit_event_raw_json(&self, webview: &Entity, id: impl Into<String>, json_body: &str) {
        if let Some(mut process_message) =
            process_message_create(Some(&PROCESS_MESSAGE_HOST_EMIT.into()))
            && let Some(argument_list) = process_message.argument_list()
            && let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
            && crate::util::ipc_allowed_browser(&frame.url().into_string())
        {
            argument_list.set_string(0, Some(&id.into().as_str().into()));
            argument_list.set_string(1, Some(&json_body.into()));
            frame.send_process_message(
                ProcessId::from(cef_dll_sys::cef_process_id_t::PID_RENDERER),
                Some(&mut process_message),
            );
        };
    }

    #[allow(dead_code)]
    pub fn emit_event_bytes(&self, webview: &Entity, id: impl Into<String>, payload: &[u8]) {
        if let Some(mut process_message) =
            process_message_create(Some(&PROCESS_MESSAGE_BIN_HOST_EMIT.into()))
            && let Some(argument_list) = process_message.argument_list()
            && let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
            && crate::util::ipc_allowed_browser(&frame.url().into_string())
        {
            argument_list.set_string(0, Some(&id.into().as_str().into()));
            // CEF's BinaryValue rejects zero-length data. For unit-shaped payloads
            // we send only the id; the receiver (handle_bin_listen_message) treats
            // a missing binary arg as Vec::new().
            if !payload.is_empty()
                && let Some(mut binary) = binary_value_create(Some(payload))
            {
                argument_list.set_binary(1, Some(&mut binary));
            }
            frame.send_process_message(
                ProcessId::from(cef_dll_sys::cef_process_id_t::PID_RENDERER),
                Some(&mut process_message),
            );
        };
    }

    pub fn resize(&self, webview: &Entity, size: Vec2, device_scale_factor: f32) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.size.set(size);
            browser.device_scale.set(device_scale_factor);
            browser.host.notify_screen_info_changed();
            browser.host.was_resized();
        }
    }

    #[cfg(target_os = "macos")]
    fn create_native_liquid_glass(
        host: &BrowserHost,
    ) -> Option<objc2::rc::Retained<objc2_app_kit::NSGlassEffectView>> {
        use objc2::{ClassType, MainThreadMarker, runtime::AnyClass};
        use objc2_app_kit::{
            NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSWindowOrderingMode,
        };
        if AnyClass::get(c"NSGlassEffectView").is_none() {
            webview_debug_log("native_liquid_glass unavailable");
            return None;
        }
        let mtm = MainThreadMarker::new()?;
        let handle = host.window_handle();
        if handle.is_null() {
            webview_debug_log("native_liquid_glass missing host window handle");
            return None;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        let clear_color = NSColor::clearColor();
        Self::make_view_tree_transparent(view, &clear_color);
        let Some(parent) = (unsafe { view.superview() }) else {
            webview_debug_log("native_liquid_glass missing parent view");
            return None;
        };
        let frame = view.frame();
        let hidden = view.isHidden();
        let glass = NSGlassEffectView::new(mtm);
        glass.setStyle(NSGlassEffectViewStyle::Clear);
        glass.setTintColor(Some(&NSColor::clearColor()));
        let glass_view: &NSView = glass.as_super();
        Self::make_view_tree_transparent(glass_view, &clear_color);
        glass_view.setFrame(frame);
        glass_view.setHidden(hidden);
        parent.addSubview_positioned_relativeTo(
            glass_view,
            NSWindowOrderingMode::Above,
            Some(view),
        );
        view.removeFromSuperview();
        glass.setContentView(Some(view));
        view.setFrame(glass_view.bounds());
        webview_debug_log("native_liquid_glass attached");
        Some(glass)
    }

    #[cfg(target_os = "macos")]
    fn make_view_tree_transparent(
        view: &objc2_app_kit::NSView,
        clear_color: &objc2_app_kit::NSColor,
    ) {
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            Self::make_layer_tree_transparent(&layer, clear_color);
        }
        let subviews = view.subviews();
        for i in 0..subviews.count() {
            let child = subviews.objectAtIndex(i);
            Self::make_view_tree_transparent(&child, clear_color);
        }
    }

    #[cfg(target_os = "macos")]
    fn make_layer_tree_transparent(
        layer: &objc2_quartz_core::CALayer,
        clear_color: &objc2_app_kit::NSColor,
    ) {
        layer.setOpaque(false);
        layer.setBackgroundColor(Some(&clear_color.CGColor()));
        let Some(sublayers) = (unsafe { layer.sublayers() }) else {
            return;
        };
        for i in 0..sublayers.count() {
            let child = sublayers.objectAtIndex(i);
            Self::make_layer_tree_transparent(&child, clear_color);
        }
    }

    #[cfg(target_os = "macos")]
    fn refresh_windowed_transparency(browser: &WebviewBrowser, view: &objc2_app_kit::NSView) {
        use objc2::ClassType;
        if browser.native_liquid_glass.is_none() {
            return;
        }
        let clear_color = objc2_app_kit::NSColor::clearColor();
        Self::make_view_tree_transparent(view, &clear_color);
        if let Some(glass_view) = browser
            .native_liquid_glass
            .as_ref()
            .map(|glass| glass.as_super())
        {
            Self::make_view_tree_transparent(glass_view, &clear_color);
        }
    }

    #[cfg(target_os = "macos")]
    fn apply_view_tree_corner_radius(view: &objc2_app_kit::NSView, radius: f64, all_corners: bool) {
        use objc2_quartz_core::CACornerMask;
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            let all = CACornerMask::LayerMinXMinYCorner
                | CACornerMask::LayerMaxXMinYCorner
                | CACornerMask::LayerMinXMaxYCorner
                | CACornerMask::LayerMaxXMaxYCorner;
            let bottom = CACornerMask::LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner;
            layer.setCornerRadius(radius);
            layer.setMasksToBounds(true);
            layer.setMaskedCorners(if all_corners { all } else { bottom });
        }
    }

    #[cfg(target_os = "macos")]
    fn update_corner_cover(
        browser: &WebviewBrowser,
        view: &objc2_app_kit::NSView,
        radius: f64,
        all_corners: bool,
        color: Option<&objc2_app_kit::NSColor>,
    ) {
        use objc2_core_graphics::CGMutablePath;
        use objc2_quartz_core::{CAShapeLayer, kCAFillRuleEvenOdd};
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        let bounds = layer.bounds();
        let want_cover = all_corners && radius > 0.0;
        let key = (
            radius.round() as i32,
            all_corners,
            bounds.size.width.round() as i32,
            bounds.size.height.round() as i32,
        );
        if browser.last_corner_cover.get() == Some(key)
            && browser.corner_cover.borrow().is_some() == want_cover
        {
            return;
        }
        browser.last_corner_cover.set(Some(key));
        if !want_cover {
            if let Some(cover) = browser.corner_cover.borrow_mut().take() {
                cover.removeFromSuperlayer();
            }
            return;
        }
        let existing = browser.corner_cover.borrow().clone();
        let cover = match existing {
            Some(c) => c,
            None => {
                let c = CAShapeLayer::new();
                c.setZPosition(1.0);
                unsafe { c.setFillRule(kCAFillRuleEvenOdd) };
                *browser.corner_cover.borrow_mut() = Some(c.clone());
                c
            }
        };
        if let Some(color) = color {
            cover.setFillColor(Some(&color.CGColor()));
        }
        cover.setFrame(bounds);
        let path = CGMutablePath::new();
        unsafe {
            CGMutablePath::add_rect(Some(&path), std::ptr::null(), bounds);
            CGMutablePath::add_rounded_rect(Some(&path), std::ptr::null(), bounds, radius, radius);
        }
        cover.setPath(Some(&path));
        layer.addSublayer(&cover);
    }

    #[cfg(target_os = "macos")]
    pub fn set_windowed_corner_cover(
        &self,
        webview: &Entity,
        radius_px: f32,
        scale: f32,
        all_corners: bool,
        color_rgb: [f32; 3],
    ) {
        use objc2_app_kit::{NSColor, NSView};
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        let s = (scale as f64).max(1.0e-6);
        let radius = (radius_px as f64 / s).max(0.0);
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        let r = color_rgb[0].clamp(0.0, 1.0) as f64;
        let g = color_rgb[1].clamp(0.0, 1.0) as f64;
        let b = color_rgb[2].clamp(0.0, 1.0) as f64;
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
        Self::update_corner_cover(browser, view, radius, all_corners, Some(&color));
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_corner_cover(&self, _: &Entity, _: f32, _: f32, _: bool, _: [f32; 3]) {}

    #[cfg(target_os = "macos")]
    pub fn set_windowed_frame(
        &self,
        webview: &Entity,
        left_px: f32,
        top_px: f32,
        w_px: f32,
        h_px: f32,
        scale: f32,
    ) {
        use objc2::ClassType;
        use objc2_app_kit::NSView;
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        let s = (scale as f64).max(1.0e-6);
        let w = w_px as f64 / s;
        let h = h_px as f64 / s;
        let x = left_px as f64 / s;
        let glass_view = browser
            .native_liquid_glass
            .as_ref()
            .map(|glass| glass.as_super());
        let parent = glass_view
            .and_then(|glass_view| unsafe { glass_view.superview() })
            .or_else(|| unsafe { view.superview() });
        // winit's content view is flipped (origin top-left, y-down) — match Bevy UI's top_px
        // directly. Only fall back to the AppKit y-up flip if the parent is not flipped.
        let flipped = parent.as_ref().is_some_and(|p| p.isFlipped());
        let y = if flipped {
            top_px as f64 / s
        } else {
            let parent_h = parent
                .as_ref()
                .map(|p| p.bounds().size.height)
                .unwrap_or(0.0);
            (parent_h - top_px as f64 / s - h).max(0.0)
        };
        let frame = (x, y, w, h);
        Self::refresh_windowed_transparency(browser, view);
        if browser.last_frame.get() == Some(frame) {
            return;
        }
        browser.last_frame.set(Some(frame));
        let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
        if let Some(glass_view) = glass_view {
            glass_view.setFrame(rect);
            view.setFrame(glass_view.bounds());
        } else {
            view.setFrame(rect);
        }
        Self::refresh_windowed_transparency(browser, view);
        browser.host.was_resized();
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_frame(&self, _: &Entity, _: f32, _: f32, _: f32, _: f32, _: f32) {}

    #[cfg(target_os = "macos")]
    pub fn set_windowed_corner_radius(
        &self,
        webview: &Entity,
        radius_px: f32,
        scale: f32,
        all_corners: bool,
    ) {
        use objc2::ClassType;
        use objc2_app_kit::NSView;
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        let s = (scale as f64).max(1.0e-6);
        let radius = (radius_px as f64 / s).max(0.0);
        if browser.last_corner_radius.get() == Some(radius)
            && browser.last_corner_radius_all_corners.get() == Some(all_corners)
        {
            return;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        browser.last_corner_radius.set(Some(radius));
        browser
            .last_corner_radius_all_corners
            .set(Some(all_corners));
        if let Some(glass) = &browser.native_liquid_glass {
            Self::apply_view_tree_corner_radius(glass.as_super(), radius, all_corners);
        }
        Self::apply_view_tree_corner_radius(view, radius, all_corners);
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_corner_radius(&self, _: &Entity, _: f32, _: f32, _: bool) {}

    #[cfg(target_os = "macos")]
    pub fn set_windowed_focus_ring(
        &self,
        webview: &Entity,
        width_px: f32,
        scale: f32,
        color_rgb: [f32; 3],
    ) {
        use objc2_app_kit::{NSColor, NSView};
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        let s = (scale as f64).max(1.0e-6);
        let width = (width_px as f64 / s).max(0.0);
        let r = color_rgb[0].clamp(0.0, 1.0) as f64;
        let g = color_rgb[1].clamp(0.0, 1.0) as f64;
        let b = color_rgb[2].clamp(0.0, 1.0) as f64;
        let state = Some((width, r, g, b));
        if browser.last_focus_ring.get() == state {
            return;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        view.setWantsLayer(true);
        let Some(layer) = view.layer() else {
            return;
        };
        browser.last_focus_ring.set(state);
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
        layer.setBorderWidth(width);
        layer.setBorderColor(Some(&color.CGColor()));
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_focus_ring(&self, _: &Entity, _: f32, _: f32, _: [f32; 3]) {}

    #[cfg(target_os = "macos")]
    pub fn set_windowed_hidden(&self, webview: &Entity, hidden: bool) {
        use objc2::ClassType;
        use objc2_app_kit::NSView;
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        if let Some(glass) = &browser.native_liquid_glass {
            glass.as_super().setHidden(hidden);
        }
        view.setHidden(hidden);
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_hidden(&self, _: &Entity, _: bool) {}

    #[cfg(target_os = "macos")]
    pub fn raise_windowed_to_front(&self, webview: &Entity) {
        use objc2::ClassType;
        use objc2_app_kit::{NSView, NSWindowOrderingMode};
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        if !browser.windowed {
            return;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        if let Some(glass) = &browser.native_liquid_glass {
            let glass_view = glass.as_super();
            let Some(parent) = (unsafe { glass_view.superview() }) else {
                return;
            };
            parent.addSubview_positioned_relativeTo(glass_view, NSWindowOrderingMode::Above, None);
        } else {
            let Some(parent) = (unsafe { view.superview() }) else {
                return;
            };
            parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Above, None);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn raise_windowed_to_front(&self, _: &Entity) {}

    /// Set the `zPosition` of a windowed browser's native view (the liquid-glass container when
    /// present). The layout composites as a sibling `CALayer` at `zPosition` 100, and a plain
    /// windowed view sits at 0 — so `raise_windowed_to_front` (subview order) cannot lift the
    /// command-bar modal above the sidebar/header overlay. A higher `zPosition` can.
    #[cfg(target_os = "macos")]
    pub fn set_windowed_z_position(&self, webview: &Entity, z: f64) {
        use objc2::ClassType;
        use objc2_app_kit::NSView;
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        if !browser.windowed {
            return;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        let target = browser
            .native_liquid_glass
            .as_ref()
            .map(|glass| glass.as_super())
            .unwrap_or(view);
        target.setWantsLayer(true);
        if let Some(layer) = target.layer() {
            layer.setZPosition(z);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn set_windowed_z_position(&self, _: &Entity, _: f64) {}

    #[cfg(target_os = "macos")]
    pub fn lower_windowed_to_back(&self, webview: &Entity) {
        use objc2::ClassType;
        use objc2_app_kit::{NSView, NSWindowOrderingMode};
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        if !browser.windowed {
            return;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        if let Some(glass) = &browser.native_liquid_glass {
            let glass_view = glass.as_super();
            let Some(parent) = (unsafe { glass_view.superview() }) else {
                return;
            };
            parent.addSubview_positioned_relativeTo(glass_view, NSWindowOrderingMode::Below, None);
        } else {
            let Some(parent) = (unsafe { view.superview() }) else {
                return;
            };
            parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, None);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn lower_windowed_to_back(&self, _: &Entity) {}

    #[cfg(target_os = "macos")]
    pub fn nudge_windowed_repaint(&self, webview: &Entity) -> bool {
        use objc2::ClassType;
        use objc2_app_kit::NSView;
        let Some(browser) = self.browsers.get(webview) else {
            return false;
        };
        if !browser.windowed {
            return false;
        }
        let handle = browser.host.window_handle();
        if handle.is_null() {
            return false;
        }
        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
        let target = browser
            .native_liquid_glass
            .as_ref()
            .map(|glass| glass.as_super())
            .unwrap_or(view);
        let mut frame = target.frame();
        frame.size.height += 1.0;
        target.setFrame(frame);
        if let Some(glass) = &browser.native_liquid_glass {
            view.setFrame(glass.as_super().bounds());
        }
        Self::refresh_windowed_transparency(browser, view);
        browser.host.was_resized();
        browser.last_frame.set(None);
        true
    }

    #[cfg(not(target_os = "macos"))]
    pub fn nudge_windowed_repaint(&self, _: &Entity) -> bool {
        false
    }

    /// Closes the browser associated with the given webview entity.
    ///
    /// The browser will be removed from the hash map after closing.
    pub fn close(&mut self, webview: &Entity) {
        if let Some(browser) = self.browsers.remove(webview) {
            info!(
                "cef_close_browser webview={webview:?} windowed={}",
                browser.windowed
            );
            #[cfg(target_os = "macos")]
            {
                use objc2::ClassType;
                use objc2_app_kit::NSView;
                if let Some(glass) = &browser.native_liquid_glass {
                    glass.setContentView(None);
                    glass.as_super().removeFromSuperview();
                } else if browser.windowed {
                    let handle = browser.host.window_handle();
                    if !handle.is_null() {
                        let view: &NSView = unsafe { &*handle.cast::<NSView>() };
                        view.removeFromSuperview();
                    }
                }
            }
            browser.host.close_browser(true as _);
            debug!("Closed browser with webview: {:?}", webview);
        }
    }

    #[inline]
    pub fn try_receive_texture(&self) -> core::result::Result<RenderTextureMessage, TryRecvError> {
        self.receiver.try_recv()
    }

    #[inline]
    pub fn try_receive_accelerated(&self) -> core::result::Result<AcceleratedFrame, TryRecvError> {
        self.accel_receiver.try_recv()
    }

    /// Shows the DevTools for the specified webview.
    pub fn show_devtool(&self, webview: &Entity) {
        let Some(browser) = self.browsers.get(webview) else {
            return;
        };
        browser.host.show_dev_tools(
            Some(&WindowInfo::default()),
            Some(&mut ClientHandlerBuilder::new(DevToolRenderHandlerBuilder::build()).build()),
            Some(&BrowserSettings::default()),
            None,
        );
    }

    /// Closes the DevTools for the specified webview.
    pub fn close_devtools(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.host.close_dev_tools();
        }
    }

    /// Navigate backwards.
    ///
    /// ## Reference
    ///
    /// - [`GoBack`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowser.html#a85b02760885c070e4ad2a2705cea56cb)
    pub fn go_back(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            let can = browser.client.can_go_back();
            bevy::log::info!("[nav] go_back {:?} can_go_back={}", webview, can);
            if can == 1 {
                browser.client.go_back();
            }
        } else {
            bevy::log::warn!("[nav] go_back {:?} — browser not found", webview);
        }
    }

    /// Navigate forwards.
    ///
    /// ## Reference
    ///
    /// - [`GoForward`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowser.html#aa8e97fc210ee0e73f16b2d98482419d0)
    pub fn go_forward(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            let can = browser.client.can_go_forward();
            bevy::log::info!("[nav] go_forward {:?} can_go_forward={}", webview, can);
            if can == 1 {
                browser.client.go_forward();
            }
        } else {
            bevy::log::warn!("[nav] go_forward {:?} — browser not found", webview);
        }
    }

    /// Navigate a specific webview to a new URL.
    pub fn execute_js(&self, webview: &Entity, script: &str) {
        if let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
        {
            frame.execute_java_script(Some(&script.into()), None, 0);
        }
    }

    pub fn navigate(&self, webview: &Entity, url: &str) {
        if let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
        {
            frame.load_url(Some(&url.into()));
        }
    }

    /// Reload a specific webview (normal reload, may use cache — browser ⌘R / Ctrl+R).
    pub fn reload_webview(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.client.reload();
        }
    }

    /// Hard-reload a specific webview (bypass HTTP cache — browser ⌘⇧R / Ctrl+Shift+R).
    pub fn reload_webview_ignore_cache(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.client.reload_ignore_cache();
        }
    }

    /// Returns the current zoom level for the specified webview.
    ///
    /// ## Reference
    ///
    /// - [`GetZoomLevel`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a524d4a358287dab284c0dfec6d6d229e)
    pub fn zoom_level(&self, webview: &Entity) -> Option<f64> {
        self.browsers
            .get(webview)
            .map(|browser| browser.host.zoom_level())
    }

    /// Sets the zoom level for the specified webview.
    ///
    /// ## Reference
    ///
    /// - [`SetZoomLevel`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#af2b7bf250ac78345117cd575190f2f7b)
    pub fn set_zoom_level(&self, webview: &Entity, zoom_level: f64) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.host.set_zoom_level(zoom_level);
        }
    }

    /// Sets whether the audio is muted for the specified webview.
    ///
    /// ## Reference
    ///
    /// - [`SetAudioMuted`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a153d179c9ff202c8bb8869d2e9a820a2)
    pub fn set_audio_muted(&self, webview: &Entity, muted: bool) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.host.set_audio_muted(muted as _);
        }
    }

    #[inline]
    pub fn reload(&self) {
        for browser in self.browsers.values() {
            info!("Reloading browser");
            browser.client.reload();
        }
    }

    /// ## Reference
    ///
    /// - [`ImeSetComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn set_ime_composition(&self, text: &str, cursor_utf16: Option<u32>) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            Self::set_ime_composition_on(browser, text, cursor_utf16);
        }
    }

    pub fn set_ime_composition_for(&self, webview: &Entity, text: &str, cursor_utf16: Option<u32>) {
        if let Some(browser) = self.browsers.get(webview) {
            if !browser.windowed {
                browser.host.set_focus(true as _);
            }
            Self::set_ime_composition_on(browser, text, cursor_utf16);
        }
    }

    fn set_ime_composition_on(browser: &WebviewBrowser, text: &str, cursor_utf16: Option<u32>) {
        let underlines = make_underlines_for(text, cursor_utf16.map(|i| (i, i)));
        let i = text.encode_utf16().count();
        let selection_range = Range {
            from: i as _,
            to: i as _,
        };
        let replacement_range = Self::ime_caret_range_for();
        browser.host.ime_set_composition(
            Some(&text.into()),
            Some(&underlines),
            Some(&replacement_range),
            Some(&selection_range),
        );
    }

    /// ## Reference
    ///
    /// [`ImeCancelComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#ac12a8076859d0c1e58e55080f698e7a9)
    pub fn ime_cancel_composition(&self) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            browser.host.ime_cancel_composition();
        }
    }

    pub fn ime_cancel_composition_for(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            if !browser.windowed {
                browser.host.set_focus(true as _);
            }
            browser.host.ime_cancel_composition();
        }
    }

    /// ## Reference
    ///
    /// [`ImeSetComposition`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowserHost.html#a567b41fb2d3917843ece3b57adc21ebe)
    pub fn ime_finish_composition(&self, keep_selection: bool) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            browser.host.ime_finish_composing_text(keep_selection as _);
        }
    }

    pub fn ime_finish_composition_for(&self, webview: &Entity, keep_selection: bool) {
        if let Some(browser) = self.browsers.get(webview) {
            if !browser.windowed {
                browser.host.set_focus(true as _);
            }
            browser.host.ime_finish_composing_text(keep_selection as _);
        }
    }

    pub fn set_ime_commit_text(&self, text: &str) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            Self::set_ime_commit_text_on(browser, text);
        }
    }

    pub fn set_ime_commit_text_for(&self, webview: &Entity, text: &str) {
        if let Some(browser) = self.browsers.get(webview) {
            if !browser.windowed {
                browser.host.set_focus(true as _);
            }
            Self::set_ime_commit_text_on(browser, text);
        }
    }

    fn set_ime_commit_text_on(browser: &WebviewBrowser, text: &str) {
        let replacement_range = Self::ime_caret_range_for();
        browser
            .host
            .ime_commit_text(Some(&text.into()), Some(&replacement_range), 0);
    }

    fn persistent_request_context_settings(cache_path: &str) -> RequestContextSettings {
        let mut settings = RequestContextSettings::default();
        settings.cache_path = cache_path.into();
        settings.persist_session_cookies = 1;
        settings
    }

    fn ensure_shared_disk_context(&mut self, requester: Requester, root: &str) {
        if self.shared_disk_context.is_some() {
            return;
        }
        let mut context = cef::request_context_create_context(
            Some(&Self::persistent_request_context_settings(root)),
            Some(&mut RequestContextHandlerBuilder::build()),
        );
        if let Some(context) = context.as_mut() {
            let emb_scheme = resolved_cef_embedded_page_config().scheme.clone();
            context.register_scheme_handler_factory(
                Some(&SCHEME_CEF.into()),
                Some(&HOST_CEF.into()),
                Some(&mut LocalSchemaHandlerBuilder::build(requester.clone())),
            );
            context.register_scheme_handler_factory(
                Some(&emb_scheme.as_str().into()),
                None,
                Some(&mut LocalSchemaHandlerBuilder::build(requester.clone())),
            );
            context.register_scheme_handler_factory(
                Some(&crate::util::FILES_SCHEME.into()),
                None,
                Some(&mut LocalSchemaHandlerBuilder::build(requester)),
            );
        }
        self.shared_disk_context = context;
    }

    fn ephemeral_request_context(requester: Requester) -> Option<RequestContext> {
        let mut context = cef::request_context_create_context(
            Some(&RequestContextSettings::default()),
            Some(&mut RequestContextHandlerBuilder::build()),
        );
        if let Some(context) = context.as_mut() {
            let emb_scheme = resolved_cef_embedded_page_config().scheme.clone();
            context.register_scheme_handler_factory(
                Some(&SCHEME_CEF.into()),
                Some(&HOST_CEF.into()),
                Some(&mut LocalSchemaHandlerBuilder::build(requester.clone())),
            );
            context.register_scheme_handler_factory(
                Some(&emb_scheme.as_str().into()),
                None,
                Some(&mut LocalSchemaHandlerBuilder::build(requester.clone())),
            );
            context.register_scheme_handler_factory(
                Some(&crate::util::FILES_SCHEME.into()),
                None,
                Some(&mut LocalSchemaHandlerBuilder::build(requester)),
            );
        }
        context
    }

    fn client_handler(
        &self,
        webview: Entity,
        size: SharedViewSize,
        device_scale: SharedDeviceScaleFactor,
        ipc_event_sender: Sender<IpcEventRaw>,
        bin_ipc_event_sender: Sender<BinIpcEventRaw>,
        brp_sender: Sender<BrpMessage>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        webview_loading_state_sender: WebviewLoadingStateSenderInner,
        webview_committed_nav_sender: WebviewCommittedNavigationSenderInner,
        webview_cef_state_sender: WebviewCefStateSenderInner,
        webview_popup_sender: WebviewPopupSenderInner,
        texture_wake: Option<TextureWake>,
        cancel_native_focus: bool,
    ) -> Client {
        let client = ClientHandlerBuilder::new(RenderHandlerBuilder::build(
            webview,
            self.sender.clone(),
            self.accel_sender.clone(),
            texture_wake.clone(),
            size.clone(),
            device_scale.clone(),
        ))
        .with_wake(texture_wake);
        let client = if cancel_native_focus {
            client.with_focus_handler(FocusCanceler::build())
        } else {
            client
        };
        client
            .with_display_handler(DisplayHandlerBuilder::build(
                webview,
                system_cursor_icon_sender,
                webview_cef_state_sender,
            ))
            .with_load_handler(WebviewLoadHandlerBuilder::build(
                webview,
                webview_loading_state_sender,
                webview_committed_nav_sender,
            ))
            .with_life_span_handler(LifeSpanHandlerBuilder::build(
                webview,
                webview_popup_sender.clone(),
            ))
            .with_request_handler(RequestHandlerBuilder::build(webview, webview_popup_sender))
            .with_message_handler(JsEmitEventHandler::new(webview, ipc_event_sender))
            .with_message_handler(BinEmitEventHandler::new(webview, bin_ipc_event_sender))
            .with_message_handler(BrpHandler::new(brp_sender))
            .build()
    }

    #[inline]
    fn ime_caret_range_for() -> Range {
        // Use sentinel replacement range to indicate caret position
        Range {
            from: u32::MAX,
            to: u32::MAX,
        }
    }

    fn create_extra_info(scripts: &[String]) -> Option<DictionaryValue> {
        if scripts.is_empty() {
            return None;
        }
        let extra = dictionary_value_create()?;
        extra.set_string(
            Some(&CefString::from(INIT_SCRIPT_KEY)),
            Some(&CefString::from(scripts.join(";").as_str())),
        );
        Some(extra)
    }
}

#[allow(clippy::unnecessary_cast)]
pub fn modifiers_from_mouse_buttons<'a>(buttons: impl IntoIterator<Item = &'a MouseButton>) -> u32 {
    let mut modifiers = cef_event_flags_t::EVENTFLAG_NONE.0 as u32;
    for button in buttons {
        match button {
            MouseButton::Left => {
                modifiers |= cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON.0 as u32
            }
            MouseButton::Right => {
                modifiers |= cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON.0 as u32
            }
            MouseButton::Middle => {
                modifiers |= cef_event_flags_t::EVENTFLAG_MIDDLE_MOUSE_BUTTON.0 as u32
            }
            _ => {}
        }
    }
    modifiers
}

pub fn make_underlines_for(
    text: &str,
    selection_utf16: Option<(u32, u32)>,
) -> Vec<CompositionUnderline> {
    let len16 = utf16_len(text);

    let base = CompositionUnderline {
        size: size_of::<CompositionUnderline>(),
        range: Range { from: 0, to: len16 },
        color: 0,
        background_color: 0,
        thick: 0,
        style: Default::default(),
    };

    if let Some((from, to)) = selection_utf16
        && from < to
    {
        let sel = CompositionUnderline {
            size: size_of::<CompositionUnderline>(),
            range: Range { from, to },
            color: 0,
            background_color: 0,
            thick: 1,
            style: Default::default(),
        };
        return vec![base, sel];
    }
    vec![base]
}

#[inline]
fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

#[allow(dead_code)]
fn utf16_index_from_byte(s: &str, byte_idx: usize) -> u32 {
    s[..byte_idx].encode_utf16().count() as u32
}

pub fn windowless_frame_rate_from_refresh_millihertz(refresh_rate_millihertz: Option<u32>) -> i32 {
    refresh_rate_millihertz
        .map(|millihertz| ((millihertz.saturating_add(999)) / 1000).max(1) as i32)
        .unwrap_or(DEFAULT_WINDOWLESS_FRAME_RATE)
}

pub fn windowless_frame_interval_from_refresh_millihertz(
    refresh_rate_millihertz: Option<u32>,
) -> Duration {
    windowless_frame_interval_from_frame_rate(windowless_frame_rate_from_refresh_millihertz(
        refresh_rate_millihertz,
    ))
}

pub fn windowless_frame_interval_from_frame_rate(frame_rate: i32) -> Duration {
    Duration::from_nanos(
        (1_000_000_000 / normalize_windowless_frame_rate(frame_rate) as u64).max(1),
    )
}

/// Scale a webview's windowless frame rate by host-window focus/visibility so unfocused or
/// hidden windows stop driving the CEF paint → texture-upload → Bevy render loop at full rate.
pub fn effective_windowless_frame_rate(monitor_rate: i32, visible: bool, focused: bool) -> i32 {
    let monitor_rate = normalize_windowless_frame_rate(monitor_rate);
    if !visible {
        HIDDEN_WINDOWLESS_FRAME_RATE
    } else if !focused {
        BACKGROUND_WINDOWLESS_FRAME_RATE.min(monitor_rate)
    } else {
        monitor_rate
    }
}

fn normalize_windowless_frame_rate(frame_rate: i32) -> i32 {
    frame_rate.max(1)
}

#[cfg(test)]
mod tests {
    use super::{
        BACKGROUND_WINDOWLESS_FRAME_RATE, DEFAULT_WINDOWLESS_FRAME_RATE,
        HIDDEN_WINDOWLESS_FRAME_RATE, effective_windowless_frame_rate,
        windowless_frame_interval_from_refresh_millihertz,
        windowless_frame_rate_from_refresh_millihertz,
    };
    use crate::prelude::modifiers_from_mouse_buttons;
    use bevy::prelude::*;
    use std::time::Duration;

    #[test]
    fn focused_window_keeps_monitor_frame_rate() {
        assert_eq!(effective_windowless_frame_rate(120, true, true), 120);
        assert_eq!(effective_windowless_frame_rate(60, true, true), 60);
    }

    #[test]
    fn visible_unfocused_window_is_throttled_but_never_above_monitor() {
        assert_eq!(
            effective_windowless_frame_rate(120, true, false),
            BACKGROUND_WINDOWLESS_FRAME_RATE
        );
        // A slow monitor caps the background rate too.
        assert_eq!(effective_windowless_frame_rate(24, true, false), 24);
    }

    #[test]
    fn hidden_window_drops_to_minimum_frame_rate() {
        assert_eq!(
            effective_windowless_frame_rate(120, false, true),
            HIDDEN_WINDOWLESS_FRAME_RATE
        );
        assert_eq!(
            effective_windowless_frame_rate(120, false, false),
            HIDDEN_WINDOWLESS_FRAME_RATE
        );
    }

    #[test]
    #[allow(clippy::unnecessary_cast)]
    fn test_modifiers_from_mouse_buttons() {
        let buttons = vec![&MouseButton::Left, &MouseButton::Right];
        let modifiers = modifiers_from_mouse_buttons(buttons);
        assert_eq!(
            modifiers,
            cef_dll_sys::cef_event_flags_t::EVENTFLAG_LEFT_MOUSE_BUTTON.0 as u32
                | cef_dll_sys::cef_event_flags_t::EVENTFLAG_RIGHT_MOUSE_BUTTON.0 as u32
        );
    }

    #[test]
    fn osr_frame_rate_uses_display_refresh_millihertz() {
        assert_eq!(
            windowless_frame_rate_from_refresh_millihertz(Some(60_000)),
            60
        );
        assert_eq!(
            windowless_frame_rate_from_refresh_millihertz(Some(119_880)),
            DEFAULT_WINDOWLESS_FRAME_RATE
        );
        assert_eq!(
            windowless_frame_rate_from_refresh_millihertz(Some(144_000)),
            144
        );
    }

    #[test]
    fn osr_frame_rate_falls_back_to_120hz_when_refresh_unknown() {
        assert_eq!(
            windowless_frame_rate_from_refresh_millihertz(None),
            DEFAULT_WINDOWLESS_FRAME_RATE
        );
    }

    #[test]
    fn osr_frame_rate_never_sets_cef_below_minimum() {
        assert_eq!(windowless_frame_rate_from_refresh_millihertz(Some(0)), 1);
    }

    #[test]
    fn osr_frame_interval_uses_display_refresh_millihertz() {
        assert_eq!(
            windowless_frame_interval_from_refresh_millihertz(Some(60_000)),
            Duration::from_nanos(16_666_666)
        );
        assert!(
            windowless_frame_interval_from_refresh_millihertz(Some(144_000))
                < Duration::from_millis(8)
        );
    }

    #[test]
    fn osr_frame_rate_is_passed_to_cef_settings() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        assert!(
            implementation.contains("let windowless_frame_rate = normalize_windowless_frame_rate")
        );
        assert!(implementation.contains("windowless_frame_rate,"));
    }

    #[test]
    fn osr_uses_cef_internal_begin_frame_scheduler() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("external_begin_frame_enabled: false as _"));
        assert!(!implementation.contains("send_external_begin_frame"));
    }

    #[test]
    fn existing_osr_browsers_can_update_frame_rate() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        assert!(implementation.contains("pub fn set_windowless_frame_rate"));
        assert!(implementation.contains("host.set_windowless_frame_rate(frame_rate)"));
    }

    #[test]
    fn hidden_osr_webviews_are_suspended_with_cef_visibility() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        assert!(implementation.contains("hidden: Cell<bool>"));
        assert!(implementation.contains("browser.host.was_hidden(1)"));
    }

    #[test]
    fn native_liquid_glass_embeds_cef_view_as_content() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let create_fn = implementation
            .split("fn create_native_liquid_glass")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(target_os = \"macos\")]\n    pub fn set_windowed_frame")
                    .next()
            })
            .unwrap_or_default();

        assert!(create_fn.contains("NSGlassEffectViewStyle::Clear"));
        assert!(!create_fn.contains("NSGlassEffectViewStyle::Regular"));
        assert!(create_fn.contains("glass.setContentView(Some(view))"));
        assert!(create_fn.contains("view.setFrame(glass_view.bounds())"));
    }

    #[test]
    fn native_liquid_glass_uses_clear_tint() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let create_fn = implementation
            .split("fn create_native_liquid_glass")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(target_os = \"macos\")]\n    pub fn set_windowed_frame")
                    .next()
            })
            .unwrap_or_default();

        assert!(create_fn.contains("glass.setTintColor(Some(&NSColor::clearColor()))"));
    }

    #[test]
    fn native_liquid_glass_makes_cef_view_layer_transparent() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let create_fn = implementation
            .split("fn create_native_liquid_glass")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(target_os = \"macos\")]\n    pub fn set_windowed_frame")
                    .next()
            })
            .unwrap_or_default();

        assert!(create_fn.contains("view.setWantsLayer(true)"));
        assert!(create_fn.contains("layer.setOpaque(false)"));
        assert!(create_fn.contains("layer.setBackgroundColor(Some(&clear_color.CGColor()))"));
        assert!(create_fn.contains("Self::make_view_tree_transparent"));
    }

    #[test]
    fn native_liquid_glass_recursively_clears_cef_subviews() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("fn make_view_tree_transparent"));
        assert!(implementation.contains("fn make_layer_tree_transparent"));
        assert!(implementation.contains("for i in 0..subviews.count()"));
        assert!(implementation.contains("let child = subviews.objectAtIndex(i)"));
        assert!(implementation.contains("Self::make_view_tree_transparent(&child, clear_color)"));
        assert!(implementation.contains("for i in 0..sublayers.count()"));
        assert!(implementation.contains("let child = sublayers.objectAtIndex(i)"));
        assert!(implementation.contains("Self::make_layer_tree_transparent(&child, clear_color)"));
    }

    #[test]
    fn windowed_frame_refreshes_transparency_before_same_frame_return() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let set_frame_fn = implementation
            .split("#[cfg(target_os = \"macos\")]\n    pub fn set_windowed_frame")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(not(target_os = \"macos\"))]\n    pub fn set_windowed_frame")
                    .next()
            })
            .unwrap_or_default();
        let refresh_idx = set_frame_fn
            .find("Self::refresh_windowed_transparency")
            .expect("windowed transparency refresh");
        let cache_idx = set_frame_fn
            .find("browser.last_frame.get() == Some(frame)")
            .expect("same frame cache check");

        assert!(refresh_idx < cache_idx);
    }

    #[test]
    fn windowed_repaint_nudge_refreshes_transparency() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let nudge_fn = implementation
            .split("#[cfg(target_os = \"macos\")]\n    pub fn nudge_windowed_repaint")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(not(target_os = \"macos\"))]\n    pub fn nudge_windowed_repaint")
                    .next()
            })
            .unwrap_or_default();

        assert!(nudge_fn.contains("Self::refresh_windowed_transparency"));
    }

    #[test]
    fn windowed_native_views_apply_corner_radius() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("last_corner_radius: Cell<Option<f64>>"));
        assert!(implementation.contains("fn apply_view_tree_corner_radius"));
        assert!(implementation.contains("layer.setCornerRadius(radius)"));
        assert!(implementation.contains("layer.setMasksToBounds(true)"));
        assert!(implementation.contains("pub fn set_windowed_corner_radius"));
    }

    #[test]
    fn windowed_native_views_clip_to_frame_with_zero_radius() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("layer.setMasksToBounds(true)"));
        assert!(!implementation.contains("layer.setMasksToBounds(radius > 0.0)"));
    }

    #[test]
    fn windowed_corner_cover_uses_even_odd_overlay() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        assert!(implementation.contains("fn update_corner_cover"));
        assert!(implementation.contains("pub fn set_windowed_corner_cover"));
        let cover_fn = implementation
            .split("fn update_corner_cover")
            .nth(1)
            .and_then(|tail| tail.split("pub fn set_windowed_corner_cover").next())
            .unwrap_or_default();
        assert!(cover_fn.contains("kCAFillRuleEvenOdd"));
        assert!(cover_fn.contains("add_rounded_rect"));
        assert!(cover_fn.contains("setZPosition"));
    }

    #[test]
    fn windowed_native_views_support_focus_ring_border() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("last_focus_ring"));
        assert!(implementation.contains("pub fn set_windowed_focus_ring"));
        assert!(implementation.contains("layer.setBorderWidth(width)"));
        assert!(implementation.contains("layer.setBorderColor"));
    }

    #[test]
    fn windowed_native_focus_can_bypass_focus_canceler() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();

        assert!(implementation.contains("allow_native_focus"));
        assert!(implementation.contains("if cancel_native_focus"));
        assert!(implementation.contains(".with_focus_handler(FocusCanceler::build())"));
        assert!(implementation.contains("pub fn set_windowed_focus"));
    }

    #[test]
    fn osr_focus_sync_does_not_clear_windowed_focus() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let sync_fn = implementation
            .split("pub fn sync_osr_focus_to_active_pane")
            .nth(1)
            .and_then(|tail| tail.split("pub fn send_mouse_move").next())
            .unwrap_or_default();

        assert!(sync_fn.contains("if !browser.windowed"));
        assert!(!sync_fn.contains(
            "for (_entity, browser) in &self.browsers {\n            browser.host.set_focus(false"
        ));
    }

    #[test]
    fn closing_non_glass_windowed_browser_removes_native_view() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let close_fn = implementation
            .split("pub fn close")
            .nth(1)
            .and_then(|tail| tail.split("pub fn try_receive_texture").next())
            .unwrap_or_default();

        assert!(close_fn.contains("window_handle"));
        assert!(close_fn.contains("removeFromSuperview"));
    }

    #[test]
    fn windowed_native_bottom_corner_mask_is_not_flipped_to_top() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        let apply_fn = implementation
            .split("fn apply_view_tree_corner_radius")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[cfg(target_os = \"macos\")]\n    pub fn set_windowed_frame")
                    .next()
            })
            .unwrap_or_default();

        assert!(apply_fn.contains("LayerMinXMinYCorner | CACornerMask::LayerMaxXMinYCorner"));
        assert!(!apply_fn.contains("isGeometryFlipped"));
    }

    #[test]
    fn all_osr_webviews_can_be_suspended_together() {
        let implementation = include_str!("browsers.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        assert!(implementation.contains("pub fn set_all_osr_hidden"));
        assert!(implementation.contains("browser.host.set_focus(false"));
    }
}
