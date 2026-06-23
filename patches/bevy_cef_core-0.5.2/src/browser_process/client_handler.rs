mod bin_emit_event_handler;
mod brp_handler;
mod js_emit_event_handler;

use crate::browser_process::ContextMenuHandlerBuilder;
use crate::browser_process::renderer_handler::TextureWake;
use crate::prelude::IntoString;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, Client, ContextMenuHandler, DisplayHandler, FocusHandler, FocusSource, Frame,
    ImplClient, ImplFocusHandler, ImplFrame, ImplProcessMessage, LifeSpanHandler, ListValue,
    LoadHandler, ProcessId, ProcessMessage, RenderHandler, RequestHandler, WrapClient,
    WrapFocusHandler, sys,
};
use std::os::raw::c_int;

pub use bin_emit_event_handler::{BinEmitEventHandler, BinIpcEventRaw};
pub use brp_handler::BrpHandler;
pub use js_emit_event_handler::{IpcEventRaw, JsEmitEventHandler};

// Cancels CEF taking keyboard focus, so winit keeps the macOS first responder and Bevy owns
// keyboard. Without this, a native (windowed) browser's NSView becomes first responder and steals
// every key from Bevy — all non-menu shortcuts die. Typing still reaches the page via the
// `CefKeyboardTarget` forwarding path (keyboard.rs), exactly as in OSR mode.
pub struct FocusCanceler {
    object: *mut RcImpl<sys::cef_focus_handler_t, Self>,
}

impl FocusCanceler {
    pub fn build() -> FocusHandler {
        FocusHandler::new(Self {
            object: core::ptr::null_mut(),
        })
    }
}

impl Rc for FocusCanceler {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for FocusCanceler {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self { object }
    }
}

impl WrapFocusHandler for FocusCanceler {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::cef_focus_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplFocusHandler for FocusCanceler {
    fn on_set_focus(&self, _browser: Option<&mut Browser>, _source: FocusSource) -> c_int {
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::cef_focus_handler_t {
        self.object.cast()
    }
}

pub trait ProcessMessageHandler {
    fn process_name(&self) -> &'static str;

    fn handle_message(&self, browser: &mut Browser, frame: &mut Frame, args: Option<ListValue>);
}

/// ## Reference
///
/// - [`CefBrowser Class Reference`](https://cef-builds.spotifycdn.com/docs/106.1/classCefBrowser.html)
pub struct ClientHandlerBuilder {
    object: *mut RcImpl<sys::cef_client_t, Self>,
    render_handler: RenderHandler,
    context_menu_handler: ContextMenuHandler,
    message_handlers: Vec<std::rc::Rc<dyn ProcessMessageHandler>>,
    display_handler: Option<DisplayHandler>,
    load_handler: Option<LoadHandler>,
    life_span_handler: Option<LifeSpanHandler>,
    request_handler: Option<RequestHandler>,
    focus_handler: Option<FocusHandler>,
    /// Wakes the Bevy/winit loop after an IPC message is handled. On macOS the CEF pump is decoupled
    /// from the Bevy tick, so a command from a native webview (e.g. tab switch) would otherwise sit
    /// in its channel until the next idle tick. Throttled to frame-rate (it's the texture wake).
    wake: Option<TextureWake>,
}

impl ClientHandlerBuilder {
    pub fn new(render_handler: RenderHandler) -> Self {
        Self {
            object: std::ptr::null_mut(),
            render_handler,
            context_menu_handler: ContextMenuHandlerBuilder::build(),
            message_handlers: Vec::new(),
            display_handler: None,
            load_handler: None,
            life_span_handler: None,
            request_handler: None,
            focus_handler: None,
            wake: None,
        }
    }

    pub fn with_wake(mut self, wake: Option<TextureWake>) -> Self {
        self.wake = wake;
        self
    }

    pub fn with_focus_handler(mut self, focus_handler: FocusHandler) -> Self {
        self.focus_handler = Some(focus_handler);
        self
    }

    pub fn with_display_handler(mut self, display_handler: DisplayHandler) -> Self {
        self.display_handler = Some(display_handler);
        self
    }

    pub fn with_load_handler(mut self, load_handler: LoadHandler) -> Self {
        self.load_handler = Some(load_handler);
        self
    }

    pub fn with_life_span_handler(mut self, life_span_handler: LifeSpanHandler) -> Self {
        self.life_span_handler = Some(life_span_handler);
        self
    }

    pub fn with_request_handler(mut self, request_handler: RequestHandler) -> Self {
        self.request_handler = Some(request_handler);
        self
    }

    pub fn with_message_handler(mut self, handler: impl ProcessMessageHandler + 'static) -> Self {
        self.message_handlers.push(std::rc::Rc::new(handler));
        self
    }

    pub fn build(self) -> Client {
        Client::new(self)
    }
}

impl Rc for ClientHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapClient for ClientHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::cef_client_t, Self>) {
        self.object = object;
    }
}

impl Clone for ClientHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            render_handler: self.render_handler.clone(),
            context_menu_handler: self.context_menu_handler.clone(),
            message_handlers: self.message_handlers.clone(),
            display_handler: self.display_handler.clone(),
            load_handler: self.load_handler.clone(),
            life_span_handler: self.life_span_handler.clone(),
            request_handler: self.request_handler.clone(),
            focus_handler: self.focus_handler.clone(),
            wake: self.wake.clone(),
        }
    }
}

impl ImplClient for ClientHandlerBuilder {
    fn render_handler(&self) -> Option<RenderHandler> {
        Some(self.render_handler.clone())
    }

    fn display_handler(&self) -> Option<DisplayHandler> {
        self.display_handler.clone()
    }

    fn load_handler(&self) -> Option<LoadHandler> {
        self.load_handler.clone()
    }

    fn life_span_handler(&self) -> Option<LifeSpanHandler> {
        self.life_span_handler.clone()
    }

    fn request_handler(&self) -> Option<RequestHandler> {
        self.request_handler.clone()
    }

    fn focus_handler(&self) -> Option<FocusHandler> {
        self.focus_handler.clone()
    }

    fn on_process_message_received(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        _: ProcessId,
        message: Option<&mut ProcessMessage>,
    ) -> c_int {
        if let Some(message) = message
            && let Some(browser) = browser
            && let Some(frame) = frame
        {
            let name = message.name().into_string();
            let url = frame.url().into_string();
            if !crate::util::ipc_allowed_browser(&url) {
                crate::util::webview_debug_log(format!(
                    "ipc: dropped inbound '{name}' from untrusted url={url}"
                ));
                return 1;
            }
            if name == crate::prelude::PROCESS_MESSAGE_BRP
                && crate::util::embedded_page_host_of(&url).as_deref() != Some("debug")
            {
                crate::util::webview_debug_log(format!(
                    "ipc: dropped BRP from non-debug url={url}"
                ));
                return 1;
            }
            if let Some(handler) = self
                .message_handlers
                .iter()
                .find(|h| h.process_name() == name.as_str())
            {
                let args = message.argument_list();
                handler.handle_message(browser, frame, args);
                // Wake Bevy so it drains the IPC channel this frame instead of on the next idle tick.
                if let Some(wake) = &self.wake {
                    wake();
                }
            }
        };
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_client_t {
        self.object.cast()
    }
}
