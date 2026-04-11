use crate::browser_process::BrpHandler;
use crate::browser_process::ClientHandlerBuilder;
use crate::browser_process::client_handler::{IpcEventRaw, JsEmitEventHandler};
use crate::prelude::*;
use async_channel::{Sender, TryRecvError};
use bevy::input::ButtonState;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_remote::BrpMessage;
use cef::{
    Browser, BrowserHost, BrowserSettings, CefString, Client, CompositionUnderline,
    DictionaryValue, ImplBrowser, ImplBrowserHost, ImplDictionaryValue, ImplFrame, ImplListValue,
    ImplProcessMessage, ImplRequestContext, MouseButtonType, ProcessId, Range, RequestContext,
    RequestContextSettings, WindowInfo, browser_host_create_browser_sync, dictionary_value_create,
    process_message_create, register_scheme_handler_factory,
};
use cef_dll_sys::{cef_event_flags_t, cef_mouse_button_type_t};
#[allow(deprecated)]
use raw_window_handle::RawWindowHandle;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Once;

mod devtool_render_handler;
mod keyboard;

use crate::browser_process::browsers::devtool_render_handler::DevToolRenderHandlerBuilder;
use crate::browser_process::display_handler::{DisplayHandlerBuilder, SystemCursorIconSenderInner};
use crate::browser_process::load_handler::{
    WebviewLoadHandlerBuilder, WebviewLoadingStateSenderInner,
};
use crate::browser_process::renderer_handler::SharedDeviceScaleFactor;
pub use keyboard::*;

/// CEF [`BrowserSettings::background_color`] is ARGB (`A` in the high byte). Matches the dark
/// gray used by bevy_cef’s webview placeholder texture (sRGB 43, 44, 47) so OSR clears are not white.
const CEF_OSR_BACKGROUND_COLOR_ARGB: u32 = 0xFF2B2C2F;

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
}

pub struct Browsers {
    browsers: HashMap<Entity, WebviewBrowser>,
    sender: TextureSender,
    receiver: TextureReceiver,
    /// Lazily created when [`Self::create_browser`] is called with a non-empty disk profile root.
    /// Shared by all webviews so multiple panes use one cookie store and avoid conflicting contexts on the same path.
    shared_disk_context: Option<RequestContext>,
}

impl Default for Browsers {
    fn default() -> Self {
        let (sender, receiver) = async_channel::unbounded::<RenderTextureMessage>();
        Browsers {
            browsers: HashMap::default(),
            sender,
            receiver,
            shared_disk_context: None,
        }
    }
}

impl Browsers {
    #[allow(clippy::too_many_arguments)]
    pub fn create_browser(
        &mut self,
        webview: Entity,
        uri: &str,
        webview_size: Vec2,
        device_scale_factor: f32,
        requester: Requester,
        ipc_event_sender: Sender<IpcEventRaw>,
        brp_sender: Sender<BrpMessage>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        webview_loading_state_sender: WebviewLoadingStateSenderInner,
        initialize_scripts: &[String],
        _window_handle: Option<RawWindowHandle>,
        disk_profile_root: Option<&str>,
    ) {
        let size = Rc::new(Cell::new(webview_size));
        let device_scale = Rc::new(Cell::new(device_scale_factor));
        // Build the client before borrowing `shared_disk_context` mutably (same `self`).
        let mut client = self.client_handler(
            webview,
            size.clone(),
            device_scale.clone(),
            ipc_event_sender,
            brp_sender,
            system_cursor_icon_sender,
            webview_loading_state_sender,
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
            assert_eq!(
                ok_cef, 1,
                "cef_register_scheme_handler_factory(cef) failed with code {ok_cef}"
            );
            let mut embedded_factory = LocalSchemaHandlerBuilder::build(requester_for_global);
            let ok_embedded = register_scheme_handler_factory(
                Some(&emb_scheme.as_str().into()),
                None,
                Some(&mut embedded_factory),
            );
            assert_eq!(
                ok_embedded, 1,
                "cef_register_scheme_handler_factory(embedded page scheme) failed with code {ok_embedded}"
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

        let browser = browser_host_create_browser_sync(
            Some(&WindowInfo {
                windowless_rendering_enabled: true as _,
                external_begin_frame_enabled: true as _,
                #[cfg(target_os = "macos")]
                parent_view: match _window_handle {
                    Some(RawWindowHandle::AppKit(handle)) => handle.ns_view.as_ptr(),
                    _ => std::ptr::null_mut(),
                },
                #[cfg(target_os = "windows")]
                parent_window: match _window_handle {
                    Some(RawWindowHandle::Win32(handle)) => {
                        cef_dll_sys::HWND(handle.hwnd.get() as _)
                    }
                    _ => cef_dll_sys::HWND(std::ptr::null_mut()),
                },
                // shared_texture_enabled: true as _,
                ..Default::default()
            }),
            Some(&mut client),
            Some(&uri.into()),
            Some(&BrowserSettings {
                // Cap for OSR; matches ProMotion / 120 Hz displays when the host can sustain it.
                windowless_frame_rate: 120,
                background_color: CEF_OSR_BACKGROUND_COLOR_ARGB,
                ..Default::default()
            }),
            Self::create_extra_info(initialize_scripts).as_mut(),
            context_for_browser,
        )
        .expect("Failed to create browser");
        let host = browser.host().expect("Failed to get browser host");
        let webview_browser = WebviewBrowser {
            host,
            client: browser,
            size,
            device_scale,
        };

        self.browsers.insert(webview, webview_browser);
    }

    /// Returns `true` if [`Self::create_browser`] has already succeeded for this webview entity.
    #[inline]
    pub fn has_browser(&self, webview: Entity) -> bool {
        self.browsers.contains_key(&webview)
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
            b.host.was_hidden(0);
        }
    }

    pub fn send_external_begin_frame(&mut self) {
        for browser in self.browsers.values_mut() {
            browser.host.send_external_begin_frame();
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
        for (_entity, browser) in &self.browsers {
            browser.host.set_focus(false as _);
        }
        if let Some(a) = active
            && let Some(browser) = self.browsers.get(&a)
        {
            browser.host.set_focus(true as _);
        }
        for &h in auxiliary_osr_focus {
            if let Some(browser) = self.browsers.get(&h) {
                browser.host.set_focus(true as _);
            }
        }
        if let Some(a) = active
            && let Some(browser) = self.browsers.get(&a)
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
            browser.host.set_focus(true as _);
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
        {
            argument_list.set_string(0, Some(&id.into().as_str().into()));
            argument_list.set_string(1, Some(&json_body.into()));
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

    /// Closes the browser associated with the given webview entity.
    ///
    /// The browser will be removed from the hash map after closing.
    pub fn close(&mut self, webview: &Entity) {
        if let Some(browser) = self.browsers.remove(webview) {
            browser.host.close_browser(true as _);
            debug!("Closed browser with webview: {:?}", webview);
        }
    }

    #[inline]
    pub fn try_receive_texture(&self) -> core::result::Result<RenderTextureMessage, TryRecvError> {
        self.receiver.try_recv()
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
        if let Some(browser) = self.browsers.get(webview)
            && browser.client.can_go_back() == 1
        {
            browser.client.go_back();
        }
    }

    /// Navigate forwards.
    ///
    /// ## Reference
    ///
    /// - [`GoForward`](https://cef-builds.spotifycdn.com/docs/122.0/classCefBrowser.html#aa8e97fc210ee0e73f16b2d98482419d0)
    pub fn go_forward(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview)
            && browser.client.can_go_forward() == 1
        {
            browser.client.go_forward();
        }
    }

    /// Navigate a specific webview to a new URL.
    pub fn navigate(&self, webview: &Entity, url: &str) {
        if let Some(browser) = self.browsers.get(webview)
            && let Some(frame) = browser.client.main_frame()
        {
            frame.load_url(Some(&url.into()));
        }
    }

    /// Reload a specific webview (normal reload, may use cache — Chrome ⌘R / Ctrl+R).
    pub fn reload_webview(&self, webview: &Entity) {
        if let Some(browser) = self.browsers.get(webview) {
            browser.client.reload();
        }
    }

    /// Hard-reload a specific webview (bypass HTTP cache — Chrome ⌘⇧R / Ctrl+Shift+R).
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
        let underlines = make_underlines_for(text, cursor_utf16.map(|i| (i, i)));
        let i = text.encode_utf16().count();
        let selection_range = Range {
            from: i as _,
            to: i as _,
        };
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            let replacement_range = Self::ime_caret_range_for();
            browser.host.ime_set_composition(
                Some(&text.into()),
                Some(&underlines),
                Some(&replacement_range),
                Some(&selection_range),
            );
        }
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

    pub fn set_ime_commit_text(&self, text: &str) {
        for browser in self
            .browsers
            .values()
            .filter(|b| b.client.focused_frame().is_some())
        {
            let replacement_range = Self::ime_caret_range_for();
            browser
                .host
                .ime_commit_text(Some(&text.into()), Some(&replacement_range), 0);
        }
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
        brp_sender: Sender<BrpMessage>,
        system_cursor_icon_sender: SystemCursorIconSenderInner,
        webview_loading_state_sender: WebviewLoadingStateSenderInner,
    ) -> Client {
        ClientHandlerBuilder::new(RenderHandlerBuilder::build(
            webview,
            self.sender.clone(),
            size.clone(),
            device_scale.clone(),
        ))
        .with_display_handler(DisplayHandlerBuilder::build(system_cursor_icon_sender))
        .with_load_handler(WebviewLoadHandlerBuilder::build(
            webview,
            webview_loading_state_sender,
        ))
        .with_message_handler(JsEmitEventHandler::new(webview, ipc_event_sender))
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

#[cfg(test)]
mod tests {
    use crate::prelude::modifiers_from_mouse_buttons;
    use bevy::prelude::*;

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
}
