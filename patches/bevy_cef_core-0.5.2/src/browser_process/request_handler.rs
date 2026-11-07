use crate::browser_process::life_span_handler::{WebviewPopupEvent, WebviewPopupSenderInner};
use crate::browser_process::renderer_handler::TextureWake;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, CefString, Frame, ImplRequestHandler, RequestHandler, TerminationStatus,
    WindowOpenDisposition, WrapRequestHandler, sys,
};
use std::os::raw::c_int;

#[derive(Clone, Debug)]
pub struct WebviewRendererTerminatedEvent {
    pub webview: Entity,
    pub status: u32,
    pub error_code: i32,
    pub error: String,
}

pub type WebviewRendererTerminatedSenderInner =
    async_channel::Sender<WebviewRendererTerminatedEvent>;

pub struct RequestHandlerBuilder {
    object: *mut RcImpl<sys::_cef_request_handler_t, Self>,
    webview: Entity,
    tx: WebviewPopupSenderInner,
    renderer_terminated_tx: WebviewRendererTerminatedSenderInner,
    wake: Option<TextureWake>,
}

impl RequestHandlerBuilder {
    pub fn build(
        webview: Entity,
        tx: WebviewPopupSenderInner,
        renderer_terminated_tx: WebviewRendererTerminatedSenderInner,
        wake: Option<TextureWake>,
    ) -> RequestHandler {
        RequestHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            tx,
            renderer_terminated_tx,
            wake,
        })
    }
}

impl Rc for RequestHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for RequestHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self {
            object,
            webview: self.webview,
            tx: self.tx.clone(),
            renderer_terminated_tx: self.renderer_terminated_tx.clone(),
            wake: self.wake.clone(),
        }
    }
}

impl WrapRequestHandler for RequestHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_request_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplRequestHandler for RequestHandlerBuilder {
    fn on_open_urlfrom_tab(
        &self,
        _browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        target_url: Option<&CefString>,
        _target_disposition: WindowOpenDisposition,
        _user_gesture: c_int,
    ) -> c_int {
        let url = target_url.map(|u| u.to_string()).unwrap_or_default();
        let _ = self.tx.send_blocking(WebviewPopupEvent {
            webview: self.webview,
            target_url: url,
        });
        1
    }

    fn on_render_process_terminated(
        &self,
        _browser: Option<&mut Browser>,
        status: TerminationStatus,
        error_code: c_int,
        error_string: Option<&CefString>,
    ) {
        let error_string = error_string.map(CefString::to_string).unwrap_or_default();
        bevy::log::warn!(
            "CEF renderer terminated for {:?}: status={} error_code={} error={}",
            self.webview,
            status.get_raw(),
            error_code,
            error_string
        );
        let _ = self
            .renderer_terminated_tx
            .send_blocking(WebviewRendererTerminatedEvent {
                webview: self.webview,
                status: status.get_raw(),
                error_code,
                error: error_string,
            });
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    fn get_raw(&self) -> *mut sys::_cef_request_handler_t {
        self.object.cast()
    }
}
