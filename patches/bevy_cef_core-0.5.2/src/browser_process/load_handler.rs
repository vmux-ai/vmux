use async_channel::Sender;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{Browser, ImplLoadHandler, LoadHandler, WrapLoadHandler, sys};
use std::os::raw::c_int;

#[derive(Clone, Debug)]
pub struct WebviewLoadingStateEvent {
    pub webview: Entity,
    pub is_loading: bool,
}

pub type WebviewLoadingStateSenderInner = Sender<WebviewLoadingStateEvent>;

/// Forwards CEF [`on_loading_state_change`](https://cef-builds.spotifycdn.com/docs/145.0/classCefLoadHandler.html) to Bevy on the browser process thread.
pub struct WebviewLoadHandlerBuilder {
    object: *mut RcImpl<sys::_cef_load_handler_t, Self>,
    webview: Entity,
    tx: WebviewLoadingStateSenderInner,
}

impl WebviewLoadHandlerBuilder {
    pub fn build(webview: Entity, tx: WebviewLoadingStateSenderInner) -> LoadHandler {
        LoadHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            tx,
        })
    }
}

impl Rc for WebviewLoadHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            core::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for WebviewLoadHandlerBuilder {
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
        }
    }
}

impl WrapLoadHandler for WebviewLoadHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_load_handler_t, Self>) {
        self.object = object;
    }
}

impl ImplLoadHandler for WebviewLoadHandlerBuilder {
    fn on_loading_state_change(
        &self,
        _browser: Option<&mut Browser>,
        is_loading: c_int,
        _can_go_back: c_int,
        _can_go_forward: c_int,
    ) {
        let loading = is_loading != 0;
        let _ = self.tx.send_blocking(WebviewLoadingStateEvent {
            webview: self.webview,
            is_loading: loading,
        });
    }

    fn get_raw(&self) -> *mut sys::_cef_load_handler_t {
        self.object.cast()
    }
}
