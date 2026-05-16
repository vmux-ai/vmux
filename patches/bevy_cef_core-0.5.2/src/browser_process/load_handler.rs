mod transition;
pub use transition::{CefTransitionCore, CefTransitionQualifiers, decode as decode_transition};

use async_channel::Sender;
use bevy::prelude::Entity;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, Frame, ImplFrame, ImplLoadHandler, LoadHandler, TransitionType, WrapLoadHandler, sys,
};
use std::os::raw::c_int;

#[derive(Clone, Debug)]
pub struct WebviewLoadingStateEvent {
    pub webview: Entity,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub type WebviewLoadingStateSenderInner = Sender<WebviewLoadingStateEvent>;

#[derive(Clone, Debug)]
pub struct WebviewCommittedNavigationEvent {
    pub webview: Entity,
    pub url: String,
    pub is_main_frame: bool,
    pub transition: CefTransitionCore,
    pub qualifiers: CefTransitionQualifiers,
}

pub type WebviewCommittedNavigationSenderInner = Sender<WebviewCommittedNavigationEvent>;

pub struct WebviewLoadHandlerBuilder {
    object: *mut RcImpl<sys::_cef_load_handler_t, Self>,
    webview: Entity,
    tx: WebviewLoadingStateSenderInner,
    nav_tx: WebviewCommittedNavigationSenderInner,
}

impl WebviewLoadHandlerBuilder {
    pub fn build(
        webview: Entity,
        tx: WebviewLoadingStateSenderInner,
        nav_tx: WebviewCommittedNavigationSenderInner,
    ) -> LoadHandler {
        LoadHandler::new(Self {
            object: core::ptr::null_mut(),
            webview,
            tx,
            nav_tx,
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
            nav_tx: self.nav_tx.clone(),
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
        can_go_back: c_int,
        can_go_forward: c_int,
    ) {
        let loading = is_loading != 0;
        let _ = self.tx.send_blocking(WebviewLoadingStateEvent {
            webview: self.webview,
            is_loading: loading,
            can_go_back: can_go_back != 0,
            can_go_forward: can_go_forward != 0,
        });
    }

    fn on_load_start(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        transition_type: TransitionType,
    ) {
        let Some(frame) = frame else { return };
        let is_main_frame = frame.is_main() != 0;
        let url = cef::CefString::from(&frame.url()).to_string();
        let (transition, qualifiers) = decode_transition(transition_type.get_raw());
        let _ = self.nav_tx.send_blocking(WebviewCommittedNavigationEvent {
            webview: self.webview,
            url,
            is_main_frame,
            transition,
            qualifiers,
        });
    }

    fn get_raw(&self) -> *mut sys::_cef_load_handler_t {
        self.object.cast()
    }
}
